use std::{fs::OpenOptions, path::Path, process::Command};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Pass,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckResult {
    pub name: &'static str,
    pub status: CheckStatus,
    pub detail: String,
}

impl CheckResult {
    #[must_use]
    pub fn passed(&self) -> bool {
        self.status == CheckStatus::Pass
    }
}

pub trait HostProbe {
    fn path_exists(&self, path: &Path) -> bool;
    fn path_read_write(&self, path: &Path) -> bool;
    fn command_succeeds(&self, program: &str, args: &[&str]) -> bool;
}

pub struct SystemProbe;

impl HostProbe for SystemProbe {
    fn path_exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn path_read_write(&self, path: &Path) -> bool {
        OpenOptions::new().read(true).write(true).open(path).is_ok()
    }

    fn command_succeeds(&self, program: &str, args: &[&str]) -> bool {
        Command::new(program)
            .args(args)
            .status()
            .is_ok_and(|status| status.success())
    }
}

#[must_use]
pub fn run(probe: &impl HostProbe) -> Vec<CheckResult> {
    vec![
        kvm_check(probe),
        command_check(probe, "qemu", "qemu-system-x86_64", &["--version"]),
        command_check(probe, "libvirt", "virsh", &["--version"]),
        command_check(probe, "nftables", "nft", &["--version"]),
        path_check(probe, "cgroup-v2", "/sys/fs/cgroup/cgroup.controllers"),
    ]
}

fn kvm_check(probe: &impl HostProbe) -> CheckResult {
    let path = Path::new("/dev/kvm");
    let exists = probe.path_exists(path);
    let accessible = exists && probe.path_read_write(path);
    CheckResult {
        name: "kvm",
        status: if accessible {
            CheckStatus::Pass
        } else {
            CheckStatus::Fail
        },
        detail: if accessible {
            "/dev/kvm is readable and writable by this process".into()
        } else if exists {
            "/dev/kvm exists but is not accessible; start a new login session or run newgrp kvm"
                .into()
        } else {
            "required path /dev/kvm does not exist".into()
        },
    }
}

fn path_check(probe: &impl HostProbe, name: &'static str, path: &str) -> CheckResult {
    let passed = probe.path_exists(Path::new(path));
    CheckResult {
        name,
        status: if passed {
            CheckStatus::Pass
        } else {
            CheckStatus::Fail
        },
        detail: if passed {
            format!("{path} is available")
        } else {
            format!("required path {path} does not exist")
        },
    }
}

fn command_check(
    probe: &impl HostProbe,
    name: &'static str,
    program: &str,
    args: &[&str],
) -> CheckResult {
    let passed = probe.command_succeeds(program, args);
    CheckResult {
        name,
        status: if passed {
            CheckStatus::Pass
        } else {
            CheckStatus::Fail
        },
        detail: if passed {
            format!("{program} is available")
        } else {
            format!("{program} is missing or could not execute")
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MissingProbe;

    impl HostProbe for MissingProbe {
        fn path_exists(&self, _path: &Path) -> bool {
            false
        }
        fn path_read_write(&self, _path: &Path) -> bool {
            false
        }
        fn command_succeeds(&self, _program: &str, _args: &[&str]) -> bool {
            false
        }
    }

    struct InaccessibleKvmProbe;

    impl HostProbe for InaccessibleKvmProbe {
        fn path_exists(&self, path: &Path) -> bool {
            path == Path::new("/dev/kvm")
        }
        fn path_read_write(&self, _path: &Path) -> bool {
            false
        }
        fn command_succeeds(&self, _program: &str, _args: &[&str]) -> bool {
            false
        }
    }

    #[test]
    fn reports_stale_kvm_group_session() {
        let result = kvm_check(&InaccessibleKvmProbe);
        assert!(!result.passed());
        assert!(result.detail.contains("newgrp kvm"));
    }

    #[test]
    fn reports_missing_dependencies() {
        let results = run(&MissingProbe);
        assert!(results.iter().all(|result| !result.passed()));
        assert!(results.iter().any(|result| result.name == "qemu"));
    }
}
