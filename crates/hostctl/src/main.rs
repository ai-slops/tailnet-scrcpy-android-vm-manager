use std::{fs, path::PathBuf, process::ExitCode, time::Duration};

use anyhow::Context;
use clap::{Args, Parser, Subcommand};
use manager_core::{
    adb::AdbPublicKey,
    bulk::{self, Operation},
    config::Config,
    guest_bootstrap,
    inventory::{self, Selector},
    libvirt_network,
    lifecycle::{self, SystemVirsh},
    preflight::{self, CheckStatus, SystemProbe},
    provision, reconcile, router_vm, snapshot,
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
    /// Print host paths used by the router provisioner.
    RouterArtifactPaths,
    /// Print the host-address-free isolated Android libvirt network XML.
    GuestNetworkXml,
    /// Reconcile the network, router, and every configured Android domain.
    Reconcile,
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
    /// List configured VMs and their current states.
    List {
        /// Require every supplied label; omit to list the full inventory.
        #[arg(long = "label")]
        labels: Vec<String>,
        #[arg(long)]
        json: bool,
        #[arg(long, default_value_t = 2)]
        jobs: usize,
    },
    /// Create a persistent qcow2 overlay and define the libvirt domain.
    Create { name: String },
    /// Print this persistent Android VM's libvirt domain XML.
    DomainXml { name: String },
    /// Validate and print this VM's Android ADB authorized_keys content.
    AdbAuthorizedKeys { name: String },
    Status {
        #[command(flatten)]
        selection: SelectionArgs,
    },
    Start {
        #[command(flatten)]
        selection: SelectionArgs,
        /// Wait for TCP 5555 readiness after starting; zero disables the wait.
        #[arg(long, default_value_t = 0)]
        wait_ready_seconds: u64,
    },
    Stop {
        #[command(flatten)]
        selection: SelectionArgs,
        #[arg(long, default_value_t = 30)]
        timeout_seconds: u64,
        /// Force power-off only if graceful shutdown exceeds the timeout.
        #[arg(long, default_value_t = false)]
        force_after_timeout: bool,
    },
    /// Save RAM state to SSD through libvirt managed save and power off.
    Hibernate {
        #[command(flatten)]
        selection: SelectionArgs,
    },
    Snapshot {
        name: String,
        #[command(subcommand)]
        command: SnapshotCommand,
    },
}

#[derive(Debug, Clone, Args)]
struct SelectionArgs {
    /// Select one VM by name.
    name: Option<String>,
    /// Select the entire inventory.
    #[arg(long)]
    all: bool,
    /// Select VMs containing every supplied label.
    #[arg(long = "label")]
    labels: Vec<String>,
    /// Maximum concurrent VM operations.
    #[arg(long, default_value_t = 2)]
    jobs: usize,
}

