use crate::config::{self, Settings, VmConfig};
use crate::disks;
use crate::hwslider::HwSlider;
use crate::qemu;
use gtk::prelude::*;
use gtk::{gio, glib, Align, Orientation};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

fn alert(parent: &gtk::Window, message: &str, detail: &str) {
    let dialog = gtk::AlertDialog::builder()
        .modal(true)
        .message(message)
        .detail(detail)
        .buttons(["OK"])
        .build();
    dialog.show(Some(parent));
}

fn confirm(parent: &gtk::Window, message: &str, detail: &str, confirm_label: &str, on_confirm: impl Fn() + 'static) {
    let dialog = gtk::AlertDialog::builder()
        .modal(true)
        .message(message)
        .detail(detail)
        .buttons(["Cancel", confirm_label])
        .cancel_button(0)
        .default_button(0)
        .build();
    dialog.choose(
        Some(parent),
        gio::Cancellable::NONE,
        move |result| {
            if let Ok(idx) = result {
                if idx == 1 {
                    on_confirm();
                }
            }
        },
    );
}

fn labeled_row(label_text: &str) -> (gtk::Box, gtk::Entry) {
    let row = gtk::Box::new(Orientation::Horizontal, 10);
    let label = gtk::Label::new(Some(label_text));
    label.set_width_chars(14);
    label.set_halign(Align::Start);
    let entry = gtk::Entry::new();
    entry.set_hexpand(true);
    row.append(&label);
    row.append(&entry);
    (row, entry)
}

