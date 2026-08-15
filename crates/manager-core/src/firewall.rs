use std::{
    io::Write,
    process::{Command, Stdio},
};

use thiserror::Error;

use crate::config::Config;

const TABLE_FAMILY: &str = "inet";
const TABLE_NAME: &str = "tailnet_android_vm_manager";

#[derive(Debug, Error)]
pub enum FirewallError {
    #[error("could not execute nft: {0}")]
    Execute(#[from] std::io::Error),
    #[error("nft rejected the generated policy: {0}")]
    Rejected(String),
}

#[must_use]
pub fn render(config: &Config, replace: bool) -> String {
    let sources = config
        .network
        .allowed_tailnet_sources
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    let delete = if replace {
        format!("delete table {TABLE_FAMILY} {TABLE_NAME}\n")
    } else {
        String::new()
    };
    format!(
        "{delete}table {TABLE_FAMILY} {TABLE_NAME} {{\n\
         \tset allowed_tailnet_sources {{\n\
         \t\ttype ipv4_addr\n\
         \t\tflags constant\n\
         \t\telements = {{ {sources} }}\n\
         \t}}\n\
         \tchain endpoint_input {{\n\
         \t\ttype filter hook input priority filter; policy accept;\n\
         \t\tiifname \"{interface}\" ip daddr {destination} tcp dport {start}-{end} ip saddr @allowed_tailnet_sources accept\n\
         \t\tiifname \"{interface}\" ip daddr {destination} tcp dport {start}-{end} drop\n\
         \t}}\n\
         }}\n",
        interface = config.network.tailscale_interface,
        destination = config.host.tailnet_address,
        start = config.network.endpoint_port_start,
        end = config.network.endpoint_port_end,
    )
}

pub fn apply(config: &Config) -> Result<(), FirewallError> {
    let exists = Command::new("nft")
        .args(["list", "table", TABLE_FAMILY, TABLE_NAME])
        .output()?
        .status
        .success();
    let mut child = Command::new("nft")
        .args(["-f", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .expect("nft stdin is piped")
        .write_all(render(config, exists).as_bytes())?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(FirewallError::Rejected(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::config::{HostConfig, NetworkConfig, StorageConfig, TailscaleConfig};

    use super::*;

    #[test]
    fn renders_only_the_configured_interface_destination_ports_and_sources() {
        let config = Config {
            host: HostConfig {
                tailnet_address: "100.64.0.1".parse().unwrap(),
            },
            tailscale: TailscaleConfig {
                hostname: "android-vm-host".into(),
                auth_key_file: "/run/secrets/tailscale-authkey".into(),
                require_tailnet_lock: true,
            },
            network: NetworkConfig {
                allowed_tailnet_sources: vec![
                    "100.64.0.2".parse().unwrap(),
                    "100.64.0.3".parse().unwrap(),
                ],
                ..NetworkConfig::default()
            },
            storage: StorageConfig::default(),
        };
        let rules = render(&config, true);
        assert!(rules.starts_with("delete table inet tailnet_android_vm_manager\n"));
        assert!(rules.contains("elements = { 100.64.0.2, 100.64.0.3 }"));
        assert!(rules.contains("iifname \"tailscale0\" ip daddr 100.64.0.1"));
        assert!(rules.contains("tcp dport 31000-31999"));
    }
}
