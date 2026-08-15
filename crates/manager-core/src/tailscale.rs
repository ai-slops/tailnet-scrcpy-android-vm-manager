use std::{
    fs,
    net::IpAddr,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Output},
};

use thiserror::Error;

use crate::config::Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Enrollment {
    AlreadyConnected,
    ConnectedAwaitingSignature,
    ConnectedAndSigned,
}

#[derive(Debug, Error)]
pub enum EnrollmentError {
    #[error("could not inspect auth key file {path}: {source}")]
    AuthKeyMetadata {
        path: String,
        source: std::io::Error,
    },
    #[error("auth key path must name a regular file: {0}")]
    AuthKeyNotFile(String),
    #[error("auth key file permissions must be 0600 or stricter, found {mode:04o}: {path}")]
    AuthKeyPermissions { path: String, mode: u32 },
    #[error("tailscale command could not be executed: {0}")]
    Execute(#[from] std::io::Error),
    #[error("tailscale up failed: {0}")]
    Up(String),
    #[error("tailscale connected without the configured host address {0}")]
    UnexpectedAddress(IpAddr),
}

pub trait TailscaleCommand {
    fn output(&self, args: &[String]) -> std::io::Result<Output>;
}

pub struct SystemTailscale;

impl TailscaleCommand for SystemTailscale {
    fn output(&self, args: &[String]) -> std::io::Result<Output> {
        Command::new("tailscale").args(args).output()
    }
}

pub fn enroll(
    config: &Config,
    command: &impl TailscaleCommand,
) -> Result<Enrollment, EnrollmentError> {
    let expected = config.host.tailnet_address;
    if has_address(command, expected)? {
        return Ok(if lock_ready(command)? {
            Enrollment::ConnectedAndSigned
        } else {
            Enrollment::AlreadyConnected
        });
    }

    validate_auth_key_file(&config.tailscale.auth_key_file)?;
    let args = vec![
        "up".into(),
        format!(
            "--auth-key=file:{}",
            config.tailscale.auth_key_file.display()
        ),
        format!("--hostname={}", config.tailscale.hostname),
        "--accept-dns=false".into(),
        "--accept-routes=false".into(),
        "--advertise-routes=".into(),
        "--advertise-exit-node=false".into(),
        "--ssh=false".into(),
        "--report-posture=false".into(),
        "--netfilter-mode=on".into(),
        "--timeout=30s".into(),
        "--reset".into(),
    ];
    let output = command.output(&args)?;
    if !output.status.success() {
        return Err(EnrollmentError::Up(output_text(&output)));
    }
    if !has_address(command, expected)? {
        return Err(EnrollmentError::UnexpectedAddress(expected));
    }
    Ok(if lock_ready(command)? {
        Enrollment::ConnectedAndSigned
    } else {
        Enrollment::ConnectedAwaitingSignature
    })
}

fn has_address(command: &impl TailscaleCommand, expected: IpAddr) -> Result<bool, EnrollmentError> {
    let output = command.output(&["ip".into(), "-4".into()])?;
    Ok(output.status.success()
        && String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| line.trim() == expected.to_string()))
}

fn lock_ready(command: &impl TailscaleCommand) -> Result<bool, EnrollmentError> {
    let output = command.output(&["lock".into(), "status".into()])?;
    let text = output_text(&output);
    Ok(output.status.success()
        && text.contains("Tailnet Lock is ENABLED.")
        && text.contains("This node is accessible under Tailnet Lock."))
}

fn validate_auth_key_file(path: &Path) -> Result<(), EnrollmentError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|source| EnrollmentError::AuthKeyMetadata {
            path: path.display().to_string(),
            source,
        })?;
    if !metadata.file_type().is_file() {
        return Err(EnrollmentError::AuthKeyNotFile(path.display().to_string()));
    }
    let mode = metadata.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
        return Err(EnrollmentError::AuthKeyPermissions {
            path: path.display().to_string(),
            mode,
        });
    }
    Ok(())
}

fn output_text(output: &Output) -> String {
    let mut text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.trim().is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(stderr.trim());
    }
    text
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        fs::Permissions,
        os::unix::{fs::PermissionsExt, process::ExitStatusExt},
        sync::Mutex,
    };

    use crate::config::{HostConfig, NetworkConfig, StorageConfig, TailscaleConfig};

    use super::*;

    struct FakeCommand {
        outputs: Mutex<VecDeque<Output>>,
        calls: Mutex<Vec<Vec<String>>>,
    }

    impl TailscaleCommand for FakeCommand {
        fn output(&self, args: &[String]) -> std::io::Result<Output> {
            self.calls.lock().unwrap().push(args.to_vec());
            Ok(self.outputs.lock().unwrap().pop_front().unwrap())
        }
    }

    fn output(success: bool, stdout: &str) -> Output {
        Output {
            status: std::process::ExitStatus::from_raw(if success { 0 } else { 1 << 8 }),
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
        }
    }

    fn config(auth_key_file: &Path) -> Config {
        Config {
            host: HostConfig {
                tailnet_address: "100.64.0.1".parse().unwrap(),
            },
            tailscale: TailscaleConfig {
                hostname: "android-vm-host".into(),
                auth_key_file: auth_key_file.into(),
                require_tailnet_lock: true,
            },
            network: NetworkConfig::default(),
            storage: StorageConfig::default(),
        }
    }

    #[test]
    fn skips_auth_key_when_already_connected_and_signed() {
        let command = FakeCommand {
            outputs: Mutex::new(VecDeque::from([
                output(true, "100.64.0.1\n"),
                output(
                    true,
                    "Tailnet Lock is ENABLED.\nThis node is accessible under Tailnet Lock.\n",
                ),
            ])),
            calls: Mutex::new(Vec::new()),
        };
        let state = enroll(&config(Path::new("/does/not/need/to/exist")), &command).unwrap();
        assert_eq!(state, Enrollment::ConnectedAndSigned);
        assert_eq!(command.calls.lock().unwrap().len(), 2);
    }

    #[test]
    fn enrolls_with_file_reference_without_reading_key_into_argv() {
        let path =
            std::env::temp_dir().join(format!("tailnet-vm-manager-authkey-{}", std::process::id()));
        fs::write(&path, "tskey-auth-test").unwrap();
        fs::set_permissions(&path, Permissions::from_mode(0o600)).unwrap();
        let command = FakeCommand {
            outputs: Mutex::new(VecDeque::from([
                output(false, ""),
                output(true, ""),
                output(true, "100.64.0.1\n"),
                output(true, "Tailnet Lock is ENABLED.\nLocked out.\n"),
            ])),
            calls: Mutex::new(Vec::new()),
        };

        let state = enroll(&config(&path), &command).unwrap();
        fs::remove_file(&path).unwrap();
        assert_eq!(state, Enrollment::ConnectedAwaitingSignature);
        let calls = command.calls.lock().unwrap();
        assert!(calls[1].contains(&format!("--auth-key=file:{}", path.display())));
        assert!(!calls[1].iter().any(|arg| arg.contains("tskey-auth-test")));
    }
}
