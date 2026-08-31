use super::{g, glyph_btn, hbox, label, Shared};
use gtk4::prelude::*;
use gtk4::{gdk, Button, Label, Picture, Scale};

use std::cell::Cell;
use std::rc::Rc;

pub struct MediaPage {
    root: gtk4::Box,
    art_holder: gtk4::Box,
    title: Label,
    artist: Label,
    player_lbl: Label,
    play_btn: Button,
    prev_btn: Button,
    next_btn: Button,
    shuffle_btn: Button,
    repeat_btn: Button,
    seek: Scale,
    time_cur: Label,
    time_len: Label,
    dragging: Cell<bool>,
    last_art: std::cell::RefCell<Option<String>>,
}

impl MediaPage {
    pub fn build(shared: &Rc<Shared>) -> Self {
        let root = super::vbox(10);
        root.set_valign(gtk4::Align::Center);
        root.set_vexpand(true);

        let top = hbox(14);
        let art_holder = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        art_holder.set_css_classes(&["na-media-art"]);
        art_holder.set_size_request(88, 88);
        art_holder.set_halign(gtk4::Align::Start);
        art_holder.set_valign(gtk4::Align::Center);
        art_holder.set_overflow(gtk4::Overflow::Hidden);

        let meta = super::vbox(2);
        let player_lbl = label(&["na-mute"], "Nothing playing");
        let title = label(&["na-media-title"], "");
        title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        title.set_xalign(0.0);
        let artist = label(&["na-media-artist"], "");
        artist.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        artist.set_xalign(0.0);
        meta.append(&player_lbl);
        meta.append(&title);
        meta.append(&artist);
        meta.set_valign(gtk4::Align::Center);
        meta.set_hexpand(true);

        top.append(&art_holder);
        top.append(&meta);
        root.append(&top);

        let seek_row = super::vbox(0);
        let seek = Scale::with_range(gtk4::Orientation::Horizontal, 0.0, 100.0, 1.0);
        seek.set_css_classes(&["na-slider"]);
        seek.set_draw_value(false);
        seek.set_hexpand(true);
        let times = hbox(6);
        let time_cur = label(&["na-mute"], "0:00");
        let spacer = label(&["na-mute"], "");
        spacer.set_hexpand(true);
        let time_len = label(&["na-mute"], "0:00");
        times.append(&time_cur);
        times.append(&spacer);
        times.append(&time_len);
        seek_row.append(&seek);
        seek_row.append(&times);
        root.append(&seek_row);

        let page = Self {
            root,
            art_holder,
            title,
            artist,
            player_lbl,
            play_btn: glyph_btn(&["na-btn", "play"], g::PLAY),
            prev_btn: glyph_btn(&["na-btn"], g::PREV),
            next_btn: glyph_btn(&["na-btn"], g::NEXT),
            shuffle_btn: glyph_btn(&["na-btn"], g::SHUFFLE),
            repeat_btn: glyph_btn(&["na-btn"], g::REPEAT),
            seek,
            time_cur,
            time_len,
            dragging: Cell::new(false),
            last_art: std::cell::RefCell::new(None),
        };

        let controls = hbox(8);
        controls.set_halign(gtk4::Align::Center);
        controls.append(&page.shuffle_btn);
        controls.append(&page.prev_btn);
        controls.append(&page.play_btn);
        controls.append(&page.next_btn);
        controls.append(&page.repeat_btn);
        page.root.append(&controls);

        let send = |sh: &Rc<Shared>, cmd: crate::services::mpris::MediaCmd| {
            if let Some(tx) = sh.media_cmd.borrow().as_ref() {
                let _ = tx.send(cmd);
            }
        };
        {
            let sh = shared.clone();
            page.play_btn
                .connect_clicked(move |_| send(&sh, crate::services::mpris::MediaCmd::PlayPause));
        }
        {
            let sh = shared.clone();
            page.next_btn
                .connect_clicked(move |_| send(&sh, crate::services::mpris::MediaCmd::Next));
        }
        {
            let sh = shared.clone();
            page.prev_btn
                .connect_clicked(move |_| send(&sh, crate::services::mpris::MediaCmd::Prev));
        }
        {
            let sh = shared.clone();
            page.shuffle_btn.connect_clicked(move |b| {
                let on = !b.has_css_class("active");
                if on {
                    b.add_css_class("active");
                } else {
                    b.remove_css_class("active");
                }
                send(&sh, crate::services::mpris::MediaCmd::SetShuffle(on));
            });
        }
        {
            let sh = shared.clone();
            page.repeat_btn.connect_clicked(move |b| {
                let next = match b
                    .css_classes()
                    .iter()
                    .find(|c| c.as_str().starts_with("rep-"))
                {
                    Some(c) => match c.as_str() {
                        "rep-off" => 1u8,
                        "rep-track" => 2,
                        _ => 0,
                    },
                    None => 1,
                };
                for c in ["rep-off", "rep-track", "rep-all"] {
                    b.remove_css_class(c);
                }
                b.add_css_class(match next {
                    1 => "rep-track",
                    2 => "rep-all",
                    _ => "rep-off",
                });
                send(&sh, crate::services::mpris::MediaCmd::SetLoop(next));
            });
        }

        {
            let drag = page.dragging.clone();
            let press = gtk4::GestureClick::new();
            press.connect_pressed(move |_g, _n, _x, _y| drag.set(true));
            page.seek.add_controller(press);
        }
        {
            let sh = shared.clone();
            let drag = page.dragging.clone();
            let scale = page.seek.clone();
            let release = gtk4::GestureClick::new();
            release.connect_released(move |_g, _n, _x, _y| {
                if drag.get() {
                    let secs = scale.value();
                    send(
                        &sh,
                        crate::services::mpris::MediaCmd::SeekAbs((secs * 1_000_000.0) as i64),
                    );
                    drag.set(false);
                }
            });
            page.seek.add_controller(release);
        }

        page.update();
        page
    }

