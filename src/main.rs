mod config;
mod dialogs;
mod disks;
mod hwslider;
mod icons;
mod qemu;

use config::VmConfig;
use gtk::glib;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

const APP_ID: &str = "io.github.Jeksonkd.Qemux";
const APP_TITLE: &str = "Qemux";

fn alert(window: &gtk::Window, message: &str, detail: &str) {
    let dialog = gtk::AlertDialog::builder()
        .modal(true)
        .message(message)
        .detail(detail)
        .buttons(["OK"])
        .build();
    dialog.show(Some(window));
}

fn list_css(font_pt: f64) -> String {
    format!(
        "/* Force a blue accent everywhere, regardless of the ambient GTK
           theme (some fallback themes/sandboxes render accents as a dull
           yellow when no desktop-specific theme is available). */
         @define-color accent_color #1c71d8;
         @define-color accent_bg_color #1c71d8;
         @define-color accent_fg_color #ffffff;

         list, listview {{ background: transparent; }}
         row {{ font-size: {font_pt:.1}pt; }}
         .vm-row {{
            border-bottom: 1px solid rgba(127,127,127,0.15);
         }}
         row:selected .vm-row {{
            background-color: rgba(28,113,216,0.20);
         }}"
    )
}

fn main() -> glib::ExitCode {
    let app = gtk::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &gtk::Application) {
    let settings = Rc::new(RefCell::new(config::load_settings()));
    let vms: Rc<RefCell<HashMap<String, (PathBuf, VmConfig)>>> = Rc::new(RefCell::new(HashMap::new()));

    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title(APP_TITLE)
        .default_width(820)
        .default_height(480)
        .build();

    let header = gtk::HeaderBar::new();
    header.set_title_widget(Some(&gtk::Label::new(Some(APP_TITLE))));
    window.set_titlebar(Some(&header));

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    window.set_child(Some(&root));

    // -- Toolbar -----------------------------------------------------------
    let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    toolbar.set_margin_top(12);
    toolbar.set_margin_bottom(12);
    toolbar.set_margin_start(12);
    toolbar.set_margin_end(12);
    root.append(&toolbar);

    let new_btn = gtk::Button::with_label("New VM…");
    new_btn.add_css_class("suggested-action");
    let edit_btn = gtk::Button::with_label("Edit…");
    let launch_btn = gtk::Button::with_label("Launch");
    let delete_btn = gtk::Button::with_label("Delete");
    toolbar.append(&new_btn);
    toolbar.append(&edit_btn);
    toolbar.append(&launch_btn);
    toolbar.append(&delete_btn);

    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    toolbar.append(&spacer);

    let disks_btn = gtk::Button::with_label("Disks…");
    let refresh_btn = gtk::Button::with_label("Refresh");
    let settings_btn = gtk::Button::with_label("Settings…");
    toolbar.append(&disks_btn);
    toolbar.append(&refresh_btn);
    toolbar.append(&settings_btn);

    // -- List view -------------------------------------------------------------
    const COL_CPUS_WIDTH: i32 = 60;
    const COL_RAM_WIDTH: i32 = 90;
    const COL_DISK_WIDTH: i32 = 80;
    const COL_NET_WIDTH: i32 = 100;
    const LIST_ICON_SIZE: i32 = 21;
    const WINDOW_BASELINE_WIDTH: f64 = 820.0;

    let list_header = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    list_header.set_margin_start(7);
    list_header.set_margin_end(7);
    list_header.set_margin_bottom(3);
    {
        let name_h = gtk::Label::new(Some("Name"));
        name_h.set_hexpand(true);
        name_h.set_halign(gtk::Align::Start);
        name_h.add_css_class("dim-label");
        list_header.append(&name_h);
        for (title, width) in [
            ("CPUs", COL_CPUS_WIDTH),
            ("RAM", COL_RAM_WIDTH),
            ("Disk", COL_DISK_WIDTH),
            ("Network", COL_NET_WIDTH),
        ] {
            let h = gtk::Label::new(Some(title));
            h.set_size_request(width, -1);
            h.set_halign(gtk::Align::Start);
            h.add_css_class("dim-label");
            list_header.append(&h);
        }
    }

    let list_box = gtk::ListBox::new();
    list_box.set_selection_mode(gtk::SelectionMode::Single);
    list_box.set_activate_on_single_click(false);

    let css = gtk::CssProvider::new();
    css.load_from_string(&list_css(11.5));
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    let list_scroller = gtk::ScrolledWindow::new();
    list_scroller.set_vexpand(true);
    list_scroller.set_hexpand(true);
    list_scroller.set_child(Some(&list_box));

    let list_page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    list_page.set_margin_start(12);
    list_page.set_margin_end(12);
    list_page.set_margin_bottom(12);
    list_page.append(&list_header);
    list_page.append(&list_scroller);
    root.append(&list_page);

    let ui_scale: Rc<Cell<f64>> = Rc::new(Cell::new(1.0));

    // -- Refresh -------------------------------------------------------------
    let refresh: Rc<dyn Fn()> = {
        let list_box = list_box.clone();
        let vms = vms.clone();
        let settings = settings.clone();
        let ui_scale = ui_scale.clone();
        Rc::new(move || {
            let unit = settings.borrow().ram_unit.clone();

            while let Some(child) = list_box.first_child() {
                list_box.remove(&child);
            }

            let mut map = vms.borrow_mut();
            map.clear();
            for (slug, vm_dir, cfg) in config::load_vms() {
                let ram = config::format_ram(cfg.ram_mb as f64, &unit);
                let disk = config::format_disk(cfg.disk_size_gb);
                let net = if cfg.network == "nat" { "NAT" } else { "None" };

                // -- list row --------------------------------------------------
                let scale = ui_scale.get();
                let list_icon_size = (LIST_ICON_SIZE as f64 * scale).round() as i32;

                let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 4);
                row_box.add_css_class("vm-row");
                row_box.set_margin_top(3);
                row_box.set_margin_bottom(3);
                row_box.set_margin_start(4);
                row_box.set_margin_end(4);

                // Render well above display size so the downscale stays crisp.
                if let Some(pix) = icons::os_pixbuf(&cfg.os_type, list_icon_size * 3) {
                    let texture = gtk::gdk::Texture::for_pixbuf(&pix);
                    let picture = gtk::Picture::for_paintable(&texture);
                    picture.set_content_fit(gtk::ContentFit::Contain);
                    picture.set_size_request(list_icon_size, list_icon_size);
                    picture.set_valign(gtk::Align::Center);
                    row_box.append(&picture);
                }

                let name_label = gtk::Label::new(Some(&cfg.name));
                name_label.set_hexpand(true);
                name_label.set_halign(gtk::Align::Start);
                name_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
                row_box.append(&name_label);

                let cpus_label = gtk::Label::new(Some(&cfg.cpus.to_string()));
                cpus_label.set_size_request(COL_CPUS_WIDTH, -1);
                cpus_label.set_halign(gtk::Align::Start);
                row_box.append(&cpus_label);

                let ram_label = gtk::Label::new(Some(&ram));
                ram_label.set_size_request(COL_RAM_WIDTH, -1);
                ram_label.set_halign(gtk::Align::Start);
                row_box.append(&ram_label);

                let disk_label = gtk::Label::new(Some(&disk));
                disk_label.set_size_request(COL_DISK_WIDTH, -1);
                disk_label.set_halign(gtk::Align::Start);
                row_box.append(&disk_label);

                let net_label = gtk::Label::new(Some(net));
                net_label.set_size_request(COL_NET_WIDTH, -1);
                net_label.set_halign(gtk::Align::Start);
                row_box.append(&net_label);

                let list_row = gtk::ListBoxRow::new();
                list_row.set_child(Some(&row_box));
                list_row.set_widget_name(&slug);
                list_box.append(&list_row);

                map.insert(slug, (vm_dir, cfg));
            }
        })
    };
    refresh();

    // The whole UI (icons, list font) resizes with the window, but damped
    // (1:4) rather than 1:1, so it doesn't balloon or shrink as fast as the
    // window does.
    {
        let window_ref = window.clone();
        let css = css.clone();
        let ui_scale = ui_scale.clone();
        let refresh = refresh.clone();
        window.add_tick_callback(move |_widget, _clock| {
            let w = window_ref.width() as f64;
            if w > 0.0 {
                let raw_ratio = w / WINDOW_BASELINE_WIDTH;
                let damped = 1.0 + (raw_ratio - 1.0) / 4.0;
                let damped = damped.clamp(0.75, 1.8);
                if (damped - ui_scale.get()).abs() > 0.04 {
                    ui_scale.set(damped);
                    css.load_from_string(&list_css(11.5 * damped));
                    refresh();
                }
            }
            glib::ControlFlow::Continue
        });
    }

    let selected_vm = {
        let list_box = list_box.clone();
        let vms = vms.clone();
        move || -> Option<(PathBuf, VmConfig)> {
            let slug = list_box.selected_row()?.widget_name().to_string();
            vms.borrow().get(&slug).cloned()
        }
    };

    // -- New VM --------------------------------------------------------------
    {
        let window = window.clone();
        let settings = settings.clone();
        let refresh = refresh.clone();
        new_btn.connect_clicked(move |_| {
            dialogs::show_vm_dialog(&window, settings.clone(), None, refresh.clone());
        });
    }

    // -- Edit ------------------------------------------------------------------
    let do_edit: Rc<dyn Fn()> = {
        let window = window.clone();
        let settings = settings.clone();
        let refresh = refresh.clone();
        let selected_vm = selected_vm.clone();
        Rc::new(move || {
            let Some((vm_dir, cfg)) = selected_vm() else {
                alert(window.upcast_ref(), "No selection", "Select a VM first.");
                return;
            };
            dialogs::show_vm_dialog(&window, settings.clone(), Some((vm_dir, cfg)), refresh.clone());
        })
    };
    {
        let do_edit = do_edit.clone();
        edit_btn.connect_clicked(move |_| do_edit());
    }

    // -- Launch ------------------------------------------------------------------
    let do_launch: Rc<dyn Fn()> = {
        let window = window.clone();
        let selected_vm = selected_vm.clone();
        Rc::new(move || {
            let Some((vm_dir, cfg)) = selected_vm() else {
                alert(window.upcast_ref(), "No selection", "Select a VM first.");
                return;
            };
            if let Err(e) = qemu::launch(&vm_dir, &cfg) {
                alert(window.upcast_ref(), "Failed to launch VM", &e.to_string());
            }
        })
    };
    {
        let do_launch = do_launch.clone();
        launch_btn.connect_clicked(move |_| do_launch());
    }
    {
        let do_launch = do_launch.clone();
        list_box.connect_row_activated(move |_, _| do_launch());
    }

    // -- Delete ------------------------------------------------------------------
    let do_delete_prompt: Rc<dyn Fn()> = {
        let window = window.clone();
        let refresh = refresh.clone();
        let selected_vm = selected_vm.clone();
        Rc::new(move || {
            let Some((vm_dir, cfg)) = selected_vm() else {
                alert(window.upcast_ref(), "No selection", "Select a VM first.");
                return;
            };
            let refresh = refresh.clone();
            dialogs::show_delete_vm_dialog(&window, &cfg.name, move |keep_disk| {
                if keep_disk {
                    let _ = std::fs::remove_file(vm_dir.join("config.json"));
                    let _ = std::fs::remove_file(vm_dir.join("run.sh"));
                    let _ = std::fs::remove_file(vm_dir.join("qemu.log"));
                } else {
                    let _ = std::fs::remove_dir_all(&vm_dir);
                }
                refresh();
            });
        })
    };
    {
        let do_delete_prompt = do_delete_prompt.clone();
        delete_btn.connect_clicked(move |_| do_delete_prompt());
    }

    // -- Disks / Settings / Refresh --------------------------------------------------------
    {
        let window = window.clone();
        disks_btn.connect_clicked(move |_| {
            dialogs::show_disks_dialog(&window);
        });
    }
    {
        let window = window.clone();
        let settings = settings.clone();
        let refresh = refresh.clone();
        settings_btn.connect_clicked(move |_| {
            dialogs::show_settings_dialog(&window, settings.clone(), refresh.clone());
        });
    }
    {
        let refresh = refresh.clone();
        refresh_btn.connect_clicked(move |_| refresh());
    }

    // -- Keyboard shortcuts: Space = launch, E = edit, Delete = delete ------------------
    {
        let key_controller = gtk::EventControllerKey::new();
        key_controller.set_propagation_phase(gtk::PropagationPhase::Bubble);
        let do_launch = do_launch.clone();
        let do_edit = do_edit.clone();
        let do_delete_prompt = do_delete_prompt.clone();
        key_controller.connect_key_pressed(move |_, keyval, _keycode, _state| {
            match keyval {
                gtk::gdk::Key::space => {
                    do_launch();
                    glib::Propagation::Stop
                }
                gtk::gdk::Key::e | gtk::gdk::Key::E => {
                    do_edit();
                    glib::Propagation::Stop
                }
                gtk::gdk::Key::Delete => {
                    do_delete_prompt();
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        window.add_controller(key_controller);
    }

    window.present();
}
