use std::{path::Path, process::Command};

use crate::config::Config;

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
    fn command_succeeds(&self, program: &str, args: &[&str]) -> bool;
    fn interface_has_address(&self, interface: &str, address: &str) -> bool;
}

pub struct SystemProbe;

impl HostProbe for SystemProbe {
    fn path_exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn command_succeeds(&self, program: &str, args: &[&str]) -> bool {
        Command::new(program)
            .args(args)
            .status()
            .is_ok_and(|status| status.success())
    }

    fn interface_has_address(&self, interface: &str, address: &str) -> bool {
        Command::new("ip")
            .args(["-brief", "address", "show", "dev", interface])
            .output()
            .is_ok_and(|output| {
                output.status.success()
                    && String::from_utf8_lossy(&output.stdout)
                        .split_ascii_whitespace()
                        .any(|field| field.split('/').next() == Some(address))
            })
    }
}

#[must_use]
pub fn run(config: &Config, probe: &impl HostProbe) -> Vec<CheckResult> {
    let mut results = vec![
        path_check(probe, "kvm", "/dev/kvm"),
        command_check(probe, "qemu", "qemu-system-x86_64", &["--version"]),
        command_check(probe, "libvirt", "virsh", &["--version"]),
        command_check(probe, "nftables", "nft", &["--version"]),
        command_check(probe, "tailscale", "tailscale", &["version"]),
        path_check(probe, "cgroup-v2", "/sys/fs/cgroup/cgroup.controllers"),
    ];
    let address = config.host.tailnet_address.to_string();
    let present = probe.interface_has_address(&config.network.tailscale_interface, &address);
    results.push(CheckResult {
        name: "tailnet-address",
        status: if present {
            CheckStatus::Pass
        } else {
            CheckStatus::Fail
        },
        detail: if present {
            format!(
                "{address} is assigned to {}",
                config.network.tailscale_interface
            )
        } else {
            format!(
                "{address} is not assigned to {}",
                config.network.tailscale_interface
            )
        },
    });
    results
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
    use crate::config::{HostConfig, NetworkConfig, StorageConfig};

    struct MissingProbe;

    impl HostProbe for MissingProbe {
        fn path_exists(&self, _path: &Path) -> bool {
            false
        }
        fn command_succeeds(&self, _program: &str, _args: &[&str]) -> bool {
            false
        }
        fn interface_has_address(&self, _interface: &str, _address: &str) -> bool {
            false
        }
    }

    #[test]
    fn reports_missing_dependencies() {
        let config = Config {
            host: HostConfig {
                tailnet_address: "100.64.0.1".parse().unwrap(),
            },
            network: NetworkConfig::default(),
            storage: StorageConfig::default(),
        };
        let results = run(&config, &MissingProbe);
        assert!(results.iter().all(|result| !result.passed()));
        assert!(
            results
                .iter()
                .any(|result| result.name == "tailnet-address")
        );
    }
}