pub fn show_vm_dialog(
    parent: &gtk::ApplicationWindow,
    settings: Rc<RefCell<Settings>>,
    existing: Option<(PathBuf, VmConfig)>,
    on_saved: Rc<dyn Fn()>,
) {
    let is_edit = existing.is_some();
    let (vm_dir, cfg0) = existing.clone().unzip();

    let name0 = cfg0.as_ref().map(|c| c.name.clone()).unwrap_or_default();
    let iso0 = cfg0.as_ref().map(|c| c.iso.clone()).unwrap_or_default();
    let cpu0 = cfg0.as_ref().map(|c| c.cpus as f64).unwrap_or(2.0);
    let ram0 = cfg0.as_ref().map(|c| c.ram_mb as f64).unwrap_or(4096.0);
    let disk0 = cfg0.as_ref().map(|c| c.disk_size_gb as f64).unwrap_or(40.0);
    let net0 = cfg0.as_ref().map(|c| c.network.clone()).unwrap_or_else(|| "nat".to_string());
    let os0 = cfg0.as_ref().map(|c| c.os_type.clone()).unwrap_or_else(|| "other".to_string());

    let window = gtk::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title(if is_edit { "Edit VM" } else { "New VM" })
        .default_width(460)
        .resizable(false)
        .build();

    let root = gtk::Box::new(Orientation::Vertical, 14);
    root.set_margin_top(16);
    root.set_margin_bottom(16);
    root.set_margin_start(16);
    root.set_margin_end(16);
    window.set_child(Some(&root));

    // -- Identity --------------------------------------------------------
    let identity_box = gtk::Box::new(Orientation::Vertical, 10);
    identity_box.set_margin_top(10);
    identity_box.set_margin_bottom(10);
    identity_box.set_margin_start(10);
    identity_box.set_margin_end(10);

    let (name_row, name_entry) = labeled_row("Display name");
    name_entry.set_text(&name0);
    identity_box.append(&name_row);

    let (iso_row, iso_entry) = labeled_row("ISO file");
    iso_entry.set_text(&iso0);
    let browse_btn = gtk::Button::with_label("Browse…");
    iso_row.append(&browse_btn);
    identity_box.append(&iso_row);

    let os_row = gtk::Box::new(Orientation::Horizontal, 10);
    let os_label = gtk::Label::new(Some("Operating system"));
    os_label.set_width_chars(14);
    os_label.set_halign(Align::Start);
    let os_labels: Vec<&str> = config::OS_TYPES.iter().map(|(_, l)| *l).collect();
    let os_dropdown = gtk::DropDown::from_strings(&os_labels);
    os_dropdown.set_hexpand(true);
    let os_initial = config::OS_TYPES
        .iter()
        .position(|(k, _)| *k == os0)
        .unwrap_or(config::OS_TYPES.len() - 1) as u32;
    os_dropdown.set_selected(os_initial);
    os_row.append(&os_label);
    os_row.append(&os_dropdown);
    identity_box.append(&os_row);

    let identity_frame = gtk::Frame::new(Some("Identity"));
    identity_frame.set_child(Some(&identity_box));
    root.append(&identity_frame);

    {
        let window = window.clone();
        let iso_entry = iso_entry.clone();
        let os_dropdown = os_dropdown.clone();
        browse_btn.connect_clicked(move |_| {
            let filter = gtk::FileFilter::new();
            filter.add_suffix("iso");
            filter.set_name(Some("ISO images"));
            let filters = gio::ListStore::new::<gtk::FileFilter>();
            filters.append(&filter);

            let file_dialog = gtk::FileDialog::builder()
                .title("Choose ISO")
                .filters(&filters)
                .build();

            let iso_entry = iso_entry.clone();
            let os_dropdown = os_dropdown.clone();
            file_dialog.open(Some(&window), gio::Cancellable::NONE, move |result| {
                if let Ok(file) = result {
                    if let Some(path) = file.path() {
                        let path_str = path.to_string_lossy().to_string();
                        iso_entry.set_text(&path_str);
                        let guessed = config::guess_os_type(&path_str);
                        if let Some(idx) = config::OS_TYPES.iter().position(|(k, _)| *k == guessed) {
                            os_dropdown.set_selected(idx as u32);
                        }
                    }
                }
            });
        });
    }

    // -- Hardware ----------------------------------------------------------
    let hardware_box = gtk::Box::new(Orientation::Vertical, 12);
    hardware_box.set_margin_top(10);
    hardware_box.set_margin_bottom(10);
    hardware_box.set_margin_start(10);
    hardware_box.set_margin_end(10);

    let max_cpu = (config::cpu_count() as f64).max(cpu0);
    let cpu_slider = HwSlider::new(
        "CPU cores",
        1.0,
        max_cpu,
        cpu0,
        1.0,
        Rc::new(|v: f64| format!("{}", v.round() as i64)),
        Rc::new(|s: &str| config::parse_number(s)),
    );
    hardware_box.append(&cpu_slider.widget);

    let ram_unit = settings.borrow().ram_unit.clone();
    let ram_max = (config::total_ram_mb() as f64).max(ram0);
    let unit_label = if ram_unit == "gb" { "GB" } else { "MB" };
    let ram_unit_disp = ram_unit.clone();
    let ram_unit_parse = ram_unit.clone();
    let ram_slider = HwSlider::new(
        &format!("RAM ({unit_label})"),
        256.0,
        ram_max,
        ram0,
        256.0,
        Rc::new(move |v: f64| config::format_ram(v, &ram_unit_disp)),
        Rc::new(move |s: &str| config::parse_ram(s, &ram_unit_parse)),
    );
    hardware_box.append(&ram_slider.widget);

    let disk_slider = HwSlider::new(
        "Storage (GB, 0 = no disk)",
        0.0,
        2000.0_f64.max(disk0),
        disk0,
        1.0,
        Rc::new(|v: f64| format!("{}", v.round() as i64)),
        Rc::new(|s: &str| config::parse_number(s)),
    );
    hardware_box.append(&disk_slider.widget);

    let available_disks = match existing.as_ref() {
        Some((vm_dir_ref, cfg_ref)) => disks::unused_disks_excluding(vm_dir_ref, cfg_ref),
        None => disks::unused_disks(),
    };
    let disk_labels: Vec<String> = available_disks
        .iter()
        .map(|d| format!("{} ({} GB)", disks::folder_label(&d.path), d.size_gb))
        .collect();
    let disk_label_refs: Vec<&str> = disk_labels.iter().map(String::as_str).collect();

    let reuse_check = gtk::CheckButton::with_label("Use an existing unused virtual disk instead of creating a new one");
    let reuse_dropdown = gtk::DropDown::from_strings(&disk_label_refs);

    let currently_external = cfg0.as_ref().and_then(|c| c.disk_path.as_ref());
    let preselected = currently_external.and_then(|dp| {
        available_disks.iter().position(|d| d.path.to_string_lossy() == *dp)
    });
    if let Some(idx) = preselected {
        reuse_dropdown.set_selected(idx as u32);
    }
    let start_reusing = preselected.is_some() && !available_disks.is_empty();
    reuse_check.set_active(start_reusing);
    if available_disks.is_empty() {
        reuse_check.set_sensitive(false);
        reuse_check.set_label(Some("No unused virtual disks available"));
    }
    disk_slider.widget.set_sensitive(!start_reusing);
    reuse_dropdown.set_sensitive(start_reusing);

    hardware_box.append(&reuse_check);
    hardware_box.append(&reuse_dropdown);

    {
        let disk_slider_widget = disk_slider.widget.clone();
        let reuse_dropdown = reuse_dropdown.clone();
        reuse_check.connect_toggled(move |cb| {
            let active = cb.is_active();
            disk_slider_widget.set_sensitive(!active);
            reuse_dropdown.set_sensitive(active);
        });
    }

    if is_edit {
        let note = gtk::Label::new(Some(
            "Growing storage is safe. Shrinking or removing it can destroy data on the disk.",
        ));
        note.set_wrap(true);
        note.set_halign(Align::Start);
        note.add_css_class("dim-label");
        hardware_box.append(&note);
    }

    let hardware_frame = gtk::Frame::new(Some("Hardware"));
    hardware_frame.set_child(Some(&hardware_box));
    root.append(&hardware_frame);

    // -- Network -------------------------------------------------------------
    let network_box = gtk::Box::new(Orientation::Vertical, 0);
    network_box.set_margin_top(10);
    network_box.set_margin_bottom(10);
    network_box.set_margin_start(10);
    network_box.set_margin_end(10);
    let net_check = gtk::CheckButton::with_label("Internet access (NAT)");
    net_check.set_active(net0 == "nat");
    network_box.append(&net_check);

    let network_frame = gtk::Frame::new(Some("Network"));
    network_frame.set_child(Some(&network_box));
    root.append(&network_frame);

    // -- Buttons -----------------------------------------------------------
    let btn_row = gtk::Box::new(Orientation::Horizontal, 8);
    btn_row.set_halign(Align::End);
    let cancel_btn = gtk::Button::with_label("Cancel");
    let save_btn = gtk::Button::with_label(if is_edit { "Save" } else { "Create" });
    save_btn.add_css_class("suggested-action");
    btn_row.append(&cancel_btn);
    btn_row.append(&save_btn);
    root.append(&btn_row);

    {
        let window = window.clone();
        cancel_btn.connect_clicked(move |_| window.close());
    }

    {
        let window = window.clone();
        let name_entry = name_entry.clone();
        let iso_entry = iso_entry.clone();
        let net_check = net_check.clone();
        let os_dropdown = os_dropdown.clone();
        let reuse_check = reuse_check.clone();
        let reuse_dropdown = reuse_dropdown.clone();
        let existing = existing.clone();
        let on_saved = on_saved.clone();

        save_btn.connect_clicked(move |_| {
            let name = name_entry.text().trim().to_string();
            let iso = iso_entry.text().trim().to_string();

            if name.is_empty() {
                alert(&window, "Missing name", "Please enter a display name.");
                return;
            }
            if iso.is_empty() || !PathBuf::from(&iso).is_file() {
                alert(&window, "Missing ISO", "Please choose a valid ISO file.");
                return;
            }

            let os_type = config::OS_TYPES
                .get(os_dropdown.selected() as usize)
                .map(|(k, _)| k.to_string())
                .unwrap_or_else(|| "other".to_string());

            let (disk_size_gb, disk_path) = if reuse_check.is_active() {
                match available_disks.get(reuse_dropdown.selected() as usize) {
                    Some(d) => (d.size_gb, Some(d.path.to_string_lossy().to_string())),
                    None => (0, None),
                }
            } else {
                (disk_slider.value().round() as u32, None)
            };

            let new_cfg = VmConfig {
                name,
                iso,
                cpus: cpu_slider.value().round() as u32,
                ram_mb: ram_slider.value().round() as u32,
                disk_size_gb,
                network: if net_check.is_active() { "nat" } else { "none" }.to_string(),
                os_type,
                disk_path,
            };

            match &existing {
                None => save_new(&window, new_cfg, on_saved.clone()),
                Some((vm_dir, old_cfg)) => {
                    save_edit(&window, vm_dir.clone(), old_cfg.clone(), new_cfg, on_saved.clone())
                }
            }
        });
    }

    let _ = vm_dir; // kept alive via `existing` captured above
    window.present();
}

