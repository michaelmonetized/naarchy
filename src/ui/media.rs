use super::{g, glyph_btn, hbox, label, Shared};
use gtk4::prelude::*;
use gtk4::{gdk, Button, Label, Picture};

use std::rc::Rc;

pub struct MediaPage {
    root: gtk4::Box,
    player_row: gtk4::Box,
    launch_row: gtk4::Box,
    art_holder: gtk4::Box,
    title: Label,
    artist: Label,
    play_btn: Button,
    prev_btn: Button,
    next_btn: Button,
    last_art: std::cell::RefCell<Option<String>>,
}

impl MediaPage {
    pub fn build(shared: &Rc<Shared>) -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.set_css_classes(&["na-media-card"]);
        root.set_valign(gtk4::Align::Center);
        root.set_hexpand(true);
        root.set_margin_top(8);
        root.set_margin_bottom(8);
        root.set_margin_start(8);
        root.set_margin_end(8);

        let player_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        player_row.set_valign(gtk4::Align::Center);
        player_row.set_hexpand(true);

        let art_holder = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        art_holder.set_css_classes(&["na-media-art", "na-media-art--small"]);
        art_holder.set_size_request(64, 64);
        art_holder.set_halign(gtk4::Align::Start);
        art_holder.set_valign(gtk4::Align::Start);
        art_holder.set_overflow(gtk4::Overflow::Hidden);
        art_holder.set_hexpand(false);

        let col = super::vbox(4);
        col.set_hexpand(true);
        col.set_halign(gtk4::Align::Fill);
        col.set_valign(gtk4::Align::Center);

        let title = label(&["na-media-title", "na-media-title--compact"], "");
        title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        title.set_xalign(0.0);
        title.set_hexpand(true);
        title.set_halign(gtk4::Align::Fill);
        title.set_single_line_mode(true);
        title.set_max_width_chars(24);
        let artist = label(&["na-media-artist"], "");
        artist.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        artist.set_xalign(0.0);
        artist.set_hexpand(true);
        artist.set_halign(gtk4::Align::Fill);
        artist.set_wrap(true);
        artist.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
        artist.set_lines(2);
        artist.set_max_width_chars(24);

        let controls = hbox(8);
        controls.set_valign(gtk4::Align::Center);
        controls.set_halign(gtk4::Align::Start);
        let play_btn = glyph_btn(&["na-btn", "play", "na-media-play"], g::PLAY);
        play_btn.set_tooltip_text(Some("Play/Pause"));
        play_btn.set_size_request(36, 36);
        let next_btn = glyph_btn(&["na-btn", "na-media-btn", "ghost"], g::NEXT);
        next_btn.set_tooltip_text(Some("Next"));
        next_btn.set_size_request(32, 32);
        let prev_btn = glyph_btn(&["na-btn", "na-media-btn", "ghost"], g::PREV);
        super::describe(&prev_btn, "Previous track");
        prev_btn.set_size_request(32, 32);
        super::describe(&next_btn, "Next track");
        controls.append(&prev_btn);
        controls.append(&play_btn);
        controls.append(&next_btn);

        col.append(&title);
        col.append(&artist);
        col.append(&controls);

        player_row.append(&art_holder);
        player_row.append(&col);

        let launch_row = super::vbox(8);
        launch_row.set_css_classes(&["na-media-launchers"]);
        launch_row.set_valign(gtk4::Align::Center);
        launch_row.set_halign(gtk4::Align::Start);
        launch_row.set_hexpand(true);
        let spotify_btn = launch_btn("Spotify", "Open Spotify");
        let cliamp_btn = launch_btn("cliamp", "Open cliamp");
        spotify_btn.connect_clicked(|_| crate::util::launch_spotify());
        cliamp_btn.connect_clicked(|_| crate::util::launch_cliamp());
        let empty_title = label(&["na-empty-title"], "Set the mood");
        empty_title.set_xalign(0.0);
        let empty_detail = label(
            &["na-dim"],
            "Play something in your favorite app.\nYour music controls will appear here.",
        );
        empty_detail.set_xalign(0.0);
        empty_detail.set_wrap(true);
        launch_row.append(&empty_title);
        launch_row.append(&empty_detail);
        let launch_buttons = hbox(8);
        launch_buttons.append(&spotify_btn);
        launch_buttons.append(&cliamp_btn);
        launch_row.append(&launch_buttons);

