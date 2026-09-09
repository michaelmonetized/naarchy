use super::{g, label, Shared};
use crate::clip_store::ClipStore;
use crate::services::ClipKind;
use gtk4::prelude::*;
use gtk4::{Button, Entry, GestureClick, Label, ListBox, ListBoxRow};
use std::rc::Rc;

type RowCache = Rc<std::cell::RefCell<std::collections::HashMap<String, (bool, ListBoxRow)>>>;

pub struct ClipPage {
    root: gtk4::Box,
    list: ListBox,
    empty: gtk4::Box,
    scroll: gtk4::ScrolledWindow,
    search: Entry,
    clear_btn: Button,
    status: Label,
    rows: RowCache,
}

impl ClipPage {
    pub fn build(shared: &Rc<Shared>) -> Self {
        let root = super::vbox(10);
        root.set_css_classes(&["na-panel-pad"]);
        root.append(&super::page_heading(
            "Clipboard",
            "A little memory for everything you copy.",
        ));

        let head = super::hbox(8);
        let search = Entry::new();
        search.set_placeholder_text(Some("Search text and images…"));
        search.set_css_classes(&["na-entry"]);
        search.set_hexpand(true);
        search.set_icon_from_icon_name(
            gtk4::EntryIconPosition::Primary,
            Some("system-search-symbolic"),
        );
        super::describe(&search, "Search clipboard history");
        let clear_btn = Button::with_label("Clear history");
        clear_btn.set_css_classes(&["na-btn", "ghost"]);
        super::describe(&clear_btn, "Clear history and keep pinned items");
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
        scroll.set_min_content_height(140);
        let list = ListBox::new();
        list.set_selection_mode(gtk4::SelectionMode::None);
        list.add_css_class("na-clip-list");
        scroll.set_child(Some(&list));
        root.append(&scroll);

        let empty = super::empty_state(
            g::CLIP,
            "Ready when you copy",
            "Copy text or an image in any app. Your recent clips will be waiting here.",
        );
        root.append(&empty);
        let status = label(&["na-mute"], "Enter to copy · Right-click for more");
        status.set_xalign(0.0);
        root.append(&status);
        let rows = Rc::new(std::cell::RefCell::new(std::collections::HashMap::new()));
        {
            let sh = shared.clone();
            let l = list.clone();
            let em = empty.clone();
            let sc = scroll.clone();
            let cb = clear_btn.clone();
            let status = status.clone();
            let rows = rows.clone();
            search.connect_changed(move |e| {
                rebuild(&sh, &l, &em, &sc, &cb, &status, &rows, e.text().as_str())
            });
        }
        {
            let list = list.clone();
            search.connect_activate(move |_| {
                if let Some(row) = list.row_at_index(0) {
                    row.grab_focus();
                    row.activate();
                }
            });
        }
        let p = Self {
            root,
            list,
            empty,
            scroll,
            search,
            clear_btn,
            status,
            rows,
        };
        rebuild(
            shared,
            &p.list,
            &p.empty,
            &p.scroll,
            &p.clear_btn,
            &p.status,
            &p.rows,
            "",
        );
        p
    }

    pub fn root(&self) -> &gtk4::Box {
        &self.root
    }

    pub fn focus_search(&self) {
        self.search.grab_focus();
    }