fn save_new(window: &gtk::Window, cfg: VmConfig, on_saved: Rc<dyn Fn()>) {
    let slug = config::unique_slug(&cfg.name);
    let vm_dir = config::vms_dir().join(slug);
    if let Err(e) = std::fs::create_dir_all(&vm_dir) {
        alert(window, "Failed to create VM", &e.to_string());
        return;
    }

    // Only create a fresh disk when not attaching a reused one.
    if cfg.disk_path.is_none() && cfg.disk_size_gb > 0 {
        let path = vm_dir.join("disk.qcow2");
        if let Err(e) = qemu::create_disk(&path, cfg.disk_size_gb) {
            let _ = std::fs::remove_dir_all(&vm_dir);
            alert(window, "Failed to create disk", &e);
            return;
        }
    }

    if let Err(e) = config::save_config(&vm_dir, &cfg) {
        alert(window, "Failed to save VM", &e.to_string());
        return;
    }
    let _ = qemu::write_run_sh(&vm_dir, &cfg);

    on_saved();
    window.close();
}

fn save_edit(window: &gtk::Window, vm_dir: PathBuf, old_cfg: VmConfig, new_cfg: VmConfig, on_saved: Rc<dyn Fn()>) {
    let finalize = {
        let vm_dir = vm_dir.clone();
        let new_cfg = new_cfg.clone();
        let window = window.clone();
        let on_saved = on_saved.clone();
        move || {
            if let Err(e) = config::save_config(&vm_dir, &new_cfg) {
                alert(&window, "Failed to save VM", &e.to_string());
                return;
            }
            let _ = qemu::write_run_sh(&vm_dir, &new_cfg);
            on_saved();
            window.close();
        }
    };

    // Attaching a different (already-existing) disk: nothing to create/resize,
    // and we leave whatever local disk this VM had alone (it becomes orphaned,
    // visible and reusable later via the Disks browser).
    let switching_to_external = new_cfg.disk_path.is_some() && new_cfg.disk_path != old_cfg.disk_path;
    if switching_to_external {
        finalize();
        return;
    }

    // Detaching a reused disk in favor of a freshly-created local one.
    let switching_away_from_external = old_cfg.disk_path.is_some() && new_cfg.disk_path.is_none();
    let old_size = if switching_away_from_external { 0 } else { old_cfg.disk_size_gb };
    let new_size = new_cfg.disk_size_gb;
    let local_path = vm_dir.join("disk.qcow2");

    if new_size == old_size {
        finalize();
    } else if old_size == 0 && new_size > 0 {
        match qemu::create_disk(&local_path, new_size) {
            Ok(()) => finalize(),
            Err(e) => alert(window, "Failed to create disk", &e),
        }
    } else if old_size > 0 && new_size == 0 {
        let local_path2 = local_path.clone();
        let window2 = window.clone();
        confirm(
            window,
            "Remove virtual disk",
            "Setting storage to 0 will permanently delete this VM's virtual disk and everything on it. Continue?",
            "Delete",
            move || {
                if let Err(e) = std::fs::remove_file(&local_path2) {
                    alert(&window2, "Failed to remove disk", &e.to_string());
                    return;
                }
                finalize();
            },
        );
    } else if new_size > old_size {
        match qemu::resize_disk(&local_path, new_size, false) {
            Ok(()) => finalize(),
            Err(e) => alert(window, "Failed to resize disk", &e),
        }
    } else {
        let local_path2 = local_path.clone();
        let window2 = window.clone();
        confirm(
            window,
            "Shrink virtual disk",
            &format!(
                "Shrinking storage from {old_size} GB to {new_size} GB can destroy data if the filesystem inside the VM isn't shrunk first. Continue anyway?"
            ),
            "Shrink",
            move || match qemu::resize_disk(&local_path2, new_size, true) {
                Ok(()) => finalize(),
                Err(e) => alert(&window2, "Failed to resize disk", &e),
            },
        );
    }
}