        root.append(&player_row);
        root.append(&launch_row);

        let page = Self {
            root,
            player_row,
            launch_row,
            art_holder,
            title,
            artist,
            play_btn: play_btn.clone(),
            next_btn: next_btn.clone(),
            prev_btn: prev_btn.clone(),
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
            prev_btn.connect_clicked(move |_| send(&sh, crate::services::mpris::MediaCmd::Prev));
        }
        page.update();
        page
    }

    pub fn root(&self) -> &gtk4::Box {
        &self.root
    }

    fn set_play_glyph(&self, playing: bool) {
        if let Some(label) = self
            .play_btn
            .child()
            .and_then(|c| c.downcast::<Label>().ok())
        {
            super::set_label_text(&label, if playing { g::PAUSE } else { g::PLAY });
        }
        super::describe(&self.play_btn, if playing { "Pause" } else { "Play" });
    }

    pub fn update(&self) {
        super::with_shared(|sh| {
            let st = sh.media.borrow().clone();
            if let Some(st) = st.as_ref().filter(|s| s.is_live()) {
                self.player_row.set_visible(true);
                self.launch_row.set_visible(false);
                super::set_label_text(
                    &self.title,
                    if st.title.is_empty() {
                        "Untitled"
                    } else {
                        &st.title
                    },
                );
                super::set_label_text(&self.artist, &artist_line(&st.artist, &st.album));
                self.set_play_glyph(st.playing);
                self.play_btn.set_sensitive(st.can_play);
                self.prev_btn.set_sensitive(st.can_prev);
                self.next_btn.set_sensitive(st.can_next);

                if let Some(path) = st.art_path.clone() {
                    if self.last_art.borrow().as_deref() != Some(path.as_str()) {
                        *self.last_art.borrow_mut() = Some(path.clone());
                        while let Some(c) = self.art_holder.first_child() {
                            self.art_holder.remove(&c);
                        }
                        if let Ok(pixbuf) =
                            gtk4::gdk_pixbuf::Pixbuf::from_file_at_scale(&path, 128, 128, true)
                        {
                            let tex = gdk::Texture::for_pixbuf(&pixbuf);
                            let pic = Picture::for_paintable(&tex);
                            pic.set_size_request(64, 64);
                            pic.set_content_fit(gtk4::ContentFit::Cover);
                            self.art_holder.append(&pic);
                        } else {
                            self.art_holder
                                .append(&label(&["na-glyph", "lg", "na-dim"], g::MUSIC));
                        }
                    }
                } else if self.last_art.borrow().is_some() {
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
                self.player_row.set_visible(false);
                self.launch_row.set_visible(true);
                if self.last_art.borrow_mut().take().is_some() {
                    while let Some(child) = self.art_holder.first_child() {
                        self.art_holder.remove(&child);
                    }
                }
            }
        });
    }

    #[allow(dead_code)]
    pub fn tick(&self) {
        // no seek bar in compact design — nothing to tick
    }
}

fn artist_line(artist: &str, album: &str) -> String {
    if artist.is_empty() {
        album.to_string()
    } else {
        artist.to_string()
    }
}

fn launch_btn(name: &str, tooltip: &str) -> Button {
    let btn = Button::with_label(name);
    btn.set_has_frame(false);
    btn.set_css_classes(&["na-btn", "ghost"]);
    super::describe(&btn, tooltip);
    btn.set_cursor_from_name(Some("pointer"));
    btn
}

#[cfg(test)]
mod tests {
    use super::artist_line;

    #[test]
    fn artist_prefers_artist_over_album() {
        assert_eq!(
            artist_line("Organ Freeman", "Organ Freeman"),
            "Organ Freeman"
        );
        assert_eq!(artist_line("The Verve", "Urban Hymns"), "The Verve");
        assert_eq!(artist_line("", "Urban Hymns"), "Urban Hymns");
        assert_eq!(artist_line("The Verve", ""), "The Verve");
    }
}
