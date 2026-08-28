use super::{g, label, Shared};
use crate::clip_store::ClipStore;
use crate::services::ClipKind;
use gtk4::prelude::*;
use gtk4::{Button, Entry, GestureClick, Label, ListBox, ListBoxRow};
use std::rc::Rc;

pub struct ClipPage {
    root: gtk4::Box,
    list: ListBox,
    empty: Label,
}

impl ClipPage {
    pub fn build(shared: &Rc<Shared>) -> Self {
        let root = super::vbox(10);
        root.set_css_classes(&["na-panel-pad"]);

        let head = super::hbox(8);
        let search = Entry::new();
        search.set_placeholder_text(Some("Search clipboard"));
        search.set_css_classes(&["na-entry"]);
        search.set_hexpand(true);
        let clear_btn = Button::with_label("Clear");
        clear_btn.set_css_classes(&["na-btn", "ghost"]);
        {
            let sh = shared.clone();
            clear_btn.connect_clicked(move |_| {
                sh.clips.borrow_mut().clear_unpinned();
                crate::app::refresh_clips();
            });
        }
        head.append(&search);
        head.append(&clear_btn);
        root.append(&head);

        let scroll = gtk4::ScrolledWindow::new();
        scroll.set_vexpand(true);
        scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
        scroll.set_css_classes(&["na-scroll"]);
        scroll.set_min_content_height(180);

        let list = ListBox::new();
        list.set_selection_mode(gtk4::SelectionMode::None);
        scroll.set_child(Some(&list));
        root.append(&scroll);

        let empty = label(&["na-empty"], "Copy anything. It shows up here.");
        empty.set_vexpand(true);
        empty.set_valign(gtk4::Align::Center);
        empty.set_halign(gtk4::Align::Center);
        root.append(&empty);

        {
            let sh = shared.clone();
            let l = list.clone();
            let em = empty.clone();
            search.connect_changed(move |e| {
                rebuild(&sh, &l, &em, Some(e.text().as_str()));
            });
        }

        let p = Self { root, list, empty };
        p.reload(None);
        p
    }

    pub fn root(&self) -> &gtk4::Box {
        &self.root
    }

    pub fn reload(&self, filter: Option<&str>) {
        super::with_shared(|sh| rebuild(sh, &self.list, &self.empty, filter));
    }
}

fn rebuild(shared: &Rc<Shared>, list: &ListBox, empty: &Label, filter: Option<&str>) {
    while let Some(c) = list.first_child() {
        list.remove(&c);
    }
    let entries: Vec<_> = shared.clips.borrow().entries.clone();
    let f = filter.map(|s| s.to_lowercase()).unwrap_or_default();
    let shown: Vec<_> = entries
        .iter()
        .filter(|e| f.is_empty() || e.preview.to_lowercase().contains(&f))
        .cloned()
        .collect();
    empty.set_visible(shown.is_empty());
    list.set_visible(!shown.is_empty());
    for e in shown {
        let row = clip_row(shared, e);
        list.append(&row);
    }
}

fn ago(ts: u64) -> String {
    let now = super::now_secs();
    match now.saturating_sub(ts) {
        0 => "now".into(),
        s if s < 60 => format!("{s}s"),
        m if m < 3600 => format!("{}m", m / 60),
        h if h < 86400 => format!("{}h", h / 3600),
        d => format!("{}d", d / 86400),
    }
}

