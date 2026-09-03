use super::{g, label, Shared};
use crate::shelf_store::ShelfItem;
use gtk4::prelude::*;
use gtk4::{gdk, glib, Button, FlowBox, FlowBoxChild, GestureClick};
use std::rc::Rc;

pub struct ShelfPage {
    root: gtk4::Box,
    grid: FlowBox,
    empty: gtk4::Box,
}

impl ShelfPage {
    pub fn build(shared: &Rc<Shared>) -> Self {
        let root = super::vbox(8);
        root.set_css_classes(&["na-panel-pad"]);

        let head = super::hbox(8);
        let spacer = label(&[""], "");
        spacer.set_hexpand(true);
        let clear_btn = Button::with_label("Clear");
        clear_btn.set_css_classes(&["na-btn", "ghost"]);
        {
            let sh = shared.clone();
            clear_btn.connect_clicked(move |_| {
                sh.shelf.borrow_mut().clear();
                crate::app::refresh_after_shelf_change();
            });
        }
        head.append(&spacer);
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

        let empty = super::vbox(8);
        empty.set_css_classes(&["na-drop-hint"]);
        empty.set_vexpand(true);
        empty.set_valign(gtk4::Align::Center);
        empty.set_halign(gtk4::Align::Fill);
        let ic = label(&["na-widget-glyph", "na-dim"], g::INBOX);
        ic.set_halign(gtk4::Align::Center);
        let hint = label(&["na-empty"], "Drop files here.\nDrag them out anywhere.");
        hint.set_justify(gtk4::Justification::Center);
        hint.set_halign(gtk4::Align::Center);
        empty.append(&ic);
        empty.append(&hint);
        root.append(&empty);

        let p = Self { root, grid, empty };
        p.reload();
        p
    }

    pub fn root(&self) -> &gtk4::Box {
        &self.root
    }

    pub fn reload(&self) {
        while let Some(c) = self.grid.first_child() {
            self.grid.remove(&c);
        }
        super::with_shared(|sh| {
            let items: Vec<ShelfItem> = sh.shelf.borrow().items().to_vec();
            self.empty.set_visible(items.is_empty());
            self.grid.set_visible(!items.is_empty());
            for item in items {
                let child = tile(sh, item);
                self.grid.append(&child);
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
    child.set_focusable(false);

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
            if let Ok(tex) = gdk::Texture::from_filename(path) {
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
    child.set_child(Some(&boxv));

    let src = gtk4::DragSource::new();
    src.set_actions(gdk::DragAction::COPY);
    {
        let item2 = item.clone();
        src.connect_prepare(move |_ds, _x, _y| {
            let content = gdk::ContentProvider::new_union(&[
                gdk::ContentProvider::for_value(&glib_uri_value(&item2)),
                text_provider(&item2),
            ]);
            Some(content)
        });
    }
    child.add_controller(src);

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
        let parent = child.clone();
        right.connect_released(move |_g, _n, _x, _y| {
            show_menu(&sh, &item3, &parent);
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

fn glib_uri_value(item: &ShelfItem) -> glib::Value {
    let uris = match item.kind.as_str() {
        "file" => vec![format!("file://{}", urlencode(&item.path))],
        "image" => vec![format!("file://{}", urlencode(&item.path))],
        _ => vec![],
    };
    let joined = uris.join("\r\n");
    String::to_value(&joined)
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

fn urlencode(p: &str) -> String {
    let mut out = String::with_capacity(p.len());
    for b in p.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            o => out.push_str(&format!("%{o:02X}")),
        }
    }
    out
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
    let pop = gtk4::Popover::new();
    pop.add_css_class("na-pop");
    let menu = super::vbox(4);
    menu.set_margin_top(8);
    menu.set_margin_bottom(8);
    menu.set_margin_start(10);
    menu.set_margin_end(10);

    let mk = |txt: &str| -> Button {
        let b = Button::with_label(txt);
        b.set_has_frame(false);
        b.set_halign(gtk4::Align::Fill);
        b
    };

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
            crate::app::refresh_after_shelf_change();
            pop2.popdown();
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
            crate::app::refresh_after_shelf_change();
            pop2.popdown();
        });
    }
    menu.append(&b_rm);

    pop.set_child(Some(&menu));
    pop.set_parent(parent);
    pop.popup();
}