pub fn show_settings_dialog(parent: &gtk::ApplicationWindow, settings: Rc<RefCell<Settings>>, on_saved: Rc<dyn Fn()>) {
    let window = gtk::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title("Settings")
        .default_width(380)
        .resizable(false)
        .build();

    let root = gtk::Box::new(Orientation::Vertical, 14);
    root.set_margin_top(16);
    root.set_margin_bottom(16);
    root.set_margin_start(16);
    root.set_margin_end(16);
    window.set_child(Some(&root));

    let ram_box = gtk::Box::new(Orientation::Vertical, 8);
    let ram_title = gtk::Label::new(Some("Show RAM as:"));
    ram_title.set_halign(Align::Start);
    ram_box.append(&ram_title);

    let mb_radio = gtk::CheckButton::with_label("Megabytes (4096 MB)");
    let gb_radio = gtk::CheckButton::with_label("Gigabytes (4,0 GB)");
    gb_radio.set_group(Some(&mb_radio));
    if settings.borrow().ram_unit == "gb" {
        gb_radio.set_active(true);
    } else {
        mb_radio.set_active(true);
    }
    ram_box.append(&mb_radio);
    ram_box.append(&gb_radio);

    let frame = gtk::Frame::new(Some("Display"));
    frame.set_child(Some(&ram_box));
    {
        let inner = &ram_box;
        inner.set_margin_top(10);
        inner.set_margin_bottom(10);
        inner.set_margin_start(10);
        inner.set_margin_end(10);
    }
    root.append(&frame);

    let btn_row = gtk::Box::new(Orientation::Horizontal, 8);
    btn_row.set_halign(Align::End);
    let cancel_btn = gtk::Button::with_label("Cancel");
    let save_btn = gtk::Button::with_label("Save");
    save_btn.add_css_class("suggested-action");
    btn_row.append(&cancel_btn);
    btn_row.append(&save_btn);
    root.append(&btn_row);

    {
        let window = window.clone();
        cancel_btn.connect_clicked(move |_| window.close());
    }
    {
        let window = window.clone();
        save_btn.connect_clicked(move |_| {
            let mut s = settings.borrow_mut();
            s.ram_unit = if gb_radio.is_active() { "gb" } else { "mb" }.to_string();
            config::save_settings(&s);
            drop(s);
            on_saved();
            window.close();
        });
    }

    window.present();
}

