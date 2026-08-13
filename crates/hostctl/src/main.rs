use std::{path::PathBuf, process::ExitCode};

use anyhow::Context;
use clap::{Parser, Subcommand};
use manager_core::{
    config::Config,
    preflight::{self, CheckStatus, SystemProbe},
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
}

fn main() -> anyhow::Result<ExitCode> {
    let cli = Cli::parse();
    let config = Config::load(&cli.config)
        .with_context(|| format!("failed to load {}", cli.config.display()))?;

    match cli.command {
        Command::Preflight => {
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
    }
}