    pub fn root(&self) -> &gtk4::Box {
        &self.root
    }

    fn set_play_glyph(&self, playing: bool) {
        self.play_btn.set_child(Some(&super::label(
            &["na-glyph"],
            if playing { g::PAUSE } else { g::PLAY },
        )));
    }

    pub fn update(&self) {
        super::with_shared(|sh| {
            let st_opt = sh.media.borrow();
            if let Some(st) = st_opt.as_ref() {
                self.player_lbl.set_text(&st.player.to_uppercase());
                self.title.set_text(if st.title.is_empty() {
                    "Untitled"
                } else {
                    &st.title
                });
                self.artist.set_text(&format!(
                    "{}{}",
                    st.artist,
                    if st.album.is_empty() {
                        String::new()
                    } else {
                        format!(" — {}", st.album)
                    }
                ));
                self.set_play_glyph(st.playing);
                self.play_btn.set_sensitive(true);
                self.next_btn.set_sensitive(st.can_next);
                self.prev_btn.set_sensitive(st.can_prev);
                self.seek.set_sensitive(st.can_seek && st.length_us > 0);
                if st.shuffle {
                    self.shuffle_btn.add_css_class("active");
                } else {
                    self.shuffle_btn.remove_css_class("active");
                }
                self.repeat_btn.remove_css_class("rep-off");
                self.repeat_btn.remove_css_class("rep-track");
                self.repeat_btn.remove_css_class("rep-all");
                self.repeat_btn.add_css_class(match st.repeat {
                    1 => "rep-track",
                    2 => "rep-all",
                    _ => "rep-off",
                });

                let len_s = (st.length_us / 1_000_000).max(0) as f64;
                let pos_s = (st.position_us / 1_000_000).max(0) as f64;
                if len_s > 0.0 {
                    self.seek.set_range(0.0, len_s);
                    if !self.dragging.get() {
                        self.seek.set_value(pos_s.min(len_s));
                    }
                }
                self.time_len.set_text(&fmt_time(len_s as u64));
                self.time_cur.set_text(&fmt_time(pos_s as u64));

                if let Some(path) = st.art_path.clone() {
                    if self.last_art.borrow().as_deref() != Some(path.as_str()) {
                        *self.last_art.borrow_mut() = Some(path.clone());
                        while let Some(c) = self.art_holder.first_child() {
                            self.art_holder.remove(&c);
                        }
                        if let Ok(tex) = gdk::Texture::from_filename(std::path::Path::new(&path)) {
                            let pic = Picture::for_paintable(&tex);
                            pic.set_size_request(88, 88);
                            pic.set_content_fit(gtk4::ContentFit::Cover);
                            self.art_holder.append(&pic);
                        }
                    }
                } else if self.last_art.borrow().is_some()
                    || self.art_holder.first_child().is_some()
                {
                    *self.last_art.borrow_mut() = None;
                    while let Some(c) = self.art_holder.first_child() {
                        self.art_holder.remove(&c);
                    }
                    self.art_holder
                        .append(&label(&["na-glyph", "lg", "na-dim"], g::MUSIC));
                }
            } else {
                self.player_lbl.set_text("NOTHING PLAYING");
                self.title.set_text("");
                self.artist.set_text("");
                self.set_play_glyph(false);
                self.play_btn.set_sensitive(false);
                self.next_btn.set_sensitive(false);
                self.prev_btn.set_sensitive(false);
                self.seek.set_sensitive(false);
                self.seek.set_range(0.0, 100.0);
                self.seek.set_value(0.0);
                *self.last_art.borrow_mut() = None;
                while let Some(c) = self.art_holder.first_child() {
                    self.art_holder.remove(&c);
                }
                self.art_holder
                    .append(&label(&["na-glyph", "lg", "na-dim"], g::MUSIC));
            }
        });
    }

    #[allow(dead_code)]
    pub fn tick(&self) {
        super::with_shared(|sh| {
            let st = sh.media.borrow().clone();
            if let Some(st) = st {
                if !self.dragging.get() {
                    let pos_s = (st.position_us / 1_000_000).max(0) as u64;
                    self.time_cur.set_text(&fmt_time(pos_s));
                    if st.length_us > 0 {
                        self.seek
                            .set_value((pos_s as f64).min(self.seek.adjustment().upper()));
                    }
                }
            }
        });
    }
}

fn fmt_time(secs: u64) -> String {
    if secs >= 3600 {
        format!("{}:{:02}:{:02}", secs / 3600, (secs % 3600) / 60, secs % 60)
    } else {
        format!("{}:{:02}", secs / 60, secs % 60)
    }
}
