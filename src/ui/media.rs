use super::{g, glyph_btn, hbox, label, Shared};
use gtk4::prelude::*;
use gtk4::{gdk, Button, Label, Picture};

use std::rc::Rc;

pub struct MediaPage {
    root: gtk4::Box,
    art_holder: gtk4::Box,
    title: Label,
    artist: Label,
    player_lbl: Label,
    add_btn: Button,
    play_btn: Button,
    next_btn: Button,
    last_art: std::cell::RefCell<Option<String>>,
}

impl MediaPage {
    pub fn build(shared: &Rc<Shared>) -> Self {
        // compact dark card like the screenshot: 64px art, title/artist, +/pause/next
        let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        root.set_css_classes(&["na-media-card"]);
        root.set_valign(gtk4::Align::Center);
        root.set_hexpand(true);
        root.set_margin_top(8);
        root.set_margin_bottom(8);
        root.set_margin_start(8);
        root.set_margin_end(8);

        let art_holder = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        art_holder.set_css_classes(&["na-media-art", "na-media-art--small"]);
        art_holder.set_size_request(56, 56);
        art_holder.set_halign(gtk4::Align::Center);
        art_holder.set_valign(gtk4::Align::Center);
        art_holder.set_overflow(gtk4::Overflow::Hidden);

        let meta = super::vbox(2);
        meta.set_hexpand(true);
        meta.set_valign(gtk4::Align::Center);
        meta.set_halign(gtk4::Align::Fill);
        let title = label(&["na-media-title", "na-media-title--compact"], "");
        title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        title.set_xalign(0.0);
        title.set_max_width_chars(22);
        let artist = label(&["na-media-artist"], "");
        artist.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        artist.set_xalign(0.0);
        artist.set_max_width_chars(24);
        let player_lbl = label(&["na-mute", "na-media-player"], "");
        player_lbl.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        player_lbl.set_xalign(0.0);
        player_lbl.set_visible(false);
        meta.append(&title);
        meta.append(&artist);
        meta.append(&player_lbl);

        let controls = hbox(8);
        controls.set_valign(gtk4::Align::Center);
        controls.set_halign(gtk4::Align::End);
        let add_btn = glyph_btn(&["na-btn", "na-media-btn", "ghost"], g::PLUS);
        add_btn.set_tooltip_text(Some("Add to Inbox"));
        add_btn.set_size_request(32, 32);
        let play_btn = glyph_btn(&["na-btn", "play", "na-media-play"], g::PLAY);
        play_btn.set_tooltip_text(Some("Play/Pause"));
        play_btn.set_size_request(36, 36);
        let next_btn = glyph_btn(&["na-btn", "na-media-btn", "ghost"], g::NEXT);
        next_btn.set_tooltip_text(Some("Next"));
        next_btn.set_size_request(32, 32);
        controls.append(&add_btn);
        controls.append(&play_btn);
        controls.append(&next_btn);

        root.append(&art_holder);
        root.append(&meta);
        root.append(&controls);

        let page = Self {
            root,
            art_holder,
            title,
            artist,
            player_lbl,
            add_btn: add_btn.clone(),
            play_btn: play_btn.clone(),
            next_btn: next_btn.clone(),
            last_art: std::cell::RefCell::new(None),
        };

        let send = |sh: &Rc<Shared>, cmd: crate::services::mpris::MediaCmd| {
            if let Some(tx) = sh.media_cmd.borrow().as_ref() {
                let _ = tx.send(cmd);
            } else {
                log::debug!(
                    "media_cmd not ready for {:?}",
                    std::any::type_name_of_val(&cmd)
                );
            }
        };
        {
            let sh = shared.clone();
            play_btn.connect_clicked(move |_| {
                log::info!(
                    "media play_pause clicked, player={:?}",
                    sh.media.borrow().as_ref().map(|s| s.player.clone())
                );
                send(&sh, crate::services::mpris::MediaCmd::PlayPause)
            });
        }
        {
            let sh = shared.clone();
            next_btn.connect_clicked(move |_| send(&sh, crate::services::mpris::MediaCmd::Next));
        }
        {
            let sh = shared.clone();
            add_btn.connect_clicked(move |_| {
                // + adds current track to shelf as text: "Title — Artist (player)"
                let txt = sh
                    .media
                    .borrow()
                    .as_ref()
                    .map(|s| {
                        if s.title.is_empty() && s.artist.is_empty() {
                            s.player.clone()
                        } else {
                            format!(
                                "{} — {} {}",
                                s.title,
                                s.artist,
                                if s.album.is_empty() {
                                    String::new()
                                } else {
                                    format!("({})", s.album)
                                }
                            )
                        }
                    })
                    .unwrap_or_default();
                if !txt.is_empty() {
                    sh.shelf.borrow_mut().add_text(&txt);
                    crate::app::refresh_after_shelf_change();
                }
            });
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
            let st = sh.media.borrow().clone();
            if let Some(st) = st.as_ref() {
                // keep player label hidden when playing (image has no player), show only when debugging
                self.player_lbl.set_visible(false);
                self.title.set_text(if st.title.is_empty() {
                    "Untitled"
                } else {
                    &st.title
                });
                let artist_txt = if st.artist.is_empty() {
                    st.album.clone()
                } else if st.album.is_empty() {
                    st.artist.clone()
                } else {
                    format!("{} — {}", st.artist, st.album)
                };
                self.artist.set_text(&artist_txt);
                // truncate for compact view is handled by ellipsize + max_width_chars
                self.set_play_glyph(st.playing);
                self.play_btn.set_sensitive(true);
                self.next_btn.set_sensitive(st.can_next);
                self.add_btn.set_sensitive(true);

                if let Some(path) = st.art_path.clone() {
                    if self.last_art.borrow().as_deref() != Some(path.as_str()) {
                        *self.last_art.borrow_mut() = Some(path.clone());
                        while let Some(c) = self.art_holder.first_child() {
                            self.art_holder.remove(&c);
                        }
                        if let Ok(tex) = gdk::Texture::from_filename(std::path::Path::new(&path)) {
                            let pic = Picture::for_paintable(&tex);
                            pic.set_size_request(56, 56);
                            pic.set_content_fit(gtk4::ContentFit::Cover);
                            self.art_holder.append(&pic);
                        } else {
                            self.art_holder
                                .append(&label(&["na-glyph", "lg", "na-dim"], g::MUSIC));
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
                } else if self.art_holder.first_child().is_none() {
                    self.art_holder
                        .append(&label(&["na-glyph", "lg", "na-dim"], g::MUSIC));
                }
            } else {
                self.player_lbl.set_text("Nothing playing");
                self.player_lbl.set_visible(false);
                self.title.set_text("Nothing playing");
                self.artist
                    .set_text("Open any MPRIS player — mpv, Spotify, YouTube in Chromium, etc.");
                self.set_play_glyph(false);
                self.play_btn.set_sensitive(false);
                self.next_btn.set_sensitive(false);
                self.add_btn.set_sensitive(false);
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
        // no seek bar in compact design — nothing to tick
    }
}
