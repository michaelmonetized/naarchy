//! Widget drawer: a grid of widgets. Click to pin one on Home; drag works too.

use super::home::drag_content;
use super::{g, label, vbox, Shared};
use crate::widget_store::WidgetKind;
use gtk4::prelude::*;
use gtk4::{DragSource, FlowBox, GestureClick};
use std::rc::Rc;

pub struct DrawerPage {
    root: gtk4::Box,
    items: Vec<(WidgetKind, gtk4::Box)>,
}

impl DrawerPage {
    pub fn build(shared: &Rc<Shared>) -> Self {
        let root = vbox(10);
        root.set_css_classes(&["na-panel-pad"]);

        let hint = label(&["na-empty"], "Tap to pin on Home. Drag if you prefer.");
        hint.set_halign(gtk4::Align::Center);
        root.append(&hint);

        let grid = FlowBox::new();
        grid.set_max_children_per_line(4);
        grid.set_min_children_per_line(2);
        grid.set_homogeneous(true);
        grid.set_selection_mode(gtk4::SelectionMode::None);
        grid.set_valign(gtk4::Align::Start);
        grid.set_vexpand(true);
        grid.set_column_spacing(10);
        grid.set_row_spacing(10);

        let store = shared.widgets.borrow().clone();
        let mut items = Vec::new();
        for kind in WidgetKind::all() {
            let item = vbox(8);
            item.set_css_classes(&["na-widget-item"]);
            let icon = label(&["na-widget-glyph"], kind.glyph());
            icon.set_halign(gtk4::Align::Center);
            let name = label(&["na-dim"], kind.name());
            name.set_halign(gtk4::Align::Center);
            let mark = label(&["na-mute"], if store.has(kind) { g::CHECK } else { "" });
            mark.set_halign(gtk4::Align::Center);
            mark.set_css_classes(&["na-glyph", "na-mute"]);
            item.append(&icon);
            item.append(&name);
            item.append(&mark);
            if store.has(kind) {
                item.add_css_class("na-on");
            }
            item.set_halign(gtk4::Align::Fill);
            item.set_valign(gtk4::Align::Center);

            let kind2 = kind;
            let source = DragSource::new();
            source.set_actions(gtk4::gdk::DragAction::COPY);
            source.connect_prepare(move |_s, _x, _y| Some(drag_content(kind2)));
            item.add_controller(source);

            let click = GestureClick::new();
            {
                let sh = shared.clone();
                click.connect_released(move |_g, _n, _x, _y| {
                    sh.widgets.borrow_mut().toggle(kind2);
                    crate::app::refresh_home();
                });
            }
            item.add_controller(click);

            grid.append(&item);
            items.push((kind, item));
        }
        root.append(&grid);

        Self { root, items }
    }

    pub fn root(&self) -> &gtk4::Box {
        &self.root
    }

    pub fn rebuild(&self) {
        let on: Vec<WidgetKind> =
            super::with_shared(|sh| sh.widgets.borrow().widgets.clone()).unwrap_or_default();
        for (kind, item) in &self.items {
            let active = on.contains(kind);
            if active {
                item.add_css_class("na-on");
            } else {
                item.remove_css_class("na-on");
            }
            if let Some(mark) = item.last_child() {
                if let Ok(l) = mark.downcast::<gtk4::Label>() {
                    l.set_text(if active { g::CHECK } else { "" });
                }
            }
        }
    }
}
