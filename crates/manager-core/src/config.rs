use std::{
    fs,
    net::IpAddr,
    path::{Path, PathBuf},
};

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub host: HostConfig,
    #[serde(default)]
    pub network: NetworkConfig,
    #[serde(default)]
    pub storage: StorageConfig,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HostConfig {
    pub tailnet_address: IpAddr,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct NetworkConfig {
    pub tailscale_interface: String,
    pub libvirt_bridge: String,
    pub endpoint_port_start: u16,
    pub endpoint_port_end: u16,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            tailscale_interface: "tailscale0".into(),
            libvirt_bridge: "vmbr-android".into(),
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
        let octets = match self.host.tailnet_address {
            IpAddr::V4(value) => value.octets(),
            IpAddr::V6(_) => [0; 4],
        };
        if octets[0] != 100 || !(64..=127).contains(&octets[1]) {
            return Err(ConfigError::Validation(
                "host.tailnet_address must be in 100.64.0.0/10".into(),
            ));
        }
        if self.network.endpoint_port_start < 1024
            || self.network.endpoint_port_start > self.network.endpoint_port_end
        {
            return Err(ConfigError::Validation(
                "endpoint port range must be ordered and unprivileged".into(),
            ));
        }
        for (name, value) in [
            ("tailscale_interface", &self.network.tailscale_interface),
            ("libvirt_bridge", &self.network.libvirt_bridge),
        ] {
            if value.is_empty()
                || value.len() > 15
                || !value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"_-.".contains(&byte))
            {
                return Err(ConfigError::Validation(format!(
                    "network.{name} is not a valid Linux interface name"
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
mod tests {
    use super::*;

    fn valid() -> Config {
        Config {
            host: HostConfig {
                tailnet_address: "100.64.0.1".parse().unwrap(),
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
    fn rejects_non_tailnet_address() {
        let mut config = valid();
        config.host.tailnet_address = "192.0.2.1".parse().unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_relative_storage() {
        let mut config = valid();
        config.storage.vm_dir = "vms".into();
        assert!(config.validate().is_err());
    }
}
