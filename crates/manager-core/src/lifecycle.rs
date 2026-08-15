use std::{
    net::{SocketAddr, TcpStream},
    process::{Command, Output},
    thread,
    time::{Duration, Instant},
};

use thiserror::Error;

use crate::config::{AndroidVmConfig, Config};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmState {
    Running,
    Stopped,
    Hibernated,
}

#[derive(Debug, Error)]
pub enum LifecycleError {
    #[error("Android VM is not configured: {0}")]
    UnknownVm(String),
    #[error("could not execute virsh: {0}")]
    Execute(#[from] std::io::Error),
    #[error("virsh {operation} failed: {detail}")]
    Virsh {
        operation: &'static str,
        detail: String,
    },
    #[error("unsupported libvirt state for {name}: {state}")]
    UnsupportedState { name: String, state: String },
    #[error("cannot hibernate stopped VM {0}")]
    HibernateStopped(String),
    #[error("timed out waiting for VM {0} to stop")]
    StopTimeout(String),
    #[error("timed out waiting for ADB readiness at {0}:5555")]
    ReadinessTimeout(std::net::Ipv4Addr),
}

pub trait Virsh {
    fn output(&self, args: &[String]) -> std::io::Result<Output>;
}

pub struct SystemVirsh;

impl Virsh for SystemVirsh {
    fn output(&self, args: &[String]) -> std::io::Result<Output> {
        Command::new("virsh").env("LC_ALL", "C").args(args).output()
    }
}

pub fn find_vm<'a>(config: &'a Config, name: &str) -> Result<&'a AndroidVmConfig, LifecycleError> {
    config
        .android_vms
        .iter()
        .find(|vm| vm.name == name)
        .ok_or_else(|| LifecycleError::UnknownVm(name.into()))
}

pub fn state(virsh: &impl Virsh, vm: &AndroidVmConfig) -> Result<VmState, LifecycleError> {
    let raw = run(virsh, "domstate", &["domstate", &vm.name])?;
    match raw.trim() {
        "running" | "idle" | "blocked" => Ok(VmState::Running),
        "shut off" => {
            let info = run(virsh, "dominfo", &["dominfo", &vm.name])?;
            if info.lines().any(|line| {
                line.split_once(':').is_some_and(|(key, value)| {
                    key.trim() == "Managed save" && value.trim() == "yes"
                })
            }) {
                Ok(VmState::Hibernated)
            } else {
                Ok(VmState::Stopped)
            }
        }
        other => Err(LifecycleError::UnsupportedState {
            name: vm.name.clone(),
            state: other.into(),
        }),
    }
}

pub fn start(virsh: &impl Virsh, vm: &AndroidVmConfig) -> Result<VmState, LifecycleError> {
    match state(virsh, vm)? {
        VmState::Running => Ok(VmState::Running),
        VmState::Stopped | VmState::Hibernated => {
            run(virsh, "start", &["start", &vm.name])?;
            Ok(VmState::Running)
        }
    }
}

pub fn hibernate(virsh: &impl Virsh, vm: &AndroidVmConfig) -> Result<VmState, LifecycleError> {
    match state(virsh, vm)? {
        VmState::Hibernated => Ok(VmState::Hibernated),
        VmState::Stopped => Err(LifecycleError::HibernateStopped(vm.name.clone())),
        VmState::Running => {
            run(
                virsh,
                "managedsave",
                &["managedsave", &vm.name, "--running"],
            )?;
            Ok(VmState::Hibernated)
        }
    }
}

pub fn stop(
    virsh: &impl Virsh,
    vm: &AndroidVmConfig,
    timeout: Duration,
    force_after_timeout: bool,
) -> Result<VmState, LifecycleError> {
    match state(virsh, vm)? {
        VmState::Stopped => return Ok(VmState::Stopped),
        VmState::Hibernated => {
            run(
                virsh,
                "managedsave-remove",
                &["managedsave-remove", &vm.name],
            )?;
            return Ok(VmState::Stopped);
        }
        VmState::Running => {
            run(virsh, "shutdown", &["shutdown", &vm.name, "--mode", "acpi"])?;
        }
    }
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if state(virsh, vm)? == VmState::Stopped {
            return Ok(VmState::Stopped);
        }
        thread::sleep(Duration::from_millis(250));
    }
    if force_after_timeout {
        run(virsh, "destroy", &["destroy", &vm.name])?;
        Ok(VmState::Stopped)
    } else {
        Err(LifecycleError::StopTimeout(vm.name.clone()))
    }
}

pub fn wait_for_adb(vm: &AndroidVmConfig, timeout: Duration) -> Result<(), LifecycleError> {
    let address = SocketAddr::new(vm.address.into(), 5555);
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if TcpStream::connect_timeout(&address, Duration::from_millis(500)).is_ok() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(250));
    }
    Err(LifecycleError::ReadinessTimeout(vm.address))
}

pub(crate) fn run(
    virsh: &impl Virsh,
    operation: &'static str,
    args: &[&str],
) -> Result<String, LifecycleError> {
    let args = args.iter().map(|value| (*value).into()).collect::<Vec<_>>();
    let output = virsh.output(&args)?;
    if !output.status.success() {
        return Err(LifecycleError::Virsh {
            operation,
            detail: output_text(&output),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn output_text(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    format!("{}{}", stdout.trim(), stderr.trim()).trim().into()
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, os::unix::process::ExitStatusExt, sync::Mutex};

    use super::*;

    struct Fake {
        outputs: Mutex<VecDeque<Output>>,
        calls: Mutex<Vec<Vec<String>>>,
    }
    impl Virsh for Fake {
        fn output(&self, args: &[String]) -> std::io::Result<Output> {
            self.calls.lock().unwrap().push(args.to_vec());
            Ok(self.outputs.lock().unwrap().pop_front().unwrap())
        }
    }
    fn output(text: &str) -> Output {
        Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout: text.as_bytes().to_vec(),
            stderr: vec![],
        }
    }
    fn vm() -> AndroidVmConfig {
        AndroidVmConfig {
            name: "android-game-01".into(),
            address: "10.80.0.2".parse().unwrap(),
            adb_public_key_files: vec![],
            vcpus: 4,
            memory_mib: 4096,
        }
    }

    #[test]
    fn detects_ssd_backed_hibernation() {
        let fake = Fake {
            outputs: Mutex::new(VecDeque::from([
                output("shut off\n"),
                output("Managed save: yes\n"),
            ])),
            calls: Mutex::new(vec![]),
        };
        assert_eq!(state(&fake, &vm()).unwrap(), VmState::Hibernated);
    }

    #[test]
    fn hibernates_running_vm_with_managed_save() {
        let fake = Fake {
            outputs: Mutex::new(VecDeque::from([output("running\n"), output("")])),
            calls: Mutex::new(vec![]),
        };
        assert_eq!(hibernate(&fake, &vm()).unwrap(), VmState::Hibernated);
        assert_eq!(
            fake.calls.lock().unwrap()[1],
            ["managedsave", "android-game-01", "--running"]
        );
    }

    #[test]
    fn stopping_hibernated_vm_discards_saved_ram() {
        let fake = Fake {
            outputs: Mutex::new(VecDeque::from([
                output("shut off\n"),
                output("Managed save: yes\n"),
                output(""),
            ])),
            calls: Mutex::new(vec![]),
        };
        assert_eq!(
            stop(&fake, &vm(), Duration::ZERO, false).unwrap(),
            VmState::Stopped
        );
        assert_eq!(
            fake.calls.lock().unwrap()[2],
            ["managedsave-remove", "android-game-01"]
        );
    }
}
