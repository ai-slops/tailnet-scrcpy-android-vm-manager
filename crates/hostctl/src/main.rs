use std::{fs, path::PathBuf, process::ExitCode};

use anyhow::Context;
use clap::{Parser, Subcommand};
use manager_core::{
    adb::AdbPublicKey,
    config::Config,
    preflight::{self, CheckStatus, SystemProbe},
    tailscale::{self, Enrollment, SystemTailscale},
};

#[derive(Debug, Parser)]
#[command(
    version,
    about = "Host administration utility for the Android VM manager"
)]
struct Cli {
    #[arg(long, default_value = "/etc/tailnet-android-vm-manager/config.toml")]
    config: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Validate configuration and required host capabilities.
    Preflight,
    /// Join Tailscale with the configured auth-key file and report signing state.
    TailscaleEnroll,
    /// Validate an ADB public key and print its stable fingerprint.
    AdbFingerprint {
        /// File containing one Android ADB public-key line.
        public_key: PathBuf,
    },
}

fn main() -> anyhow::Result<ExitCode> {
    let cli = Cli::parse();

    match cli.command {
        Command::Preflight => {
            let config = Config::load(&cli.config)
                .with_context(|| format!("failed to load {}", cli.config.display()))?;
            let results = preflight::run(&config, &SystemProbe);
            for result in &results {
                let label = match result.status {
                    CheckStatus::Pass => "PASS",
                    CheckStatus::Fail => "FAIL",
                };
                println!("[{label}] {}: {}", result.name, result.detail);
            }
            Ok(if results.iter().all(|result| result.passed()) {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            })
        }
        Command::TailscaleEnroll => {
            let config = Config::load(&cli.config)
                .with_context(|| format!("failed to load {}", cli.config.display()))?;
            match tailscale::enroll(&config, &SystemTailscale)? {
                Enrollment::ConnectedAndSigned => {
                    println!("Tailscale is connected and this host is signed by Tailnet Lock.");
                }
                Enrollment::AlreadyConnected | Enrollment::ConnectedAwaitingSignature => {
                    println!(
                        "Tailscale is connected, but this host still requires a Tailnet Lock signature."
                    );
                    println!(
                        "Run `tailscale lock status`, then execute its sign command on a trusted signing node."
                    );
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::AdbFingerprint { public_key } => {
            let contents = fs::read_to_string(&public_key)
                .with_context(|| format!("failed to read {}", public_key.display()))?;
            let key = AdbPublicKey::parse(&contents)
                .with_context(|| format!("invalid ADB public key in {}", public_key.display()))?;
            println!("{}", key.fingerprint());
            Ok(ExitCode::SUCCESS)
        }
    }
}
