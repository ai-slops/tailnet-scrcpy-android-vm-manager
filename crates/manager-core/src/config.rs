use std::{
    collections::{BTreeMap, HashSet},
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
    pub controllers: BTreeMap<String, ControllerConfig>,
    pub vms: BTreeMap<String, AndroidVmConfig>,
    #[serde(default)]
    pub network: NetworkConfig,
    #[serde(default)]
    pub storage: StorageConfig,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AndroidVmConfig {
    #[serde(skip)]
    pub name: String,
    #[serde(default)]
    pub labels: Vec<String>,
    pub address: Ipv4Addr,
    pub base_image: PathBuf,
    #[serde(default)]
    pub controllers: Vec<String>,
    #[serde(default = "default_vm_vcpus")]
    pub vcpus: u16,
    #[serde(default = "default_vm_memory_mib")]
    pub memory_mib: u32,
    #[serde(default)]
    pub autostart: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ControllerConfig {
    #[serde(skip)]
    pub name: String,
    pub sources: Vec<Ipv4Addr>,
    pub adb_public_key_file: PathBuf,
    #[serde(default = "default_true")]
    pub active: bool,
}

const fn default_true() -> bool {
    true
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
        let mut config: Self = toml::from_str(&contents).map_err(|source| ConfigError::Parse {
            path: path.into(),
            source,
        })?;
        for (name, controller) in &mut config.controllers {
            controller.name.clone_from(name);
        }
        for (name, vm) in &mut config.vms {
            vm.name.clone_from(name);
        }
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
        if self.controllers.is_empty() {
            return Err(ConfigError::Validation(
                "controllers must contain at least one controller".into(),
            ));
        }
        if self.vms.is_empty() {
            return Err(ConfigError::Validation(
                "vms must contain at least one persistent VM".into(),
            ));
        }
        let mut controller_sources = HashSet::new();
        for (name, controller) in &self.controllers {
            if !valid_identifier(name) || controller.name != *name {
                return Err(ConfigError::Validation(format!(
                    "controller name {name} is invalid or inconsistent"
                )));
            }
            if controller.active && controller.sources.is_empty() {
                return Err(ConfigError::Validation(format!(
                    "active controller {name} must contain at least one source"
                )));
            }
            if !controller.adb_public_key_file.is_absolute() {
                return Err(ConfigError::Validation(format!(
                    "controller {name} ADB public-key path must be absolute"
                )));
            }
            for source in &controller.sources {
                let octets = source.octets();
                if octets[0] != 100 || !(64..=127).contains(&octets[1]) {
                    return Err(ConfigError::Validation(format!(
                        "controller source {source} is outside 100.64.0.0/10"
                    )));
                }
                if controller.active && !controller_sources.insert(*source) {
                    return Err(ConfigError::Validation(format!(
                        "active controller source {source} is duplicated"
                    )));
                }
            }
        }
        let mut vm_addresses = HashSet::new();
        for (name, vm) in &self.vms {
            if !valid_identifier(name) || vm.name != *name {
                return Err(ConfigError::Validation(format!(
                    "Android VM name {name} is invalid or inconsistent"
                )));
            }
            if !self.network.guest_subnet.contains(&IpAddr::V4(vm.address)) {
                return Err(ConfigError::Validation(format!(
                    "Android VM {} address {} is outside network.guest_subnet",
                    vm.name, vm.address
                )));
            }
            if !vm_addresses.insert(vm.address) {
                return Err(ConfigError::Validation(
                    "Android VM addresses must be unique".into(),
                ));
            }
            let mut labels = std::collections::HashSet::new();
            for label in &vm.labels {
                if !valid_identifier(label) || !labels.insert(label) {
                    return Err(ConfigError::Validation(format!(
                        "Android VM {} labels must be unique valid identifiers",
                        vm.name
                    )));
                }
            }
            if !vm.base_image.is_absolute() {
                return Err(ConfigError::Validation(format!(
                    "Android VM {} base_image must be absolute",
                    vm.name
                )));
            }
            if vm.vcpus == 0 || vm.vcpus > 64 || vm.memory_mib < 512 {
                return Err(ConfigError::Validation(format!(
                    "Android VM {} has invalid vcpus or memory_mib",
                    vm.name
                )));
            }
            let mut selected = HashSet::new();
            for controller in &vm.controllers {
                if !selected.insert(controller) || !self.controllers.contains_key(controller) {
                    return Err(ConfigError::Validation(format!(
                        "Android VM {} controllers must be unique configured controller names",
                        vm.name,
                    )));
                }
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

    pub fn controllers_for_vm(
        &self,
        vm: &AndroidVmConfig,
    ) -> impl Iterator<Item = &ControllerConfig> {
        self.controllers.values().filter(move |controller| {
            controller.active
                && (vm.controllers.is_empty() || vm.controllers.contains(&controller.name))
        })
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
        let controller = ControllerConfig {
            name: "my-iphone".into(),
            sources: vec!["100.64.0.2".parse().unwrap()],
            adb_public_key_file: "/etc/adb/my-iphone.pub".into(),
            active: true,
        };
        let vm = AndroidVmConfig {
            name: "android-game-01".into(),
            labels: vec!["game".into()],
            address: "10.80.0.2".parse().unwrap(),
            base_image: "/var/lib/tailnet-android-vm-manager/images/android-base.qcow2".into(),
            controllers: vec![],
            vcpus: 4,
            memory_mib: 4096,
            autostart: false,
        };
        Config {
            router: RouterConfig {
                hostname: "android-tailnet-router".into(),
                auth_key_file: "/run/secrets/tailscale-authkey".into(),
                tailscale_interface: "tailscale0".into(),
                uplink_interface: "ens3".into(),
                guest_interface: "ens4".into(),
                lan_address: "10.80.0.1".parse().unwrap(),
            },
            controllers: BTreeMap::from([("my-iphone".into(), controller)]),
            vms: BTreeMap::from([("android-game-01".into(), vm)]),
            network: NetworkConfig::default(),
            storage: StorageConfig::default(),
        }
    }
    #[test]
    fn accepts_valid_config() {
        valid().validate().unwrap();
    }
    #[test]
    fn loads_names_from_map_keys() {
        let path = std::env::temp_dir().join(format!("manager-config-{}.toml", std::process::id()));
        fs::write(
            &path,
            r#"
[router]
hostname = "router"
auth_key_file = "/run/auth-key"
uplink_interface = "ens3"
guest_interface = "ens4"
lan_address = "10.80.0.1"

[controllers.phone]
sources = ["100.64.0.2"]
adb_public_key_file = "/etc/adb/phone.pub"

[vms.game-01]
address = "10.80.0.2"
base_image = "/var/lib/images/base.qcow2"
"#,
        )
        .unwrap();
        let config = Config::load(&path).unwrap();
        fs::remove_file(path).unwrap();
        assert_eq!(config.controllers["phone"].name, "phone");
        assert_eq!(config.vms["game-01"].name, "game-01");
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
    fn rejects_empty_controllers() {
        let mut c = valid();
        c.controllers.clear();
        assert!(c.validate().is_err());
    }
    #[test]
    fn rejects_non_tailnet_source() {
        let mut c = valid();
        c.controllers.get_mut("my-iphone").unwrap().sources[0] = "192.0.2.1".parse().unwrap();
        assert!(c.validate().is_err());
    }

    #[test]
    fn rejects_unsafe_vm_names() {
        let mut c = valid();
        let vm = c.vms.remove("android-game-01").unwrap();
        c.vms.insert("../../domain".into(), vm);
        assert!(c.validate().is_err());
    }

    #[test]
    fn rejects_duplicate_or_unsafe_labels() {
        let mut config = valid();
        config.vms.get_mut("android-game-01").unwrap().labels = vec!["game".into(), "game".into()];
        assert!(config.validate().is_err());
        config.vms.get_mut("android-game-01").unwrap().labels = vec!["contains spaces".into()];
        assert!(config.validate().is_err());
    }
}
