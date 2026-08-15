use std::{
    collections::HashSet,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    process::ExitCode,
    time::Duration,
};

use anyhow::{Context, bail};
use clap::Parser;
use endpoint_gateway::serve;
use manager_core::{config::Config, endpoint::PortRange};
use tokio::net::TcpListener;

#[derive(Debug, Parser)]
#[command(version, about = "Lease-bounded ADB TCP endpoint")]
struct Cli {
    #[arg(long, default_value = "/etc/tailnet-android-vm-manager/config.toml")]
    config: PathBuf,
    /// Explicit fallback listen address; the KVM host is not a Tailscale node.
    #[arg(long)]
    listen_address: IpAddr,
    /// Port for the optional host-side compatibility fallback.
    #[arg(long)]
    listen_port: u16,
    /// Private Android guest ADB endpoint. Port 5555 is required.
    #[arg(long)]
    guest: SocketAddr,
    /// Lease lifetime in seconds (1 through 86400).
    #[arg(long)]
    lease_seconds: u64,
    /// Maximum simultaneous TCP connections for this lease.
    #[arg(long, default_value_t = 3)]
    max_connections: usize,
}

#[tokio::main]
async fn main() -> anyhow::Result<ExitCode> {
    let cli = Cli::parse();
    let config = Config::load(&cli.config)
        .with_context(|| format!("failed to load {}", cli.config.display()))?;
    validate(&config, &cli)?;

    let listen = SocketAddr::new(cli.listen_address, cli.listen_port);
    let listener = TcpListener::bind(listen)
        .await
        .with_context(|| format!("failed to bind endpoint {listen}"))?;
    let shutdown = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            eprintln!("shutdown signal listener failed: {error}");
        }
    };
    let reason = serve(
        listener,
        cli.guest,
        config
            .router
            .access
            .iter()
            .map(|access| IpAddr::V4(access.source))
            .collect::<HashSet<_>>(),
        Duration::from_secs(cli.lease_seconds),
        cli.max_connections,
        shutdown,
    )
    .await?;
    eprintln!("ADB endpoint closed: {reason:?}");
    Ok(ExitCode::SUCCESS)
}

fn validate(config: &Config, cli: &Cli) -> anyhow::Result<()> {
    let range = PortRange::new(
        config.network.endpoint_port_start,
        config.network.endpoint_port_end,
    )?;
    if !range.contains(cli.listen_port) {
        bail!("listen port is outside the configured endpoint range");
    }
    if cli.guest.port() != 5555 || !config.network.guest_subnet.contains(&cli.guest.ip()) {
        bail!("guest must be inside the configured guest subnet on ADB port 5555");
    }
    if !(1..=86_400).contains(&cli.lease_seconds) {
        bail!("lease-seconds must be between 1 and 86400");
    }
    if !(1..=16).contains(&cli.max_connections) {
        bail!("max-connections must be between 1 and 16");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use manager_core::config::{NetworkConfig, RouterAccess, RouterConfig, StorageConfig};

    use super::*;

    fn config() -> Config {
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

    fn args() -> Cli {
        Cli {
            config: "unused".into(),
            listen_address: "127.0.0.1".parse().unwrap(),
            listen_port: 31_000,
            guest: "10.80.0.2:5555".parse().unwrap(),
            lease_seconds: 60,
            max_connections: 3,
        }
    }

    #[test]
    fn accepts_bounded_private_adb_endpoint() {
        validate(&config(), &args()).unwrap();
    }

    #[test]
    fn rejects_public_or_non_adb_destination() {
        let mut cli = args();
        cli.guest = "192.0.2.1:5555".parse().unwrap();
        assert!(validate(&config(), &cli).is_err());
        cli.guest = "10.80.0.2:22".parse().unwrap();
        assert!(validate(&config(), &cli).is_err());
    }

    #[test]
    fn rejects_port_outside_configured_range() {
        let mut cli = args();
        cli.listen_port = 30_999;
        assert!(validate(&config(), &cli).is_err());
    }
}
