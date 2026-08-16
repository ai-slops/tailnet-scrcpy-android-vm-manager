use thiserror::Error;

use crate::{
    config::AndroidVmConfig,
    lifecycle::{self, LifecycleError, Virsh, VmState},
};

#[derive(Debug, Error)]
pub enum SnapshotError {
    #[error(transparent)]
    Lifecycle(#[from] LifecycleError),
    #[error("invalid snapshot name: {0}")]
    InvalidName(String),
    #[error("snapshot {operation} requires Stopped state; {vm} is {state:?}")]
    State {
        operation: &'static str,
        vm: String,
        state: VmState,
    },
}

pub fn create(virsh: &impl Virsh, vm: &AndroidVmConfig, name: &str) -> Result<(), SnapshotError> {
    validate_name(name)?;
    require_stopped(virsh, vm, "create")?;
    lifecycle::run(
        virsh,
        "snapshot-create-as",
        &[
            "snapshot-create-as",
            "--domain",
            &vm.name,
            "--name",
            name,
            "--description",
            "managed by tailnet-android-vm-manager",
            "--atomic",
        ],
    )?;
    Ok(())
}

pub fn list(virsh: &impl Virsh, vm: &AndroidVmConfig) -> Result<Vec<String>, SnapshotError> {
    let output = lifecycle::run(
        virsh,
        "snapshot-list",
        &["snapshot-list", &vm.name, "--name"],
    )?;
    Ok(output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect())
}

pub fn revert(virsh: &impl Virsh, vm: &AndroidVmConfig, name: &str) -> Result<(), SnapshotError> {
    validate_name(name)?;
    require_stopped(virsh, vm, "revert")?;
    lifecycle::run(
        virsh,
        "snapshot-revert",
        &["snapshot-revert", &vm.name, name],
    )?;
    Ok(())
}

pub fn delete(virsh: &impl Virsh, vm: &AndroidVmConfig, name: &str) -> Result<(), SnapshotError> {
    validate_name(name)?;
    require_stopped(virsh, vm, "delete")?;
    lifecycle::run(
        virsh,
        "snapshot-delete",
        &["snapshot-delete", &vm.name, name],
    )?;
    Ok(())
}

fn require_stopped(
    virsh: &impl Virsh,
    vm: &AndroidVmConfig,
    operation: &'static str,
) -> Result<(), SnapshotError> {
    let state = lifecycle::state(virsh, vm)?;
    if state != VmState::Stopped {
        return Err(SnapshotError::State {
            operation,
            vm: vm.name.clone(),
            state,
        });
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<(), SnapshotError> {
    if name.is_empty()
        || name.len() > 63
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(SnapshotError::InvalidName(name.into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque, os::unix::process::ExitStatusExt, process::Output, sync::Mutex,
    };

    use crate::lifecycle::Virsh;

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
            labels: vec!["game".into()],
            address: "10.80.0.2".parse().unwrap(),
            base_image: "/var/lib/tailnet-android-vm-manager/images/android-base.qcow2".into(),
            controllers: vec![],
            vcpus: 4,
            memory_mib: 4096,
            autostart: false,
        }
    }

    #[test]
    fn creates_atomic_snapshot_only_while_stopped() {
        let fake = Fake {
            outputs: Mutex::new(VecDeque::from([
                output("shut off\n"),
                output("Managed save: no\n"),
                output(""),
            ])),
            calls: Mutex::new(vec![]),
        };
        create(&fake, &vm(), "before-update").unwrap();
        assert_eq!(
            fake.calls.lock().unwrap()[2],
            [
                "snapshot-create-as",
                "--domain",
                "android-game-01",
                "--name",
                "before-update",
                "--description",
                "managed by tailnet-android-vm-manager",
                "--atomic"
            ]
        );
    }

    #[test]
    fn rejects_hibernated_snapshot_to_prevent_ram_disk_mismatch() {
        let fake = Fake {
            outputs: Mutex::new(VecDeque::from([
                output("shut off\n"),
                output("Managed save: yes\n"),
            ])),
            calls: Mutex::new(vec![]),
        };
        assert!(matches!(
            revert(&fake, &vm(), "clean"),
            Err(SnapshotError::State {
                state: VmState::Hibernated,
                ..
            })
        ));
    }

    #[test]
    fn rejects_snapshot_name_injection() {
        let fake = Fake {
            outputs: Mutex::new(VecDeque::new()),
            calls: Mutex::new(vec![]),
        };
        assert!(matches!(
            delete(&fake, &vm(), "../../disk"),
            Err(SnapshotError::InvalidName(_))
        ));
        assert!(fake.calls.lock().unwrap().is_empty());
    }
}
