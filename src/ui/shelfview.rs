use super::{g, label, Shared};
use crate::shelf_store::ShelfItem;
use gtk4::prelude::*;
use gtk4::{gdk, glib, Button, FlowBox, FlowBoxChild, GestureClick};
use std::rc::Rc;

pub struct ShelfPage {
    root: gtk4::Box,
    grid: FlowBox,
    empty: gtk4::Box,
    scroll: gtk4::ScrolledWindow,
    clear_btn: Button,
    count: gtk4::Label,
    rows: std::cell::RefCell<std::collections::HashMap<String, (bool, FlowBoxChild)>>,
}

impl ShelfPage {
    pub fn build(shared: &Rc<Shared>) -> Self {
        let root = super::vbox(8);
        root.set_css_classes(&["na-panel-pad"]);

        let head = super::hbox(8);
        let heading = super::page_heading("Inbox", "Drop it here. Pick it up anywhere.");
        heading.set_hexpand(true);
        let clear_btn = Button::with_label("Clear inbox");
        clear_btn.set_css_classes(&["na-btn", "ghost"]);
        {
            let sh = shared.clone();
            clear_btn.connect_clicked(move |_| {
                sh.shelf.borrow_mut().clear();
                crate::app::refresh_after_shelf_change();
            });
        }
        super::describe(
            &clear_btn,
            "Clear unpinned items; original files stay in place",
        );
        head.append(&heading);
        head.append(&clear_btn);
        root.append(&head);

        let scroll = gtk4::ScrolledWindow::new();
        scroll.set_vexpand(true);
        scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
        scroll.set_css_classes(&["na-scroll"]);

        let grid = FlowBox::new();
        grid.set_max_children_per_line(4);
        grid.set_min_children_per_line(3);
        grid.set_homogeneous(true);
        grid.set_selection_mode(gtk4::SelectionMode::None);
        grid.set_valign(gtk4::Align::Start);
        grid.set_column_spacing(10);
        grid.set_row_spacing(10);
        scroll.set_child(Some(&grid));
        root.append(&scroll);

        let empty = super::empty_state(g::INBOX, "Your temporary landing place", "Drop files, images or text onto the island. Drag them into another app whenever you need them.");
        root.append(&empty);
        let count = label(
            &["na-mute"],
            "Double-click or Enter to open · Right-click for more",
        );
        count.set_xalign(0.0);
        root.append(&count);

        let p = Self {
            root,
            grid,
            empty,
            scroll,
            clear_btn,
            count,
            rows: Default::default(),
        };
        p.reload();
        p
    }

    pub fn root(&self) -> &gtk4::Box {
        &self.root
    }

    pub fn reload(&self) {
        super::with_shared(|sh| {
            let items: Vec<ShelfItem> = sh.shelf.borrow().items().to_vec();
            self.empty.set_visible(items.is_empty());
            self.scroll.set_visible(!items.is_empty());
            self.clear_btn
                .set_sensitive(items.iter().any(|i| !i.pinned));
            self.count.set_text(&format!(
                "{} items · Double-click or Enter to open · Right-click for more",
                items.len()
            ));
            let mut rows = self.rows.borrow_mut();
            let present: std::collections::HashSet<_> =
                items.iter().map(|i| i.id.as_str()).collect();
            rows.retain(|id, (_, row)| {
                if present.contains(id.as_str()) {
                    true
                } else {
                    if row.parent().is_some() {
                        self.grid.remove(row);
                    }
                    false
                }
            });
            for (index, item) in items.into_iter().enumerate() {
                if rows
                    .get(&item.id)
                    .is_some_and(|(pin, _)| *pin != item.pinned)
                {
                    if let Some((_, row)) = rows.remove(&item.id) {
                        if row.parent().is_some() {
                            self.grid.remove(&row);
                        }
                    }
                }
                let (_, row) = rows
                    .entry(item.id.clone())
                    .or_insert_with(|| (item.pinned, tile(sh, item)));
                if self.grid.child_at_index(index as i32).as_ref() != Some(row) {
                    if row.parent().is_some() {
                        self.grid.remove(row);
                    }
                    self.grid.insert(row, index as i32);
                }
            }
        });
    }
}

