use crate::config::{self, VmConfig};
use crate::qemu;
use std::collections::HashMap;
use std::path::PathBuf;

pub struct DiskInfo {
    pub path: PathBuf,
    pub size_gb: u32,
    pub used_by: Option<String>,
}

/// Every `disk.qcow2` found under any VM folder, whether or not that folder
/// still has a `config.json` (a VM deleted with "keep disk" leaves one behind).
pub fn scan_disks() -> Vec<DiskInfo> {
    let vms = config::load_vms();
    let mut used: HashMap<PathBuf, String> = HashMap::new();
    for (_, vm_dir, cfg) in &vms {
        if cfg.disk_size_gb > 0 {
            used.insert(config::resolved_disk_path(vm_dir, cfg), cfg.name.clone());
        }
    }

    let mut disks = Vec::new();
    if let Ok(entries) = std::fs::read_dir(config::vms_dir()) {
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let candidate = dir.join("disk.qcow2");
            if candidate.is_file() {
                let size_gb = qemu::disk_size_gb(&candidate).unwrap_or(0);
                let used_by = used.get(&candidate).cloned();
                disks.push(DiskInfo { path: candidate, size_gb, used_by });
            }
        }
    }
    disks.sort_by(|a, b| a.path.cmp(&b.path));
    disks
}

/// Disks not currently attached to any VM, available to reuse.
pub fn unused_disks() -> Vec<DiskInfo> {
    scan_disks().into_iter().filter(|d| d.used_by.is_none()).collect()
}

/// A disk currently attached to `cfg` also counts as "available" for that
/// same VM's own edit dialog (so it can keep showing as selected).
pub fn unused_disks_excluding(vm_dir: &std::path::Path, cfg: &VmConfig) -> Vec<DiskInfo> {
    let own_path = config::resolved_disk_path(vm_dir, cfg);
    scan_disks()
        .into_iter()
        .filter(|d| d.used_by.is_none() || d.path == own_path)
        .collect()
}

pub fn folder_label(path: &std::path::Path) -> String {
    path.parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}
