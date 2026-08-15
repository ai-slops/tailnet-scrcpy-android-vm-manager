use std::{fs, path::PathBuf, process::ExitCode, time::Duration};

use anyhow::Context;
use clap::{Parser, Subcommand};
use manager_core::{
    adb::AdbPublicKey,
    config::Config,
    lifecycle::{self, SystemVirsh},
    preflight::{self, CheckStatus, SystemProbe},
    router_vm, snapshot,
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
    /// Print the dedicated Tailnet router VM libvirt domain XML.
    RouterDomainXml,
    /// Operate a configured persistent Android VM.
    Vm {
        #[command(subcommand)]
        command: VmCommand,
    },
    /// Validate an ADB public key and print its stable fingerprint.
    AdbFingerprint {
        /// File containing one Android ADB public-key line.
        public_key: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum VmCommand {
    Status {
        name: String,
    },
    Start {
        name: String,
        /// Wait for TCP 5555 readiness after starting; zero disables the wait.
        #[arg(long, default_value_t = 0)]
        wait_ready_seconds: u64,
    },
    Stop {
        name: String,
        #[arg(long, default_value_t = 30)]
        timeout_seconds: u64,
        /// Force power-off only if graceful shutdown exceeds the timeout.
        #[arg(long, default_value_t = false)]
        force_after_timeout: bool,
    },
    /// Save RAM state to SSD through libvirt managed save and power off.
    Hibernate {
        name: String,
    },
    Snapshot {
        name: String,
        #[command(subcommand)]
        command: SnapshotCommand,
    },
}

#[derive(Debug, Subcommand)]
enum SnapshotCommand {
    List,
    Create { snapshot: String },
    Revert { snapshot: String },
    Delete { snapshot: String },
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
        Command::RouterDomainXml => {
            let config = Config::load(&cli.config)
                .with_context(|| format!("failed to load {}", cli.config.display()))?;
            print!("{}", router_vm::domain_xml(&config));
            Ok(ExitCode::SUCCESS)
        }
        Command::Vm { command } => {
            let config = Config::load(&cli.config)
                .with_context(|| format!("failed to load {}", cli.config.display()))?;
            match command {
                VmCommand::Status { name } => {
                    let vm = lifecycle::find_vm(&config, &name)?;
                    println!("{:?}", lifecycle::state(&SystemVirsh, vm)?);
                }
                VmCommand::Start {
                    name,
                    wait_ready_seconds,
                } => {
                    let vm = lifecycle::find_vm(&config, &name)?;
                    lifecycle::start(&SystemVirsh, vm)?;
                    if wait_ready_seconds > 0 {
                        lifecycle::wait_for_adb(vm, Duration::from_secs(wait_ready_seconds))?;
                    }
                    println!("Running");
                }
                VmCommand::Stop {
                    name,
                    timeout_seconds,
                    force_after_timeout,
                } => {
                    let vm = lifecycle::find_vm(&config, &name)?;
                    lifecycle::stop(
                        &SystemVirsh,
                        vm,
                        Duration::from_secs(timeout_seconds),
                        force_after_timeout,
                    )?;
                    println!("Stopped");
                }
                VmCommand::Hibernate { name } => {
                    let vm = lifecycle::find_vm(&config, &name)?;
                    lifecycle::hibernate(&SystemVirsh, vm)?;
                    println!("Hibernated");
                }
                VmCommand::Snapshot { name, command } => {
                    let vm = lifecycle::find_vm(&config, &name)?;
                    match command {
                        SnapshotCommand::List => {
                            for snapshot in snapshot::list(&SystemVirsh, vm)? {
                                println!("{snapshot}");
                            }
                        }
                        SnapshotCommand::Create { snapshot: name } => {
                            snapshot::create(&SystemVirsh, vm, &name)?;
                            println!("Created {name}");
                        }
                        SnapshotCommand::Revert { snapshot: name } => {
                            snapshot::revert(&SystemVirsh, vm, &name)?;
                            println!("Reverted to {name}");
                        }
                        SnapshotCommand::Delete { snapshot: name } => {
                            snapshot::delete(&SystemVirsh, vm, &name)?;
                            println!("Deleted {name}");
                        }
                    }
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
