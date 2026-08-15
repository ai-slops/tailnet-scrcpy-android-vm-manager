use crate::config::Config;
use std::{
    io::Write,
    process::{Command, Stdio},
};
use thiserror::Error;

const FAMILY: &str = "inet";
const TABLE: &str = "tailnet_android_router";
#[derive(Debug, Error)]
pub enum FirewallError {
    #[error("could not execute nft: {0}")]
    Execute(#[from] std::io::Error),
    #[error("nft rejected the generated policy: {0}")]
    Rejected(String),
}

#[must_use]
pub fn render(config: &Config, replace: bool) -> String {
    let mut rules = if replace {
        format!("delete table {FAMILY} {TABLE}\n")
    } else {
        String::new()
    };
    rules.push_str(&format!("table {FAMILY} {TABLE} {{\n\tchain forward {{\n\t\ttype filter hook forward priority filter; policy drop;\n"));
    for access in &config.router.access {
        rules.push_str(&format!(
            "\t\tiifname \"{}\" oifname \"{}\" ip saddr {} ip daddr {} tcp dport 5555 accept\n",
            config.router.tailscale_interface,
            config.router.guest_interface,
            access.source,
            access.guest
        ));
    }
    rules.push_str(&format!(
        "\t\tiifname \"{}\" oifname \"{}\" ip daddr {} drop\n\t\tiifname \"{}\" oifname \"{}\" ip saddr {} ct state established,related accept\n\t\tiifname \"{}\" oifname \"{}\" ip saddr {} drop\n\t}}\n}}\n",
        config.router.tailscale_interface,
        config.router.guest_interface,
        config.network.guest_subnet,
        config.router.guest_interface,
        config.router.tailscale_interface,
        config.network.guest_subnet,
        config.router.guest_interface,
        config.router.tailscale_interface,
        config.network.guest_subnet
    ));
    rules
}

pub fn apply(config: &Config) -> Result<(), FirewallError> {
    let exists = Command::new("nft")
        .args(["list", "table", FAMILY, TABLE])
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
        .expect("piped")
        .write_all(render(config, exists).as_bytes())?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(FirewallError::Rejected(
            String::from_utf8_lossy(&output.stderr).trim().into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn renders_controller_to_guest_only() {
        let c = crate::config::tests::valid();
        let r = render(&c, false);
        assert!(r.contains("ip saddr 100.64.0.2 ip daddr 10.80.0.2 tcp dport 5555 accept"));
        assert!(r.contains("type filter hook forward priority filter; policy drop;"));
        assert!(r.contains("ip daddr 10.80.0.0/24 drop"));
        assert!(r.contains("ip saddr 10.80.0.0/24 ct state established,related accept"));
        assert!(r.contains("oifname \"tailscale0\" ip saddr 10.80.0.0/24 drop"));
    }
}