fn icon_for(item: &ShelfItem) -> &'static str {
    match item.mime.as_str() {
        m if m.starts_with("image/") => g::IMAGE,
        m if m.starts_with("text/") => g::TEXT,
        _ if item.kind == "text" => g::TEXT,
        _ => g::FILE,
    }
}

fn tile(shared: &Rc<Shared>, item: ShelfItem) -> FlowBoxChild {
    let child = FlowBoxChild::new();
    child.set_focusable(true);
    child.set_cursor_from_name(Some("pointer"));
    super::describe(&child, &display_name(&item));

    let boxv = super::vbox(8);
    boxv.set_css_classes(&["na-shelf-tile"]);

    let thumb_holder = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    thumb_holder.set_css_classes(&["na-shelf-thumb"]);
    thumb_holder.set_size_request(96, 80);
    thumb_holder.set_halign(gtk4::Align::Center);
    thumb_holder.set_overflow(gtk4::Overflow::Hidden);
    if item.kind == "image" || item.mime.starts_with("image/") {
        let path = std::path::Path::new(&item.path);
        if !item.path.is_empty() {
            if let Ok(pixbuf) = gtk4::gdk_pixbuf::Pixbuf::from_file_at_scale(path, 192, 160, true) {
                let tex = gdk::Texture::for_pixbuf(&pixbuf);
                let pic = gtk4::Picture::for_paintable(&tex);
                pic.set_size_request(96, 80);
                pic.set_content_fit(gtk4::ContentFit::Cover);
                thumb_holder.append(&pic);
            } else {
                thumb_holder.append(&label(&["na-widget-glyph"], icon_for(&item)));
            }
        } else {
            thumb_holder.append(&label(&["na-widget-glyph"], icon_for(&item)));
        }
    } else {
        let ic = label(&["na-widget-glyph"], icon_for(&item));
        ic.set_valign(gtk4::Align::Center);
        ic.set_halign(gtk4::Align::Center);
        thumb_holder.append(&ic);
    }
    boxv.append(&thumb_holder);

    let name = label(&["na-shelf-name"], &display_name(&item));
    name.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
    name.set_max_width_chars(14);
    name.set_single_line_mode(true);
    name.set_halign(gtk4::Align::Center);
    boxv.append(&name);
    if item.pinned {
        let pin = label(&["na-feedback"], "Pinned");
        boxv.append(&pin);
    }
    child.set_child(Some(&boxv));

    let src = gtk4::DragSource::new();
    src.set_actions(gdk::DragAction::COPY);
    {
        let item2 = item.clone();
        src.connect_prepare(move |_ds, _x, _y| {
            if item2.kind == "text" {
                return Some(text_provider(&item2));
            }
            let file = gtk4::gio::File::for_path(&item2.path);
            let uri = format!("{}\r\n", file.uri());
            Some(gdk::ContentProvider::new_union(&[
                gdk::ContentProvider::for_value(&gdk::FileList::from_array(&[file]).to_value()),
                gdk::ContentProvider::for_bytes(
                    "text/uri-list",
                    &glib::Bytes::from(uri.as_bytes()),
                ),
                text_provider(&item2),
            ]))
        });
    }
    child.add_controller(src);

    {
        let item = item.clone();
        child.connect_activate(move |_| open_item(&item));
    }
    {
        let key = gtk4::EventControllerKey::new();
        let sh = shared.clone();
        let item = item.clone();
        let parent = child.downgrade();
        key.connect_key_pressed(move |_, key, _, mods| {
            if key == gdk::Key::Menu
                || (key == gdk::Key::F10 && mods.contains(gdk::ModifierType::SHIFT_MASK))
            {
                if let Some(parent) = parent.upgrade() {
                    show_menu(&sh, &item, &parent);
                }
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        child.add_controller(key);
    }

    let click = GestureClick::new();
    click.set_button(1);
    {
        let item3 = item.clone();
        click.connect_released(move |_g, n, _x, _y| {
            if n == 2 {
                open_item(&item3);
            }
        });
    }
    child.add_controller(click);

    let right = GestureClick::new();
    right.set_button(3);
    {
        let sh = shared.clone();
        let item3 = item.clone();
        let parent = child.downgrade();
        right.connect_released(move |_g, _n, _x, _y| {
            if let Some(parent) = parent.upgrade() {
                show_menu(&sh, &item3, &parent);
            }
        });
    }
    child.add_controller(right);
    child
}

fn display_name(item: &ShelfItem) -> String {
    if item.kind == "text" {
        let mut t: String = item.text.chars().take(24).collect();
        if item.text.chars().count() > 24 {
            t.push('…');
        }
        return t;
    }
    item.name.clone()
}

fn text_provider(item: &ShelfItem) -> gdk::ContentProvider {
    let text = match item.kind.as_str() {
        "text" => item.text.clone(),
        _ => item.path.clone(),
    };
    gdk::ContentProvider::for_bytes(
        "text/plain;charset=utf-8",
        &glib::Bytes::from(text.as_bytes()),
    )
}

fn open_item(item: &ShelfItem) {
    match item.kind.as_str() {
        "file" | "image" => crate::util::open_paths(std::slice::from_ref(&item.path)),
        "text" => {
            crate::ui::clipview::set_clipboard_text(item.text.clone());
        }
        _ => {}
    }
}

fn show_menu(shared: &Rc<Shared>, item: &ShelfItem, parent: &FlowBoxChild) {
    let (pop, menu) = super::context_menu();
    let mk = super::menu_button;

    let id = item.id.clone();

    if item.kind != "text" {
        let b_open = mk("Open");
        {
            let it = item.clone();
            let pop2 = pop.clone();
            b_open.connect_clicked(move |_| {
                open_item(&it);
                pop2.popdown();
            });
        }
        menu.append(&b_open);

        let b_rev = mk("Reveal in Files");
        {
            let it = item.clone();
            let pop2 = pop.clone();
            b_rev.connect_clicked(move |_| {
                crate::util::reveal_in_files(std::slice::from_ref(&it.path));
                pop2.popdown();
            });
        }
        menu.append(&b_rev);

        let b_cp = mk("Copy Path");
        {
            let it = item.clone();
            let pop2 = pop.clone();
            b_cp.connect_clicked(move |_| {
                crate::ui::clipview::set_clipboard_text(it.path.clone());
                pop2.popdown();
            });
        }
        menu.append(&b_cp);
    } else {
        let b_cp = mk("Copy Text");
        {
            let it = item.clone();
            let pop2 = pop.clone();
            b_cp.connect_clicked(move |_| {
                crate::ui::clipview::set_clipboard_text(it.text.clone());
                pop2.popdown();
            });
        }
        menu.append(&b_cp);
    }

    let pin_label = if item.pinned { "Unpin" } else { "Pin" };
    let b_pin = mk(pin_label);
    {
        let sh = shared.clone();
        let id2 = id.clone();
        let pop2 = pop.clone();
        b_pin.connect_clicked(move |_| {
            sh.shelf.borrow_mut().toggle_pin(&id2);
            pop2.popdown();
            crate::app::refresh_after_shelf_change();
        });
    }
    menu.append(&b_pin);

    let b_rm = mk("Remove");
    {
        let sh = shared.clone();
        let id2 = id.clone();
        let pop2 = pop.clone();
        b_rm.connect_clicked(move |_| {
            sh.shelf.borrow_mut().remove(&id2);
            pop2.popdown();
            crate::app::refresh_after_shelf_change();
        });
    }
    menu.append(&b_rm);

    pop.set_child(Some(&menu));
    pop.set_parent(parent);
    pop.popup();
}
