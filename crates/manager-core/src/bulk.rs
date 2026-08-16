use crate::{
    config::{AndroidVmConfig, Config},
    lifecycle::{self, SystemVirsh, VmState},
};
use fs2::FileExt;
use std::{
    fs::{self, File, OpenOptions},
    path::Path,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

#[derive(Debug, Clone, Copy)]
pub enum Operation {
    Status,
    Start {
        wait_ready: Duration,
    },
    Stop {
        timeout: Duration,
        force_after_timeout: bool,
    },
    Hibernate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultRow {
    pub name: String,
    pub result: Result<VmState, String>,
}

pub fn execute(
    config: &Config,
    vms: &[&AndroidVmConfig],
    operation: Operation,
    jobs: usize,
) -> Vec<ResultRow> {
    assert!(jobs > 0, "jobs validated by CLI");
    let next = AtomicUsize::new(0);
    let rows = Mutex::new((0..vms.len()).map(|_| None).collect::<Vec<_>>());
    let worker_count = jobs.min(vms.len());
    std::thread::scope(|scope| {
        for _ in 0..worker_count {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(vm) = vms.get(index) else { break };
                    let result = run_one(&config.storage.state_dir, vm, operation);
                    rows.lock().expect("bulk result mutex")[index] = Some(ResultRow {
                        name: vm.name.clone(),
                        result,
                    });
                }
            });
        }
    });
    rows.into_inner()
        .expect("bulk result mutex")
        .into_iter()
        .map(|row| row.expect("every selected VM processed"))
        .collect()
}

fn run_one(
    state_dir: &Path,
    vm: &AndroidVmConfig,
    operation: Operation,
) -> Result<VmState, String> {
    let _lock = VmLock::acquire(state_dir, &vm.name).map_err(|error| error.to_string())?;
    let virsh = SystemVirsh;
    match operation {
        Operation::Status => lifecycle::state(&virsh, vm).map_err(|error| error.to_string()),
        Operation::Start { wait_ready } => {
            let state = lifecycle::start(&virsh, vm).map_err(|error| error.to_string())?;
            if !wait_ready.is_zero() {
                lifecycle::wait_for_adb(vm, wait_ready).map_err(|error| error.to_string())?;
            }
            Ok(state)
        }
        Operation::Stop {
            timeout,
            force_after_timeout,
        } => lifecycle::stop(&virsh, vm, timeout, force_after_timeout)
            .map_err(|error| error.to_string()),
        Operation::Hibernate => lifecycle::hibernate(&virsh, vm).map_err(|error| error.to_string()),
    }
}

struct VmLock(File);

impl VmLock {
    fn acquire(state_dir: &Path, name: &str) -> std::io::Result<Self> {
        let directory = state_dir.join("locks");
        fs::create_dir_all(&directory)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(directory.join(format!("{name}.lock")))?;
        file.try_lock_exclusive()?;
        Ok(Self(file))
    }
}

impl Drop for VmLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}
