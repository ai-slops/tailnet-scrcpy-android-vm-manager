use crate::config::Config;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Output},
};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Enrollment {
    AlreadyConnected,
    Connected,
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
    if is_connected(command)? {
        reconfigure(config, command)?;
        return Ok(Enrollment::AlreadyConnected);
    }
    validate_auth_key_file(&config.router.auth_key_file)?;
    let args = vec![
        "up".into(),
        format!("--auth-key=file:{}", config.router.auth_key_file.display()),
        format!("--hostname={}", config.router.hostname),
        "--accept-dns=false".into(),
        "--accept-routes=false".into(),
        format!("--advertise-routes={}", advertised_routes(config)),
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
    Ok(Enrollment::Connected)
}

pub fn reconfigure(
    config: &Config,
    command: &impl TailscaleCommand,
) -> Result<(), EnrollmentError> {
    let args = vec![
        "set".into(),
        format!("--advertise-routes={}", advertised_routes(config)),
        "--accept-dns=false".into(),
        "--accept-routes=false".into(),
        "--advertise-exit-node=false".into(),
    ];
    let output = command.output(&args)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(EnrollmentError::Up(output_text(&output)))
    }
}

#[must_use]
pub fn advertised_routes(config: &Config) -> String {
    let mut guests = config
        .router
        .access
        .iter()
        .map(|access| access.guest)
        .collect::<Vec<_>>();
    guests.sort_unstable();
    guests.dedup();
    guests
        .into_iter()
        .map(|guest| format!("{guest}/32"))
        .collect::<Vec<_>>()
        .join(",")
}

fn is_connected(command: &impl TailscaleCommand) -> Result<bool, EnrollmentError> {
    let output = command.output(&["ip".into(), "-4".into()])?;
    Ok(output.status.success()
        && String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| !line.trim().is_empty()))
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
    use super::*;
    use crate::config::{
        AndroidVmConfig, NetworkConfig, RouterAccess, RouterConfig, StorageConfig,
    };
    use std::{
        collections::VecDeque,
        fs::Permissions,
        os::unix::{fs::PermissionsExt, process::ExitStatusExt},
        sync::Mutex,
    };
    struct Fake {
        outputs: Mutex<VecDeque<Output>>,
        calls: Mutex<Vec<Vec<String>>>,
    }
    impl TailscaleCommand for Fake {
        fn output(&self, args: &[String]) -> std::io::Result<Output> {
            self.calls.lock().unwrap().push(args.to_vec());
            Ok(self.outputs.lock().unwrap().pop_front().unwrap())
        }
    }
    fn out(ok: bool, s: &str) -> Output {
        Output {
            status: std::process::ExitStatus::from_raw(if ok { 0 } else { 1 << 8 }),
            stdout: s.as_bytes().to_vec(),
            stderr: vec![],
        }
    }
    fn config(path: &Path) -> Config {
        Config {
            router: RouterConfig {
                hostname: "android-tailnet-router".into(),
                auth_key_file: path.into(),
                tailscale_interface: "tailscale0".into(),
                uplink_interface: "ens3".into(),
                guest_interface: "ens4".into(),
                lan_address: "10.80.0.1".parse().unwrap(),
                access: vec![RouterAccess {
                    sources: vec!["100.64.0.2".parse().unwrap()],
                    guest: "10.80.0.2".parse().unwrap(),
                }],
            },
            android_vms: vec![AndroidVmConfig {
                name: "android-game-01".into(),
                labels: vec!["game".into()],
                address: "10.80.0.2".parse().unwrap(),
                base_image: "/var/lib/tailnet-android-vm-manager/images/android-base.qcow2".into(),
                adb_public_key_files: vec![],
                vcpus: 4,
                memory_mib: 4096,
                autostart: false,
            }],
            network: NetworkConfig::default(),
            storage: StorageConfig::default(),
        }
    }
    #[test]
    fn enrolls_and_advertises_only_guest_32s() {
        let p = std::env::temp_dir().join(format!("router-auth-{}", std::process::id()));
        fs::write(&p, "secret").unwrap();
        fs::set_permissions(&p, Permissions::from_mode(0o600)).unwrap();
        let f = Fake {
            outputs: Mutex::new(VecDeque::from([out(false, ""), out(true, "")])),
            calls: Mutex::new(vec![]),
        };
        assert_eq!(enroll(&config(&p), &f).unwrap(), Enrollment::Connected);
        fs::remove_file(&p).unwrap();
        let calls = f.calls.lock().unwrap();
        assert!(calls[1].contains(&"--advertise-routes=10.80.0.2/32".into()));
        assert!(!calls[1].iter().any(|a| a.contains("secret")));
    }

    #[test]
    fn reconfigures_routes_without_auth_key() {
        let f = Fake {
            outputs: Mutex::new(VecDeque::from([out(true, "")])),
            calls: Mutex::new(vec![]),
        };
        reconfigure(&config(Path::new("/missing")), &f).unwrap();
        let calls = f.calls.lock().unwrap();
        assert_eq!(calls[0][0], "set");
        assert!(calls[0].contains(&"--advertise-routes=10.80.0.2/32".into()));
        assert!(
            !calls[0]
                .iter()
                .any(|argument| argument.contains("auth-key"))
        );
    }
}
