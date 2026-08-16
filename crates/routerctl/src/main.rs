use anyhow::Context;
use clap::{Parser, Subcommand};
use manager_core::{
    config::Config,
    firewall,
    tailscale::{self, Enrollment, SystemTailscale},
};
use std::{path::PathBuf, process::ExitCode};

#[derive(Parser)]
#[command(
    version,
    about = "Administration utility for the isolated Tailnet router VM"
)]
struct Cli {
    #[arg(long, default_value = "/etc/tailnet-android-vm-manager/config.toml")]
    config: PathBuf,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Enroll,
    /// Reapply non-secret Tailscale route and safety settings.
    Reconfigure,
    FirewallPrint,
    FirewallApply,
    DnsmasqPrint,
    NetplanPrint,
    Preflight,
}

fn main() -> anyhow::Result<ExitCode> {
    let cli = Cli::parse();
    let config = Config::load(&cli.config)
        .with_context(|| format!("failed to load {}", cli.config.display()))?;
    match cli.command {
        Command::Enroll => match tailscale::enroll(&config, &SystemTailscale)? {
            Enrollment::ConnectedAndSigned => {
                println!("Router is connected and signed by Tailnet Lock.")
            }
            Enrollment::AlreadyConnected | Enrollment::ConnectedAwaitingSignature => {
                println!("Router is connected but requires a Tailnet Lock signature.");
                println!(
                    "Run `tailscale lock status` here and its sign command on a trusted signing node."
                );
            }
        },
        Command::Reconfigure => {
            tailscale::reconfigure(&config, &SystemTailscale)?;
            println!("Reapplied advertised Android /32 routes.");
        }
        Command::FirewallPrint => print!("{}", firewall::render(&config, false)?),
        Command::FirewallApply => {
            firewall::apply(&config)?;
            println!("Installed router forwarding allowlist.");
        }
        Command::DnsmasqPrint => {
            print!("{}", manager_core::guest_bootstrap::dnsmasq_config(&config))
        }
        Command::NetplanPrint => {
            print!("{}", manager_core::guest_bootstrap::router_netplan(&config))
        }
        Command::Preflight => {
            let forwarding = std::fs::read_to_string("/proc/sys/net/ipv4/ip_forward")
                .is_ok_and(|v| v.trim() == "1");
            let uplink = command_succeeds(
                "ip",
                &["link", "show", "dev", &config.router.uplink_interface],
            );
            let guest = std::process::Command::new("ip")
                .args([
                    "-4",
                    "address",
                    "show",
                    "dev",
                    &config.router.guest_interface,
                ])
                .output()
                .is_ok_and(|output| {
                    output.status.success()
                        && String::from_utf8_lossy(&output.stdout).contains(&format!(
                            "{}/{}",
                            config.router.lan_address,
                            config.network.guest_subnet.prefix_len()
                        ))
                });
            let tailnet = command_succeeds(
                "ip",
                &["link", "show", "dev", &config.router.tailscale_interface],
            );
            let firewall =
                command_succeeds("nft", &["list", "table", "inet", "tailnet_android_router"]);
            let dnsmasq = command_succeeds("systemctl", &["is-active", "--quiet", "dnsmasq"]);
            let lock = std::process::Command::new("tailscale")
                .args(["lock", "status"])
                .output()
                .is_ok_and(|o| {
                    o.status.success()
                        && String::from_utf8_lossy(&o.stdout)
                            .contains("This node is accessible under Tailnet Lock.")
                });
            println!(
                "[{}] ipv4-forwarding",
                if forwarding { "PASS" } else { "FAIL" }
            );
            for (name, passed) in [
                ("uplink-interface", uplink),
                ("guest-address", guest),
                ("tailscale-interface", tailnet),
                ("router-firewall", firewall),
                ("guest-dhcp-dns", dnsmasq),
            ] {
                println!("[{}] {name}", if passed { "PASS" } else { "FAIL" });
            }
            println!("[{}] tailnet-lock", if lock { "PASS" } else { "FAIL" });
            if ![forwarding, uplink, guest, tailnet, firewall, dnsmasq, lock]
                .into_iter()
                .all(|passed| passed)
            {
                return Ok(ExitCode::FAILURE);
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn command_succeeds(program: &str, args: &[&str]) -> bool {
    std::process::Command::new(program)
        .args(args)
        .status()
        .is_ok_and(|status| status.success())
}