    pub fn reload(&self, filter: Option<&str>) {
        if let Some(filter) = filter {
            self.search.set_text(filter);
        }
        super::with_shared(|sh| {
            rebuild(
                sh,
                &self.list,
                &self.empty,
                &self.scroll,
                &self.clear_btn,
                &self.status,
                &self.rows,
                self.search.text().as_str(),
            )
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn rebuild(
    shared: &Rc<Shared>,
    list: &ListBox,
    empty: &gtk4::Box,
    scroll: &gtk4::ScrolledWindow,
    clear_btn: &Button,
    status: &Label,
    rows: &RowCache,
    filter: &str,
) {
    let query = filter.trim().to_lowercase();
    let (filtered, total, clearable) = {
        let store = shared.clips.borrow();
        (
            store
                .entries
                .iter()
                .filter(|e| query.is_empty() || e.preview.to_lowercase().contains(&query))
                .take(200)
                .cloned()
                .collect::<Vec<_>>(),
            store.entries.len(),
            store.entries.iter().any(|e| !e.pinned),
        )
    };
    let mut cache = rows.borrow_mut();
    // Reuse existing rows so clipboard updates preserve focus, hover and menus.
    for (index, entry) in filtered.iter().enumerate() {
        if cache
            .get(&entry.id)
            .is_some_and(|(pin, _)| *pin != entry.pinned)
        {
            if let Some((_, old)) = cache.remove(&entry.id) {
                if old.parent().is_some() {
                    list.remove(&old);
                }
            }
        }
        let (_, row) = cache
            .entry(entry.id.clone())
            .or_insert_with(|| (entry.pinned, clip_row(shared, entry)));
        if let Some(time) = row
            .child()
            .and_then(|child| child.last_child())
            .and_then(|child| child.downcast::<Label>().ok())
        {
            if time.text() != "Copied" {
                super::set_label_text(&time, &ago(entry.at));
            }
        }
        if list.row_at_index(index as i32).as_ref() != Some(row) {
            if row.parent().is_some() {
                list.remove(row);
            }
            list.insert(row, index as i32);
        }
    }
    while let Some(row) = list.row_at_index(filtered.len() as i32) {
        list.remove(&row);
    }
    let present: std::collections::HashSet<String> = shared
        .clips
        .borrow()
        .entries
        .iter()
        .map(|e| e.id.clone())
        .collect();
    cache.retain(|id, _| present.contains(id));
    if cache.len() > 400 {
        cache.retain(|id, _| filtered.iter().any(|e| &e.id == id));
    }
    empty.set_visible(filtered.is_empty());
    scroll.set_visible(!filtered.is_empty());
    clear_btn.set_sensitive(clearable);
    if let Some(title) = empty
        .first_child()
        .and_then(|c| c.next_sibling())
        .and_then(|c| c.downcast::<Label>().ok())
    {
        title.set_text(if query.is_empty() {
            "Ready when you copy"
        } else {
            "No matching clips"
        });
    }
    if let Some(detail) = empty.last_child().and_then(|c| c.downcast::<Label>().ok()) {
        detail.set_text(if query.is_empty() {
            "Copy text or an image in any app. Your recent clips will be waiting here."
        } else {
            "Try a different word or clear your search to see everything."
        });
    }
    status.set_text(&format!(
        "{} of {total} clips · Enter to copy · Right-click for more",
        filtered.len()
    ));
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

fn clip_row(shared: &Rc<Shared>, e: &crate::services::ClipEntry) -> ListBoxRow {
    let row = ListBoxRow::new();
    row.set_css_classes(&["na-clip-row"]);
    row.set_activatable(true);

    let h = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);

    let kind = match e.kind {
        ClipKind::Image => g::IMAGE,
        ClipKind::Text => g::TEXT,
    };
    let kind_l = label(&["na-kind"], kind);

    let preview_txt = match e.kind {
        ClipKind::Image => format!("Image ({})", crate::util::human_size(blob_size(shared, e))),
        ClipKind::Text => e.preview.replace('\n', " ⏎ "),
    };
    let prev = label(&["na-clip-preview"], &preview_txt);
    prev.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    prev.set_max_width_chars(48);
    prev.set_hexpand(true);
    prev.set_xalign(0.0);

    let pin_l = label(&["na-pin"], "★");
    pin_l.set_visible(e.pinned);
    pin_l.set_valign(gtk4::Align::Center);

    let when = label(&["na-clip-time"], &ago(e.at));

    h.append(&kind_l);
    h.append(&prev);
    h.append(&pin_l);
    h.append(&when);
    row.set_child(Some(&h));

    super::describe(&row, &format!("Copy {}", preview_txt));
    row.set_focusable(true);
    row.set_cursor_from_name(Some("pointer"));
    {
        let entry = e.clone();
        let sh = shared.clone();
        let when = when.clone();
        row.connect_activate(move |_| {
            copy_entry_to_clipboard(&entry, &sh.clips.borrow());
            when.set_text("Copied");
            when.add_css_class("na-feedback");
            let when = when.downgrade();
            let at = entry.at;
            gtk4::glib::timeout_add_local_once(std::time::Duration::from_secs(2), move || {
                if let Some(when) = when.upgrade() {
                    when.set_text(&ago(at));
                    when.remove_css_class("na-feedback");
                }
            });
        });
    }
    {
        let key = gtk4::EventControllerKey::new();
        let entry = e.clone();
        let sh = shared.clone();
        let weak_row = row.downgrade();
        key.connect_key_pressed(move |_, key, _, mods| {
            if key == gtk4::gdk::Key::Menu
                || (key == gtk4::gdk::Key::F10
                    && mods.contains(gtk4::gdk::ModifierType::SHIFT_MASK))
            {
                if let Some(row) = weak_row.upgrade() {
                    show_menu(&sh, &entry, &row);
                }
                gtk4::glib::Propagation::Stop
            } else {
                gtk4::glib::Propagation::Proceed
            }
        });
        row.add_controller(key);
    }

    let right = GestureClick::new();
    right.set_button(3);
    {
        let e2 = e.clone();
        let sh2 = shared.clone();
        let row2 = row.downgrade();
        right.connect_released(move |_g, _n, _x, _y| {
            if let Some(row) = row2.upgrade() {
                show_menu(&sh2, &e2, &row);
            }
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
    let (pop, menu) = super::context_menu();
    let mk = super::menu_button;

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
            pop2.popdown();
            crate::app::refresh_clips();
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
            pop2.popdown();
            crate::app::refresh_clips();
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