pub fn show_delete_vm_dialog(
    parent: &gtk::ApplicationWindow,
    vm_name: &str,
    on_confirm: impl Fn(bool) + 'static,
) {
    let window = gtk::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title("Delete VM")
        .default_width(380)
        .resizable(false)
        .build();

    let root = gtk::Box::new(Orientation::Vertical, 12);
    root.set_margin_top(16);
    root.set_margin_bottom(16);
    root.set_margin_start(16);
    root.set_margin_end(16);
    window.set_child(Some(&root));

    let message = gtk::Label::new(Some(&format!("Delete \"{vm_name}\"?")));
    message.set_halign(Align::Start);
    message.add_css_class("heading");
    root.append(&message);

    let keep_check = gtk::CheckButton::with_label("Keep the virtual disk (don't delete disk.qcow2)");
    root.append(&keep_check);

    let btn_row = gtk::Box::new(Orientation::Horizontal, 8);
    btn_row.set_halign(Align::End);
    btn_row.set_margin_top(8);
    let cancel_btn = gtk::Button::with_label("Cancel");
    let delete_btn = gtk::Button::with_label("Delete");
    delete_btn.add_css_class("destructive-action");
    btn_row.append(&cancel_btn);
    btn_row.append(&delete_btn);
    root.append(&btn_row);

    {
        let window = window.clone();
        cancel_btn.connect_clicked(move |_| window.close());
    }
    {
        let window = window.clone();
        delete_btn.connect_clicked(move |_| {
            on_confirm(keep_check.is_active());
            window.close();
        });
    }

    window.present();
}

