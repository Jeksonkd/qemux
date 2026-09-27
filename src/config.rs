use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub fn vms_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join("vms")
}

fn settings_path() -> PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("qemux");
    let _ = fs::create_dir_all(&dir);
    dir.join("settings.json")
}

/// Settings used to live under the app's old name; carry them forward once.
fn legacy_settings_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("vm-manager")
        .join("settings.json")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_ram_unit")]
    pub ram_unit: String,
}

fn default_ram_unit() -> String {
    "mb".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            ram_unit: default_ram_unit(),
        }
    }
}

pub fn load_settings() -> Settings {
    if let Some(s) = fs::read_to_string(settings_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
    {
        return s;
    }
    if let Some(s) = fs::read_to_string(legacy_settings_path())
        .ok()
        .and_then(|s| serde_json::from_str::<Settings>(&s).ok())
    {
        save_settings(&s);
        return s;
    }
    Settings::default()
}

pub fn save_settings(settings: &Settings) {
    if let Ok(s) = serde_json::to_string_pretty(settings) {
        let _ = fs::write(settings_path(), s + "\n");
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmConfig {
    pub name: String,
    pub iso: String,
    pub cpus: u32,
    pub ram_mb: u32,
    #[serde(default)]
    pub disk_size_gb: u32,
    #[serde(default = "default_network")]
    pub network: String,
    #[serde(default = "default_os_type")]
    pub os_type: String,
    /// When set, the VM's disk lives at this absolute path instead of
    /// `<vm_dir>/disk.qcow2` (used when attaching a reused/orphaned disk).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk_path: Option<String>,
}

/// Where this VM's disk actually lives on disk, honoring `disk_path` if set.
pub fn resolved_disk_path(vm_dir: &Path, cfg: &VmConfig) -> PathBuf {
    match &cfg.disk_path {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => vm_dir.join("disk.qcow2"),
    }
}

fn default_network() -> String {
    "none".to_string()
}

fn default_os_type() -> String {
    "other".to_string()
}

/// The known OS categories, matching the bundled icon set.
pub const OS_TYPES: &[(&str, &str)] = &[
    ("win64", "Windows (64-bit)"),
    ("win32", "Windows (32-bit)"),
    ("linux", "Linux"),
    ("bsd", "BSD"),
    ("other", "Other"),
];

pub fn os_type_label(os_type: &str) -> &'static str {
    OS_TYPES
        .iter()
        .find(|(key, _)| *key == os_type)
        .map(|(_, label)| *label)
        .unwrap_or("Other")
}

/// Best-effort guess of the OS category from an ISO's file name.
pub fn guess_os_type(iso_path: &str) -> String {
    let name = Path::new(iso_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    let is_bsd = ["bsd", "freebsd", "openbsd", "netbsd", "ghostbsd"]
        .iter()
        .any(|k| name.contains(k));
    if is_bsd {
        return "bsd".to_string();
    }

    let linux_markers = [
        "linux", "ubuntu", "debian", "fedora", "arch", "mint", "manjaro",
        "centos", "opensuse", "suse", "gentoo", "kali", "rhel", "rocky",
        "alma", "pop-os", "popos", "zorin", "elementary", "endeavour", "nixos",
    ];
    if linux_markers.iter().any(|k| name.contains(k)) {
        return "linux".to_string();
    }

    let is_windows = name.contains("win") || name.contains("windows");
    if is_windows {
        let has_64 = name.contains("64") || name.contains("x64");
        let has_32 = name.contains("32") || name.contains("x86");
        if has_32 && !has_64 {
            return "win32".to_string();
        }
        return "win64".to_string();
    }

    "other".to_string()
}

pub fn slugify(name: &str) -> String {
    let mut slug = String::new();
    let mut last_was_sep = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            last_was_sep = false;
        } else if !last_was_sep && !slug.is_empty() {
            slug.push('-');
            last_was_sep = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "vm".to_string()
    } else {
        slug
    }
}

pub fn unique_slug(name: &str) -> String {
    let base = slugify(name);
    let mut slug = base.clone();
    let mut i = 2;
    while vms_dir().join(&slug).exists() {
        slug = format!("{base}-{i}");
        i += 1;
    }
    slug
}

pub fn load_vms() -> Vec<(String, PathBuf, VmConfig)> {
    let mut out = Vec::new();
    let dir = vms_dir();
    let Ok(entries) = fs::read_dir(&dir) else { return out };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let cfg_path = path.join("config.json");
        if let Ok(text) = fs::read_to_string(&cfg_path) {
            if let Ok(cfg) = serde_json::from_str::<VmConfig>(&text) {
                let slug = entry.file_name().to_string_lossy().to_string();
                out.push((slug, path, cfg));
            }
        }
    }
    out
}

pub fn save_config(vm_dir: &Path, cfg: &VmConfig) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(cfg).unwrap_or_default();
    fs::write(vm_dir.join("config.json"), text + "\n")
}

// ---------------------------------------------------------------------
// Formatting / parsing
// ---------------------------------------------------------------------

pub fn total_ram_mb() -> u32 {
    if let Ok(text) = fs::read_to_string("/proc/meminfo") {
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("MemTotal:") {
                if let Some(kb) = rest.split_whitespace().next() {
                    if let Ok(kb) = kb.parse::<u32>() {
                        return kb / 1024;
                    }
                }
            }
        }
    }
    32768
}

pub fn cpu_count() -> u32 {
    std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(4)
}

pub fn parse_number(text: &str) -> Option<f64> {
    let t = text.trim().to_lowercase();
    let t = t
        .trim_end_matches("gb")
        .trim_end_matches("mb")
        .trim_end_matches('g')
        .trim_end_matches('m')
        .trim();
    let t = t.replace(',', ".");
    t.parse::<f64>().ok()
}

pub fn format_ram_mb(mb: f64) -> String {
    format!("{} MB", mb.round() as i64)
}

pub fn format_ram_gb(mb: f64) -> String {
    let gb = mb / 1024.0;
    let mut s = format!("{gb:.1}");
    if s.ends_with(".0") {
        s.truncate(s.len() - 2);
    }
    format!("{} GB", s.replace('.', ","))
}

pub fn format_ram(mb: f64, unit: &str) -> String {
    if unit == "gb" {
        format_ram_gb(mb)
    } else {
        format_ram_mb(mb)
    }
}

pub fn parse_ram(text: &str, unit: &str) -> Option<f64> {
    let val = parse_number(text)?;
    Some(if unit == "gb" { val * 1024.0 } else { val })
}

pub fn format_disk(gb: u32) -> String {
    if gb == 0 {
        "—".to_string()
    } else {
        format!("{gb} GB")
    }
}