impl SelectionArgs {
    fn selector(&self) -> Selector {
        Selector {
            name: self.name.clone(),
            all: self.all,
            labels: self.labels.clone(),
        }
    }
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
            Config::load(&cli.config)
                .with_context(|| format!("failed to load {}", cli.config.display()))?;
            let results = preflight::run(&SystemProbe);
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
        Command::RouterArtifactPaths => {
            let config = Config::load(&cli.config)
                .with_context(|| format!("failed to load {}", cli.config.display()))?;
            println!("disk={}", router_vm::disk_path(&config).display());
            println!("seed={}", router_vm::seed_path(&config).display());
            println!(
                "base={}",
                config
                    .storage
                    .image_dir
                    .join("ubuntu-24.04-server-cloudimg-amd64.img")
                    .display()
            );
            println!("guest_network={}", config.network.guest_network);
            println!("hostname={}", config.router.hostname);
            Ok(ExitCode::SUCCESS)
        }
        Command::GuestNetworkXml => {
            let config = Config::load(&cli.config)
                .with_context(|| format!("failed to load {}", cli.config.display()))?;
            print!("{}", libvirt_network::guest_network_xml(&config));
            Ok(ExitCode::SUCCESS)
        }
        Command::Reconcile => {
            let config = Config::load(&cli.config)
                .with_context(|| format!("failed to load {}", cli.config.display()))?;
            let report = reconcile::run(&config)?;
            for line in report.lines {
                println!("{line}");
            }
            Ok(if report.failed {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            })
        }
        Command::Vm { command } => {
            let config = Config::load(&cli.config)
                .with_context(|| format!("failed to load {}", cli.config.display()))?;
            match command {
                VmCommand::List { labels, json, jobs } => {
                    if !(1..=32).contains(&jobs) {
                        anyhow::bail!("--jobs must be between 1 and 32");
                    }
                    let selector = Selector {
                        all: labels.is_empty(),
                        labels,
                        name: None,
                    };
                    let selected = inventory::select(&config, &selector)?;
                    let rows = bulk::execute(&config, &selected, Operation::Status, jobs);
                    if json {
                        let output = rows
                            .iter()
                            .zip(selected.iter())
                            .map(|(row, vm)| {
                                serde_json::json!({
                                    "name": row.name,
                                    "address": vm.address,
                                    "labels": vm.labels,
                                    "autostart": vm.autostart,
                                    "state": row.result.as_ref().map(|state| format!("{state:?}")).ok(),
                                    "error": row.result.as_ref().err(),
                                })
                            })
                            .collect::<Vec<_>>();
                        println!("{}", serde_json::to_string_pretty(&output)?);
                    } else {
                        for (row, vm) in rows.iter().zip(selected.iter()) {
                            let state = match &row.result {
                                Ok(state) => format!("{state:?}"),
                                Err(error) => format!("ERROR: {error}"),
                            };
                            println!(
                                "{}\t{}\t{}\t{}",
                                row.name,
                                vm.address,
                                vm.labels.join(","),
                                state
                            );
                        }
                    }
                    Ok(exit_for_rows(&rows))
                }
                VmCommand::Create { name } => {
                    let vm = lifecycle::find_vm(&config, &name)?;
                    provision::create(&config, vm)?;
                    println!("Created");
                    Ok(ExitCode::SUCCESS)
                }
                VmCommand::DomainXml { name } => {
                    let vm = lifecycle::find_vm(&config, &name)?;
                    print!("{}", manager_core::android_vm::domain_xml(&config, vm));
                    Ok(ExitCode::SUCCESS)
                }
                VmCommand::AdbAuthorizedKeys { name } => {
                    let vm = lifecycle::find_vm(&config, &name)?;
                    print!("{}", guest_bootstrap::adb_authorized_keys(&config, vm)?);
                    Ok(ExitCode::SUCCESS)
                }
                VmCommand::Status { selection } => run_bulk(&config, &selection, Operation::Status),
                VmCommand::Start {
                    selection,
                    wait_ready_seconds,
                } => run_bulk(
                    &config,
                    &selection,
                    Operation::Start {
                        wait_ready: Duration::from_secs(wait_ready_seconds),
                    },
                ),
                VmCommand::Stop {
                    selection,
                    timeout_seconds,
                    force_after_timeout,
                } => run_bulk(
                    &config,
                    &selection,
                    Operation::Stop {
                        timeout: Duration::from_secs(timeout_seconds),
                        force_after_timeout,
                    },
                ),
                VmCommand::Hibernate { selection } => {
                    run_bulk(&config, &selection, Operation::Hibernate)
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
                    Ok(ExitCode::SUCCESS)
                }
            }
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

fn run_bulk(
    config: &Config,
    selection: &SelectionArgs,
    operation: Operation,
) -> anyhow::Result<ExitCode> {
    if !(1..=32).contains(&selection.jobs) {
        anyhow::bail!("--jobs must be between 1 and 32");
    }
    let selected = inventory::select(config, &selection.selector())?;
    let rows = bulk::execute(config, &selected, operation, selection.jobs);
    for row in &rows {
        match &row.result {
            Ok(state) => println!("{}\t{state:?}", row.name),
            Err(error) => println!("{}\tERROR\t{error}", row.name),
        }
    }
    Ok(exit_for_rows(&rows))
}

fn exit_for_rows(rows: &[bulk::ResultRow]) -> ExitCode {
    if rows.iter().all(|row| row.result.is_ok()) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
