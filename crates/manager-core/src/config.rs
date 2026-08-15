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
    #[serde(default)]
    pub network: NetworkConfig,
    #[serde(default)]
    pub storage: StorageConfig,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RouterConfig {
    pub hostname: String,
    pub auth_key_file: PathBuf,
    #[serde(default = "default_tailscale_interface")]
    pub tailscale_interface: String,
    pub guest_interface: String,
    pub lan_address: Ipv4Addr,
    #[serde(default = "default_require_tailnet_lock")]
    pub require_tailnet_lock: bool,
    pub access: Vec<RouterAccess>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub struct RouterAccess {
    pub source: Ipv4Addr,
    pub guest: Ipv4Addr,
}

fn default_tailscale_interface() -> String {
    "tailscale0".into()
}
const fn default_require_tailnet_lock() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct NetworkConfig {
    pub libvirt_bridge: String,
    pub guest_subnet: IpNet,
    pub endpoint_port_start: u16,
    pub endpoint_port_end: u16,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            libvirt_bridge: "vmbr-android".into(),
            guest_subnet: "10.80.0.0/24".parse().expect("valid default guest subnet"),
            endpoint_port_start: 31_000,
            endpoint_port_end: 31_999,
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
        if self.network.endpoint_port_start < 1024
            || self.network.endpoint_port_start > self.network.endpoint_port_end
        {
            return Err(ConfigError::Validation(
                "endpoint port range must be ordered and unprivileged".into(),
            ));
        }
        if self.router.access.is_empty() {
            return Err(ConfigError::Validation(
                "router.access must contain at least one controller-to-guest mapping".into(),
            ));
        }
        let mut mappings = std::collections::HashSet::new();
        for access in &self.router.access {
            let octets = access.source.octets();
            if octets[0] != 100 || !(64..=127).contains(&octets[1]) {
                return Err(ConfigError::Validation(format!(
                    "controller source {} is outside 100.64.0.0/10",
                    access.source
                )));
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
            if !mappings.insert(access) {
                return Err(ConfigError::Validation(format!(
                    "duplicate router access mapping {} -> {}",
                    access.source, access.guest
                )));
            }
        }
        for (name, value) in [
            ("tailscale_interface", &self.router.tailscale_interface),
            ("guest_interface", &self.router.guest_interface),
            ("libvirt_bridge", &self.network.libvirt_bridge),
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn valid() -> Config {
        Config {
            router: RouterConfig {
                hostname: "android-tailnet-router".into(),
                auth_key_file: "/run/secrets/tailscale-authkey".into(),
                tailscale_interface: "tailscale0".into(),
                guest_interface: "ens3".into(),
                lan_address: "10.80.0.1".parse().unwrap(),
                require_tailnet_lock: true,
                access: vec![RouterAccess {
                    source: "100.64.0.2".parse().unwrap(),
                    guest: "10.80.0.2".parse().unwrap(),
                }],
            },
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
        c.router.access[0].source = "192.0.2.1".parse().unwrap();
        assert!(c.validate().is_err());
    }
    #[test]
    fn rejects_guest_outside_subnet() {
        let mut c = valid();
        c.router.access[0].guest = "10.81.0.2".parse().unwrap();
        assert!(c.validate().is_err());
    }
}