fn clip_row(shared: &Rc<Shared>, e: crate::services::ClipEntry) -> ListBoxRow {
    let row = ListBoxRow::new();
    row.set_css_classes(&["na-clip-row"]);
    row.set_activatable(true);

    let h = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);

    let kind = match e.kind {
        ClipKind::Image => g::IMAGE,
        ClipKind::Text => g::TEXT,
    };
    let kind_l = label(&["na-kind"], kind);

    let pin = if e.pinned { "  ★" } else { "" };
    let preview_txt = match e.kind {
        ClipKind::Image => format!(
            "Image ({}){pin}",
            crate::util::human_size(blob_size(shared, &e))
        ),
        ClipKind::Text => format!("{}{pin}", e.preview.replace('\n', " ⏎ ")),
    };
    let prev = label(&["na-clip-preview"], &preview_txt);
    prev.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    prev.set_max_width_chars(52);
    prev.set_hexpand(true);
    prev.set_xalign(0.0);

    let when = label(&["na-mute"], &ago(e.at));

    h.append(&kind_l);
    h.append(&prev);
    h.append(&when);
    row.set_child(Some(&h));

    let click = GestureClick::new();
    click.set_button(1);
    {
        let e2 = e.clone();
        let sh2 = shared.clone();
        click.connect_released(move |_g, n, _x, _y| {
            if n == 1 {
                copy_entry_to_clipboard(&e2, &sh2.clips.borrow());
            }
        });
    }
    row.add_controller(click);

    let right = GestureClick::new();
    right.set_button(3);
    {
        let e2 = e.clone();
        let sh2 = shared.clone();
        let row2 = row.clone();
        right.connect_released(move |_g, _n, _x, _y| {
            show_menu(&sh2, &e2, &row2);
        });
    }
    row.add_controller(right);
    row
}

fn blob_size(shared: &Rc<Shared>, e: &crate::services::ClipEntry) -> usize {
    std::fs::metadata(shared.clips.borrow().blob_path(&e.data_ref))
        .map(|m| m.len() as usize)
        .unwrap_or(0)
}

fn show_menu(shared: &Rc<Shared>, e: &crate::services::ClipEntry, parent: &ListBoxRow) {
    let pop = gtk4::Popover::new();
    pop.add_css_class("na-pop");
    let menu = super::vbox(4);
    menu.set_margin_top(6);
    menu.set_margin_bottom(6);
    menu.set_margin_start(8);
    menu.set_margin_end(8);

    let mk = |txt: &str| -> Button {
        let b = Button::with_label(txt);
        b.set_has_frame(false);
        b.set_halign(gtk4::Align::Fill);
        b
    };

    let b_copy = mk("Copy");
    {
        let e2 = e.clone();
        let sh = shared.clone();
        let pop2 = pop.clone();
        b_copy.connect_clicked(move |_| {
            copy_entry_to_clipboard(&e2, &sh.clips.borrow());
            pop2.popdown();
        });
    }
    menu.append(&b_copy);

    let b_pin = mk(if e.pinned { "Unpin" } else { "Pin" });
    {
        let e2 = e.clone();
        let sh = shared.clone();
        let pop2 = pop.clone();
        b_pin.connect_clicked(move |_| {
            sh.clips.borrow_mut().toggle_pin(&e2.id);
            crate::app::refresh_clips();
            pop2.popdown();
        });
    }
    menu.append(&b_pin);

    let b_rm = mk("Remove");
    {
        let e2 = e.clone();
        let sh = shared.clone();
        let pop2 = pop.clone();
        b_rm.connect_clicked(move |_| {
            sh.clips.borrow_mut().remove(&e2.id);
            crate::app::refresh_clips();
            pop2.popdown();
        });
    }
    menu.append(&b_rm);

    pop.set_child(Some(&menu));
    pop.set_parent(parent);
    pop.popup();
}

/// Put an entry back on the Wayland clipboard (background thread).
pub fn copy_entry_to_clipboard(e: &crate::services::ClipEntry, store: &ClipStore) {
    match e.kind {
        ClipKind::Text => set_clipboard_text(e.text.clone()),
        ClipKind::Image => {
            let path = store.blob_path(&e.data_ref);
            std::thread::spawn(move || {
                if let Ok(data) = std::fs::read(path) {
                    use wl_clipboard_rs::copy::{self as wcopy, MimeType, Source};
                    wcopy::copy(
                        wcopy::Options::new(),
                        Source::Bytes(data.into_boxed_slice()),
                        MimeType::Specific("image/png".into()),
                    )
                    .ok();
                }
            });
        }
    }
}

pub fn set_clipboard_text(text: String) {
    std::thread::spawn(move || {
        use wl_clipboard_rs::copy::{self as wcopy, MimeType, Source};
        wcopy::copy(
            wcopy::Options::new(),
            Source::Bytes(text.into_bytes().into_boxed_slice()),
            MimeType::Text,
        )
        .ok();
    });
}
