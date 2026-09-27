use gtk::prelude::*;
use gtk::{Align, Orientation};
use std::cell::Cell;
use std::rc::Rc;

/// A GtkScale paired with an entry box that accepts typed values,
/// kept in sync in both directions.
#[allow(dead_code)]
pub struct HwSlider {
    pub widget: gtk::Box,
    scale: gtk::Scale,
    entry: gtk::Entry,
    to_display: Rc<dyn Fn(f64) -> String>,
    updating: Rc<Cell<bool>>,
}

impl HwSlider {
    pub fn new(
        label_text: &str,
        min: f64,
        max: f64,
        initial: f64,
        step: f64,
        to_display: Rc<dyn Fn(f64) -> String>,
        from_display: Rc<dyn Fn(&str) -> Option<f64>>,
    ) -> Self {
        let widget = gtk::Box::new(Orientation::Vertical, 4);

        let label = gtk::Label::new(Some(label_text));
        label.set_halign(Align::Start);
        widget.append(&label);

        let row = gtk::Box::new(Orientation::Horizontal, 10);

        let adjustment = gtk::Adjustment::new(initial, min, max.max(min + 1.0), step, step * 10.0, 0.0);
        let scale = gtk::Scale::new(Orientation::Horizontal, Some(&adjustment));
        scale.set_hexpand(true);
        scale.set_draw_value(false);
        scale.set_size_request(200, -1);

        let entry = gtk::Entry::new();
        entry.set_width_chars(10);
        gtk::prelude::EditableExt::set_alignment(&entry, 1.0);
        entry.set_text(&to_display(initial));

        row.append(&scale);
        row.append(&entry);
        widget.append(&row);

        let updating = Rc::new(Cell::new(false));

        {
            let entry = entry.clone();
            let updating = updating.clone();
            let to_display = to_display.clone();
            scale.connect_value_changed(move |s| {
                if updating.get() {
                    return;
                }
                updating.set(true);
                entry.set_text(&to_display(s.value()));
                updating.set(false);
            });
        }

        let commit: Rc<dyn Fn()> = {
            let scale = scale.clone();
            let entry = entry.clone();
            let updating = updating.clone();
            let to_display = to_display.clone();
            Rc::new(move || {
                if updating.get() {
                    return;
                }
                let text = entry.text().to_string();
                let v = from_display(&text).unwrap_or_else(|| scale.value());
                let v = v.clamp(min, max);
                updating.set(true);
                scale.set_value(v);
                entry.set_text(&to_display(v));
                updating.set(false);
            })
        };

        {
            let commit = commit.clone();
            entry.connect_activate(move |_| commit());
        }
        {
            let commit = commit.clone();
            let focus = gtk::EventControllerFocus::new();
            focus.connect_leave(move |_| commit());
            entry.add_controller(focus);
        }

        HwSlider {
            widget,
            scale,
            entry,
            to_display,
            updating,
        }
    }

    pub fn value(&self) -> f64 {
        self.scale.value()
    }

    pub fn set_value(&self, v: f64) {
        self.updating.set(true);
        self.scale.set_value(v);
        self.entry.set_text(&(self.to_display)(v));
        self.updating.set(false);
    }
}
