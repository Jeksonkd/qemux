use crate::config::{self, VmConfig};
use std::fs::File;
use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// True when running inside a Flatpak sandbox.
fn in_flatpak() -> bool {
    Path::new("/.flatpak-info").exists()
}

/// A `Command` for a host tool. Inside a Flatpak sandbox, `qemu-system-x86_64`
/// and `qemu-img` aren't part of the sandboxed runtime, so this routes through
/// `flatpak-spawn --host` to run the host's own install of them instead.
fn host_command(program: &str) -> Command {
    if in_flatpak() {
        let mut cmd = Command::new("flatpak-spawn");
        cmd.arg("--host").arg(program);
        cmd
    } else {
        Command::new(program)
    }
}

pub fn build_qemu_args(vm_dir: &Path, cfg: &VmConfig) -> Vec<String> {
    let mut args = vec![
        "qemu-system-x86_64".to_string(),
        "-name".to_string(),
        cfg.name.clone(),
        "-machine".to_string(),
        "q35,accel=kvm".to_string(),
        "-cpu".to_string(),
        "host".to_string(),
        "-smp".to_string(),
        cfg.cpus.to_string(),
        "-m".to_string(),
        cfg.ram_mb.to_string(),
        "-display".to_string(),
        "gtk".to_string(),
        "-vga".to_string(),
        "std".to_string(),
        "-usb".to_string(),
        "-device".to_string(),
        "usb-tablet".to_string(),
        "-no-reboot".to_string(),
    ];

    if cfg.disk_size_gb > 0 {
        let disk_path = config::resolved_disk_path(vm_dir, cfg);
        args.push("-drive".to_string());
        args.push(format!(
            "file={},if=none,id=hd0,format=qcow2",
            disk_path.display()
        ));
        args.push("-device".to_string());
        args.push("ahci,id=ahci0".to_string());
        args.push("-device".to_string());
        args.push("ide-hd,drive=hd0,bus=ahci0.0".to_string());
    }

    if !cfg.iso.is_empty() {
        args.push("-cdrom".to_string());
        args.push(cfg.iso.clone());
    }

    if cfg.disk_size_gb > 0 {
        args.push("-boot".to_string());
        args.push("menu=on".to_string());
    } else {
        args.push("-boot".to_string());
        args.push("d".to_string());
    }

    if cfg.network == "nat" {
        args.push("-nic".to_string());
        args.push("user,model=e1000".to_string());
    } else {
        args.push("-nic".to_string());
        args.push("none".to_string());
    }

    args
}

pub fn write_run_sh(vm_dir: &Path, cfg: &VmConfig) -> std::io::Result<PathBuf> {
    let args = build_qemu_args(vm_dir, cfg);
    let quoted: Vec<String> = args
        .iter()
        .map(|a| {
            if a.contains(' ') {
                format!("\"{a}\"")
            } else {
                a.clone()
            }
        })
        .collect();
    let script = format!("#!/bin/bash\nexec {}\n", quoted.join(" \\\n  "));
    let run_sh = vm_dir.join("run.sh");
    let mut f = File::create(&run_sh)?;
    f.write_all(script.as_bytes())?;
    drop(f);
    let mut perms = std::fs::metadata(&run_sh)?.permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    std::fs::set_permissions(&run_sh, perms)?;
    Ok(run_sh)
}

pub fn launch(vm_dir: &Path, cfg: &VmConfig) -> std::io::Result<()> {
    let run_sh = write_run_sh(vm_dir, cfg)?;
    let log_path = vm_dir.join("qemu.log");
    let log_out = File::create(&log_path)?;
    let log_err = log_out.try_clone()?;
    host_command("bash")
        .arg(run_sh)
        .current_dir(vm_dir)
        .stdout(Stdio::from(log_out))
        .stderr(Stdio::from(log_err))
        .process_group(0)
        .spawn()?;
    Ok(())
}

pub fn create_disk(disk_path: &Path, size_gb: u32) -> Result<(), String> {
    run_qemu_img(&[
        "create",
        "-f",
        "qcow2",
        &disk_path.to_string_lossy(),
        &format!("{size_gb}G"),
    ])
}

pub fn resize_disk(disk_path: &Path, size_gb: u32, shrink: bool) -> Result<(), String> {
    let mut args = vec!["resize"];
    if shrink {
        args.push("--shrink");
    }
    let disk_str = disk_path.to_string_lossy().to_string();
    let size_str = format!("{size_gb}G");
    args.push(&disk_str);
    args.push(&size_str);
    run_qemu_img(&args)
}

/// The disk's virtual size in GB, via `qemu-img info`.
pub fn disk_size_gb(disk_path: &Path) -> Option<u32> {
    let output = host_command("qemu-img")
        .args(["info", "--output=json", &disk_path.to_string_lossy()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let bytes = json.get("virtual-size")?.as_u64()?;
    Some((bytes as f64 / 1_000_000_000.0).round() as u32)
}

fn run_qemu_img(args: &[&str]) -> Result<(), String> {
    let output = host_command("qemu-img")
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}
