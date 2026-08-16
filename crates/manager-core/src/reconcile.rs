use crate::{config::Config, libvirt_network, provision, router_vm};
use fs2::FileExt;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    process::{Command, Stdio},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReconcileError {
    #[error("could not acquire reconcile lock: {0}")]
    Lock(String),
    #[error("reconciliation I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("router artifact is missing: {0}; run router-provision first")]
    MissingRouterArtifact(String),
    #[error("virsh {operation} failed: {detail}")]
    Virsh {
        operation: &'static str,
        detail: String,
    },
    #[error("could not reconcile Android VM {name}: {detail}")]
    Android { name: String, detail: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub lines: Vec<String>,
    pub failed: bool,
}

pub fn run(config: &Config) -> Result<Report, ReconcileError> {
    fs::create_dir_all(&config.storage.state_dir)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(config.storage.state_dir.join("reconcile.lock"))?;
    lock.try_lock_exclusive()
        .map_err(|error| ReconcileError::Lock(error.to_string()))?;

    let mut lines = Vec::new();
    let mut failed = false;
    define_xml("net-define", &libvirt_network::guest_network_xml(config))?;
    command(
        "net-autostart",
        &["net-autostart", &config.network.guest_network],
    )?;
    let network_info = command("net-info", &["net-info", &config.network.guest_network])?;
    if !field_is_yes(&network_info, "Active") {
        command("net-start", &["net-start", &config.network.guest_network])?;
    }
    lines.push(format!("network\t{}\tready", config.network.guest_network));

    require_file(&router_vm::disk_path(config))?;
    require_file(&router_vm::seed_path(config))?;
    define_xml("define-router", &router_vm::domain_xml(config))?;
    command("autostart-router", &["autostart", "tailnet-android-router"])?;
    let router_state = command("router-state", &["domstate", "tailnet-android-router"])?;
    if router_state.trim() != "running" {
        command("start-router", &["start", "tailnet-android-router"])?;
    }
    lines.push("router\ttailnet-android-router\trunning".into());

    for vm in &config.android_vms {
        match reconcile_vm(config, vm) {
            Ok(detail) => lines.push(format!("vm\t{}\t{detail}", vm.name)),
            Err(error) => {
                failed = true;
                lines.push(format!("vm\t{}\tERROR\t{error}", vm.name));
            }
        }
    }
    let _ = lock.unlock();
    Ok(Report { lines, failed })
}

fn reconcile_vm(
    config: &Config,
    vm: &crate::config::AndroidVmConfig,
) -> Result<String, ReconcileError> {
    let ensured = provision::ensure(config, vm).map_err(|error| ReconcileError::Android {
        name: vm.name.clone(),
        detail: error.to_string(),
    })?;
    if vm.autostart {
        command("autostart-vm", &["autostart", &vm.name])?;
        let state = command("vm-state", &["domstate", &vm.name])?;
        if state.trim() != "running" {
            command("start-vm", &["start", &vm.name])?;
        }
    } else {
        command(
            "disable-autostart-vm",
            &["autostart", &vm.name, "--disable"],
        )?;
    }
    Ok(format!(
        "{}\tautostart={}",
        match ensured {
            provision::EnsureResult::Created => "created",
            provision::EnsureResult::Defined => "defined",
        },
        vm.autostart
    ))
}

fn require_file(path: &Path) -> Result<(), ReconcileError> {
    if path.is_file() {
        Ok(())
    } else {
        Err(ReconcileError::MissingRouterArtifact(
            path.display().to_string(),
        ))
    }
}

fn field_is_yes(output: &str, field: &str) -> bool {
    output.lines().any(|line| {
        line.split_once(':')
            .is_some_and(|(key, value)| key.trim() == field && value.trim() == "yes")
    })
}

fn define_xml(operation: &'static str, xml: &str) -> Result<(), ReconcileError> {
    let mut child = Command::new("virsh")
        .args([operation_name(operation), "/dev/stdin"])
        .env("LC_ALL", "C")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(xml.as_bytes())?;
    let output = child.wait_with_output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(ReconcileError::Virsh {
            operation,
            detail: output_text(&output),
        })
    }
}

fn operation_name(operation: &str) -> &str {
    if operation == "net-define" {
        "net-define"
    } else {
        "define"
    }
}

fn command(operation: &'static str, args: &[&str]) -> Result<String, ReconcileError> {
    let output = Command::new("virsh")
        .env("LC_ALL", "C")
        .args(args)
        .output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(ReconcileError::Virsh {
            operation,
            detail: output_text(&output),
        })
    }
}

fn output_text(output: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout).trim(),
        String::from_utf8_lossy(&output.stderr).trim()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_libvirt_yes_fields() {
        assert!(field_is_yes("Name: guest\nActive: yes\n", "Active"));
        assert!(!field_is_yes("Active: no\n", "Active"));
    }
}
