use crate::{
    android_vm,
    config::{AndroidVmConfig, Config},
};
use serde_json::Value;
use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProvisionError {
    #[error("Android base image is not a regular file: {0}")]
    InvalidBase(String),
    #[error("persistent VM disk already exists: {0}")]
    AlreadyExists(String),
    #[error("could not prepare VM storage: {0}")]
    Io(#[from] std::io::Error),
    #[error("qemu-img info failed: {0}")]
    ImageInfo(String),
    #[error("Android base image must be qcow2, found {0}")]
    BaseFormat(String),
    #[error("qemu-img failed to create the persistent overlay: {0}")]
    Overlay(String),
    #[error("virsh failed to define the Android domain: {0}")]
    Define(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnsureResult {
    Created,
    Defined,
}

pub fn ensure(config: &Config, vm: &AndroidVmConfig) -> Result<EnsureResult, ProvisionError> {
    if android_vm::disk_path(config, vm).exists() {
        define(config, vm)?;
        Ok(EnsureResult::Defined)
    } else {
        create(config, vm)?;
        Ok(EnsureResult::Created)
    }
}

pub fn create(config: &Config, vm: &AndroidVmConfig) -> Result<(), ProvisionError> {
    let base_metadata = fs::metadata(&vm.base_image)
        .map_err(|_| ProvisionError::InvalidBase(vm.base_image.display().to_string()))?;
    if !base_metadata.is_file() {
        return Err(ProvisionError::InvalidBase(
            vm.base_image.display().to_string(),
        ));
    }
    let disk = android_vm::disk_path(config, vm);
    if disk.exists() {
        return Err(ProvisionError::AlreadyExists(disk.display().to_string()));
    }
    fs::create_dir_all(&config.storage.vm_dir)?;

    let info = Command::new("qemu-img")
        .args(["info", "--output=json"])
        .arg(&vm.base_image)
        .output()?;
    if !info.status.success() {
        return Err(ProvisionError::ImageInfo(output_text(&info)));
    }
    let parsed: Value = serde_json::from_slice(&info.stdout)
        .map_err(|error| ProvisionError::ImageInfo(error.to_string()))?;
    let format = parsed["format"].as_str().unwrap_or("unknown");
    if format != "qcow2" {
        return Err(ProvisionError::BaseFormat(format.to_owned()));
    }

    let temporary =
        config
            .storage
            .vm_dir
            .join(format!(".{}.qcow2.tmp-{}", vm.name, std::process::id()));
    if temporary.exists() {
        return Err(ProvisionError::AlreadyExists(
            temporary.display().to_string(),
        ));
    }
    let create = Command::new("qemu-img")
        .args(["create", "-f", "qcow2", "-F", "qcow2", "-b"])
        .arg(&vm.base_image)
        .arg(&temporary)
        .output()?;
    if !create.status.success() {
        return Err(ProvisionError::Overlay(output_text(&create)));
    }
    if let Err(error) = fs::hard_link(&temporary, &disk) {
        let _ = fs::remove_file(&temporary);
        return if error.kind() == std::io::ErrorKind::AlreadyExists {
            Err(ProvisionError::AlreadyExists(disk.display().to_string()))
        } else {
            Err(ProvisionError::Io(error))
        };
    }
    fs::remove_file(&temporary)?;

    if let Err(error) = define(config, vm) {
        fs::remove_file(&disk)?;
        return Err(error);
    }
    Ok(())
}

pub fn define(config: &Config, vm: &AndroidVmConfig) -> Result<(), ProvisionError> {
    let mut child = Command::new("virsh")
        .args(["define", "/dev/stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(android_vm::domain_xml(config, vm).as_bytes())?;
    let defined = child.wait_with_output()?;
    if !defined.status.success() {
        return Err(ProvisionError::Define(output_text(&defined)));
    }
    Ok(())
}

fn output_text(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let text = if stderr.trim().is_empty() {
        stdout
    } else {
        stderr
    };
    text.trim().to_owned()
}