pub fn show_disks_dialog(parent: &gtk::ApplicationWindow) {
    let window = gtk::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title("Virtual Disks")
        .default_width(480)
        .default_height(360)
        .build();

    let root = gtk::Box::new(Orientation::Vertical, 10);
    root.set_margin_top(16);
    root.set_margin_bottom(16);
    root.set_margin_start(16);
    root.set_margin_end(16);
    window.set_child(Some(&root));

    let list_box = gtk::ListBox::new();
    list_box.set_selection_mode(gtk::SelectionMode::None);
    list_box.add_css_class("boxed-list");

    let scroller = gtk::ScrolledWindow::new();
    scroller.set_vexpand(true);
    scroller.set_child(Some(&list_box));
    root.append(&scroller);

    let close_row = gtk::Box::new(Orientation::Horizontal, 8);
    close_row.set_halign(Align::End);
    let close_btn = gtk::Button::with_label("Close");
    close_row.append(&close_btn);
    root.append(&close_row);
    {
        let window = window.clone();
        close_btn.connect_clicked(move |_| window.close());
    }

    populate_disks_list(&list_box, &window);

    window.present();
}

fn populate_disks_list(list_box: &gtk::ListBox, window: &gtk::Window) {
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }
    let disks = disks::scan_disks();
    if disks.is_empty() {
        let empty = gtk::Label::new(Some("No virtual disks found."));
        empty.set_margin_top(20);
        empty.set_margin_bottom(20);
        empty.add_css_class("dim-label");
        list_box.append(&empty);
        return;
    }
    for disk in disks {
        let row = gtk::Box::new(Orientation::Horizontal, 10);
        row.set_margin_top(8);
        row.set_margin_bottom(8);
        row.set_margin_start(10);
        row.set_margin_end(10);

        let name = gtk::Label::new(Some(&disks::folder_label(&disk.path)));
        name.set_halign(Align::Start);
        name.set_hexpand(true);
        row.append(&name);

        let size = gtk::Label::new(Some(&format!("{} GB", disk.size_gb)));
        size.add_css_class("dim-label");
        row.append(&size);

        let status_text = match &disk.used_by {
            Some(vm_name) => format!("Used by {vm_name}"),
            None => "Not used".to_string(),
        };
        let status = gtk::Label::new(Some(&status_text));
        status.add_css_class(if disk.used_by.is_some() { "dim-label" } else { "success" });
        row.append(&status);

        let delete_btn = gtk::Button::with_label("Delete");
        delete_btn.add_css_class("destructive-action");
        row.append(&delete_btn);

        let path = disk.path.clone();
        let used_by = disk.used_by.clone();
        let window = window.clone();
        let list_box_for_closure = list_box.clone();
        delete_btn.connect_clicked(move |_| {
            let path = path.clone();
            let window2 = window.clone();
            let list_box = list_box_for_closure.clone();
            let detail = match &used_by {
                Some(vm_name) => format!(
                    "\"{vm_name}\" is currently using this disk. Deleting it will break that VM \
                     until you attach a different disk or create a new one. This cannot be undone."
                ),
                None => "This permanently deletes the disk file. This cannot be undone.".to_string(),
            };
            confirm(
                &window,
                "Delete virtual disk",
                &detail,
                "Delete",
                move || {
                    let _ = std::fs::remove_file(&path);
                    populate_disks_list(&list_box, &window2);
                },
            );
        });

        list_box.append(&row);
    }
}

#[allow(unused)]
fn suppress_unused(_: glib::Value) {}
