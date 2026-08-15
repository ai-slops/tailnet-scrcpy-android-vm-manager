use std::{
    fs,
    net::{IpAddr, Ipv4Addr},
    path::{Path, PathBuf},
};

use ipnet::IpNet;
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub router: RouterConfig,
    pub android_vms: Vec<AndroidVmConfig>,
    #[serde(default)]
    pub network: NetworkConfig,
    #[serde(default)]
    pub storage: StorageConfig,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AndroidVmConfig {
    pub name: String,
    pub address: Ipv4Addr,
    #[serde(default)]
    pub adb_public_key_files: Vec<PathBuf>,
    #[serde(default = "default_vm_vcpus")]
    pub vcpus: u16,
    #[serde(default = "default_vm_memory_mib")]
    pub memory_mib: u32,
}

const fn default_vm_vcpus() -> u16 {
    4
}
const fn default_vm_memory_mib() -> u32 {
    4096
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RouterConfig {
    pub hostname: String,
    pub auth_key_file: PathBuf,
    #[serde(default = "default_tailscale_interface")]
    pub tailscale_interface: String,
    pub uplink_interface: String,
    pub guest_interface: String,
    pub lan_address: Ipv4Addr,
    pub access: Vec<RouterAccess>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub struct RouterAccess {
    pub sources: Vec<Ipv4Addr>,
    pub guest: Ipv4Addr,
}

fn default_tailscale_interface() -> String {
    "tailscale0".into()
}
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct NetworkConfig {
    pub router_uplink_network: String,
    pub guest_network: String,
    pub guest_bridge: String,
    pub guest_subnet: IpNet,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            router_uplink_network: "default".into(),
            guest_network: "tailnet-android-guest".into(),
            guest_bridge: "vmbr-android".into(),
            guest_subnet: "10.80.0.0/24".parse().expect("valid default guest subnet"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct StorageConfig {
    pub state_dir: PathBuf,
    pub image_dir: PathBuf,
    pub vm_dir: PathBuf,
}

impl Default for StorageConfig {
    fn default() -> Self {
        let root = PathBuf::from("/var/lib/tailnet-android-vm-manager");
        Self {
            state_dir: root.clone(),
            image_dir: root.join("images"),
            vm_dir: root.join("vms"),
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("could not read configuration {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid configuration {path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("invalid configuration: {0}")]
    Validation(String),
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let contents = fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.into(),
            source,
        })?;
        let config: Self = toml::from_str(&contents).map_err(|source| ConfigError::Parse {
            path: path.into(),
            source,
        })?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        let guest_network = self.network.guest_subnet.network();
        if !matches!(guest_network, IpAddr::V4(address) if address.is_private() && !address.is_loopback())
        {
            return Err(ConfigError::Validation(
                "network.guest_subnet must be a private non-loopback IPv4 subnet".into(),
            ));
        }
        if self.router.hostname.is_empty()
            || self.router.hostname.len() > 63
            || !self
                .router
                .hostname
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
        {
            return Err(ConfigError::Validation(
                "router.hostname is not a valid DNS-style hostname".into(),
            ));
        }
        if !self.router.auth_key_file.is_absolute() {
            return Err(ConfigError::Validation(
                "router.auth_key_file must be absolute".into(),
            ));
        }
        if !self
            .network
            .guest_subnet
            .contains(&IpAddr::V4(self.router.lan_address))
        {
            return Err(ConfigError::Validation(
                "router.lan_address must be inside network.guest_subnet".into(),
            ));
        }
        if self.router.access.is_empty() {
            return Err(ConfigError::Validation(
                "router.access must contain at least one controller-to-guest mapping".into(),
            ));
        }
        if self.android_vms.is_empty() {
            return Err(ConfigError::Validation(
                "android_vms must contain at least one persistent VM".into(),
            ));
        }
        let mut vm_names = std::collections::HashSet::new();
        let mut vm_addresses = std::collections::HashSet::new();
        for vm in &self.android_vms {
            if !valid_identifier(&vm.name) {
                return Err(ConfigError::Validation(format!(
                    "Android VM name {} is invalid",
                    vm.name
                )));
            }
            if !self.network.guest_subnet.contains(&IpAddr::V4(vm.address)) {
                return Err(ConfigError::Validation(format!(
                    "Android VM {} address {} is outside network.guest_subnet",
                    vm.name, vm.address
                )));
            }
            if !vm_names.insert(&vm.name) || !vm_addresses.insert(vm.address) {
                return Err(ConfigError::Validation(
                    "Android VM names and addresses must be unique".into(),
                ));
            }
            if vm.vcpus == 0 || vm.vcpus > 64 || vm.memory_mib < 512 {
                return Err(ConfigError::Validation(format!(
                    "Android VM {} has invalid vcpus or memory_mib",
                    vm.name
                )));
            }
            for path in &vm.adb_public_key_files {
                if !path.is_absolute() {
                    return Err(ConfigError::Validation(format!(
                        "Android VM {} ADB public-key paths must be absolute",
                        vm.name
                    )));
                }
            }
        }
        let mut mappings = std::collections::HashSet::new();
        for access in &self.router.access {
            if access.sources.is_empty() {
                return Err(ConfigError::Validation(format!(
                    "router access for {} must contain at least one source",
                    access.guest
                )));
            }
            for source in &access.sources {
                let octets = source.octets();
                if octets[0] != 100 || !(64..=127).contains(&octets[1]) {
                    return Err(ConfigError::Validation(format!(
                        "controller source {source} is outside 100.64.0.0/10"
                    )));
                }
                if !mappings.insert((*source, access.guest)) {
                    return Err(ConfigError::Validation(format!(
                        "duplicate router access mapping {source} -> {}",
                        access.guest
                    )));
                }
            }
            if !self
                .network
                .guest_subnet
                .contains(&IpAddr::V4(access.guest))
            {
                return Err(ConfigError::Validation(format!(
                    "routed Android guest {} is outside network.guest_subnet",
                    access.guest
                )));
            }
            if !vm_addresses.contains(&access.guest) {
                return Err(ConfigError::Validation(format!(
                    "router access guest {} is not a configured Android VM",
                    access.guest
                )));
            }
        }
        for (name, value) in [
            ("tailscale_interface", &self.router.tailscale_interface),
            ("uplink_interface", &self.router.uplink_interface),
            ("guest_interface", &self.router.guest_interface),
            ("guest_bridge", &self.network.guest_bridge),
        ] {
            if value.is_empty()
                || value.len() > 15
                || !value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b))
            {
                return Err(ConfigError::Validation(format!(
                    "{name} is not a valid Linux interface name"
                )));
            }
        }
        for (name, value) in [
            ("router_uplink_network", &self.network.router_uplink_network),
            ("guest_network", &self.network.guest_network),
        ] {
            if !valid_identifier(value) {
                return Err(ConfigError::Validation(format!(
                    "network.{name} is not a valid libvirt network name"
                )));
            }
        }
        for (name, path) in [
            ("state_dir", &self.storage.state_dir),
            ("image_dir", &self.storage.image_dir),
            ("vm_dir", &self.storage.vm_dir),
        ] {
            if !path.is_absolute() {
                return Err(ConfigError::Validation(format!(
                    "storage.{name} must be absolute"
                )));
            }
        }
        Ok(())
    }
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 63
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn valid() -> Config {
        Config {
            router: RouterConfig {
                hostname: "android-tailnet-router".into(),
                auth_key_file: "/run/secrets/tailscale-authkey".into(),
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
                address: "10.80.0.2".parse().unwrap(),
                adb_public_key_files: vec![],
                vcpus: 4,
                memory_mib: 4096,
            }],
            network: NetworkConfig::default(),
            storage: StorageConfig::default(),
        }
    }
    #[test]
    fn accepts_valid_config() {
        valid().validate().unwrap();
    }
    #[test]
    fn rejects_public_guest_subnet() {
        let mut c = valid();
        c.network.guest_subnet = "192.0.2.0/24".parse().unwrap();
        assert!(c.validate().is_err());
    }
    #[test]
    fn rejects_relative_storage() {
        let mut c = valid();
        c.storage.vm_dir = "vms".into();
        assert!(c.validate().is_err());
    }
    #[test]
    fn rejects_empty_access() {
        let mut c = valid();
        c.router.access.clear();
        assert!(c.validate().is_err());
    }
    #[test]
    fn rejects_non_tailnet_source() {
        let mut c = valid();
        c.router.access[0].sources[0] = "192.0.2.1".parse().unwrap();
        assert!(c.validate().is_err());
    }
    #[test]
    fn rejects_guest_outside_subnet() {
        let mut c = valid();
        c.router.access[0].guest = "10.81.0.2".parse().unwrap();
        assert!(c.validate().is_err());
    }

    #[test]
    fn rejects_uninventoried_router_guest() {
        let mut c = valid();
        c.router.access[0].guest = "10.80.0.3".parse().unwrap();
        assert!(c.validate().is_err());
    }

    #[test]
    fn rejects_duplicate_or_unsafe_vm_names() {
        let mut c = valid();
        c.android_vms[0].name = "../../domain".into();
        assert!(c.validate().is_err());
        let mut c = valid();
        c.android_vms.push(c.android_vms[0].clone());
        assert!(c.validate().is_err());
    }
}
