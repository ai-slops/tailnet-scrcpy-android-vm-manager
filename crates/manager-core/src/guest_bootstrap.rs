use crate::{
    adb::AdbPublicKey,
    android_vm,
    config::{AndroidVmConfig, Config},
};
use ipnet::IpNet;
use std::{collections::HashSet, fs};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GuestBootstrapError {
    #[error("could not read ADB public key {path}: {source}")]
    ReadKey {
        path: String,
        source: std::io::Error,
    },
    #[error("invalid ADB public key {path}: {source}")]
    InvalidKey {
        path: String,
        source: crate::adb::AdbKeyError,
    },
}

pub fn adb_authorized_keys(vm: &AndroidVmConfig) -> Result<String, GuestBootstrapError> {
    let mut fingerprints = HashSet::new();
    let mut lines = Vec::new();
    for path in &vm.adb_public_key_files {
        let contents = fs::read_to_string(path).map_err(|source| GuestBootstrapError::ReadKey {
            path: path.display().to_string(),
            source,
        })?;
        let key =
            AdbPublicKey::parse(&contents).map_err(|source| GuestBootstrapError::InvalidKey {
                path: path.display().to_string(),
                source,
            })?;
        if fingerprints.insert(key.fingerprint().to_owned()) {
            lines.push(key.authorized_line());
        }
    }
    if lines.is_empty() {
        Ok(String::new())
    } else {
        Ok(format!("{}\n", lines.join("\n")))
    }
}

#[must_use]
pub fn dnsmasq_config(config: &Config) -> String {
    let netmask = match config.network.guest_subnet {
        IpNet::V4(network) => network.netmask(),
        IpNet::V6(_) => unreachable!("configuration validation requires IPv4"),
    };
    let mut output = format!(
        "interface={}\nbind-dynamic\ndhcp-authoritative\ndhcp-range={},static,{},12h\ndhcp-option=option:router,{}\ndhcp-option=option:dns-server,{}\n",
        config.router.guest_interface,
        config.network.guest_subnet.network(),
        netmask,
        config.router.lan_address,
        config.router.lan_address,
    );
    for vm in &config.android_vms {
        output.push_str(&format!(
            "dhcp-host={},{},{},infinite\n",
            android_vm::mac_address(vm),
            vm.address,
            vm.name,
        ));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_static_guest_leases_via_router() {
        let config = crate::config::tests::valid();
        let output = dnsmasq_config(&config);
        assert!(output.contains("interface=ens4"));
        assert!(output.contains("dhcp-option=option:router,10.80.0.1"));
        assert!(output.contains("52:54:00:50:00:02,10.80.0.2,android-game-01,infinite"));
    }
}
