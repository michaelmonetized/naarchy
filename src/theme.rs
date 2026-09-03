use crate::config::Config;

/// Everything visual derived (once) from config + the active omarchy theme.
/// Cached values feed both the CSS and the cairo-drawn layers (pill capsule,
/// shelf gradient) so colors and font live in one place.
#[derive(Clone, Debug)]
pub struct Palette {
    // hex strings, e.g. "#89b4fa"
    pub accent: String,
    pub bg: String,
    pub bg2: String,
    pub fg: String,
    pub icon_font: String,
    // rgb components of accent for glow math
    pub accent_rgb: (u8, u8, u8),
}

const DEFAULT_ACCENT: &str = "#7aa2f7";
const DEFAULT_ICON_FONT: &str = "JetBrainsMono Nerd Font";

fn hex_or_base(
    cfg_opt: &Option<String>,
    omarchy: Option<String>,
    base_dark: &str,
    base_light: &str,
    dark: bool,
) -> String {
    if let Some(v) = cfg_opt {
        let v = v.trim();
        if v.starts_with('#') && v.len() >= 7 {
            return v.to_string();
        }
    }
    if let Some(v) = omarchy {
        return v;
    }
    if dark {
        base_dark.to_string()
    } else {
        base_light.to_string()
    }
}

/// Resolve colors for the current config. `dark` respected, but the omarchy
/// theme's own `mode` wins when the config theme is "auto".
pub fn resolve(cfg: &Config, dark: bool) -> Palette {
    let a = &cfg.appearance;
    let om = if a.omarchy {
        crate::omarchy::palette()
    } else {
        None
    };
    let theme_dark = match a.theme.as_str() {
        "dark" => true,
        "light" => false,
        _ => om.as_ref().map(|p| p.mode != "light").unwrap_or(dark),
    };

    let accent = a
        .accent
        .clone()
        .or_else(|| om.as_ref().map(|p| p.accent.clone()))
        .unwrap_or_else(|| DEFAULT_ACCENT.into());
    let bg = hex_or_base(
        &a.bg,
        om.as_ref().map(|p| p.background.clone()),
        "#0a0a0f",
        "#f4f4f8",
        theme_dark,
    );
    let bg2 = om
        .as_ref()
        .map(|p| p.dark_background.clone())
        .unwrap_or_else(|| if theme_dark { "#131318" } else { "#ffffff" }.into());
    let fg = hex_or_base(
        &a.fg,
        om.as_ref().map(|p| p.foreground.clone()),
        "#eef0f6",
        "#17171c",
        theme_dark,
    );
    let icon_font = a
        .icon_font
        .clone()
        .or_else(crate::omarchy::icon_font)
        .unwrap_or_else(|| DEFAULT_ICON_FONT.into());

    let accent_rgb = hex_triple(&accent).unwrap_or((0x7a, 0xa2, 0xf7));

    Palette {
        accent,
        bg,
        bg2,
        fg,
        icon_font,
        accent_rgb,
    }
}

/// Convenience for cairo callers that only need the accent.
#[allow(dead_code)]
pub fn accent_hex(cfg: &Config) -> String {
    let a = &cfg.appearance;
    if let Some(v) = &a.accent {
        return v.clone();
    }
    if a.omarchy {
        if let Some(p) = crate::omarchy::palette() {
            return p.accent;
        }
    }
    DEFAULT_ACCENT.into()
}

pub fn hex_triple(s: &str) -> Option<(u8, u8, u8)> {
    let h = s.trim_start_matches('#');
    if h.len() < 6 {
        return None;
    }
    Some((
        u8::from_str_radix(&h[0..2], 16).ok()?,
        u8::from_str_radix(&h[2..4], 16).ok()?,
        u8::from_str_radix(&h[4..6], 16).ok()?,
    ))
}

fn hex_rgb(s: &str) -> String {
    let (r, g, b) = hex_triple(s).unwrap_or((0x7a, 0xa2, 0xf7));
    format!("{r}, {g}, {b}")
}

/// Build the full application CSS from config. Recomputed on config reload
/// and dark/light scheme flips.
pub fn build_css(cfg: &Config, dark: bool) -> String {
    let a = &cfg.appearance;
    let pal = resolve(cfg, dark);

    let alpha = a.opacity.clamp(0.3, 1.0);
    let bg_rgb = hex_rgb(&pal.bg);
    let bg = format!("rgba({bg_rgb}, {alpha})");
    let bg2 = &pal.bg2;
    let fg = &pal.fg;
    let fg_rgb = hex_rgb(&pal.fg);
    let fg_dim = format!("rgba({fg_rgb}, 0.72)");
    let fg_mute = format!("rgba({fg_rgb}, 0.52)");
    let glass = format!("rgba({fg_rgb}, 0.06)");
    let glass_2 = format!("rgba({fg_rgb}, 0.09)");
    let glass_3 = format!("rgba({fg_rgb}, 0.14)");
    let border = format!("rgba({fg_rgb}, 0.10)");
    let hover = format!("rgba({fg_rgb}, 0.08)");
    let shadow = "0 12px 40px rgba(0,0,0,0.45), 0 2px 8px rgba(0,0,0,0.25)".to_string();
    let accent = &pal.accent;
    let accent_rgb = hex_rgb(&pal.accent);
    let icon_font = &pal.icon_font;
    let ease = "cubic-bezier(0.22, 1, 0.36, 1)";

    format!(
        r#"
/* Scoped to our windows so we don't bleach every GTK app on the display. */
window.naarchy {{
  background-color: transparent;
  background-image: none;
  color: {fg};
}}
window.naarchy box,
window.naarchy label,
window.naarchy button,
window.naarchy entry,
window.naarchy scrollbar,
window.naarchy revealer,
window.naarchy stack,
window.naarchy scrolledwindow,
window.naarchy viewport,
window.naarchy flowbox,
window.naarchy flowboxchild,
window.naarchy listbox,
window.naarchy listboxrow,
window.naarchy scale,
window.naarchy image,
window.naarchy picture {{
  background-color: transparent;
  background-image: none;
  border: none;
  box-shadow: none;
  outline: none;
}}

.na-pill {{
  background-color: transparent;
  color: #ffffff;
  padding: 0 10px;
}}
.na-bubble {{
  background-color: transparent;
  color: #ffffff;
  padding: 0 12px;
}}
.na-bubble-text {{
  color: #ffffff;
  font-size: 13px;
  font-weight: 700;
  letter-spacing: 0.15px;
  font-feature-settings: "tnum";
}}
.na-pill-count {{
  font-size: 13px;
  font-weight: 700;
  font-feature-settings: "tnum";
}}
.na-pill-live .na-bubble-text,
.na-pill-live .na-pill-count {{
  font-size: 20px;
}}
.na-pill-live .na-glyph {{
  font-size: 20px;
}}

.na-chip {{
  border-radius: 999px;
  padding: 4px 11px;
  background-color: rgba(255,255,255,0.10);
  color: #ffffff;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.3px;
}}
.na-chip.accent {{ background-color: {accent}; color: #0b0c10; font-weight: 700; }}

.na-dock {{
  padding: 6px 8px;
  border-radius: 999px;
  background-color: rgba(12, 12, 16, 0.62);
  border: 1px solid rgba(255,255,255,0.10);
  box-shadow: 0 10px 28px rgba(0,0,0,0.38), inset 0 1px 0 rgba(255,255,255,0.08);
}}
.na-dock-btn {{
  min-width: 32px;
  min-height: 32px;
  padding: 0;
  border-radius: 999px;
  color: {fg};
  transition: background-color 160ms {ease};
}}
.na-dock-btn:hover {{
  background-color: {glass_2};
}}
.na-dock-btn:checked {{
  background-color: rgba(255,255,255,0.16);
}}
.na-dock-btn:checked .na-dock-glyph {{ color: #ffffff; }}
.na-dock-glyph {{
  font-family: "{icon_font}";
  font-size: 16px;
  color: rgba(255,255,255,0.85);
  transition: color 180ms {ease};
}}

.na-shelf-drop {{
  border: 1.5px dashed {accent};
  border-radius: 28px;
  background-color: rgba({accent_rgb},0.07);
}}
window.naarchy box.na-drop-veil {{
  background-color: rgba({accent_rgb}, 0.16);
  border: 2px dashed {accent};
  border-radius: 28px;
  box-shadow: none;
}}
.na-drop-veil-label {{
  font-size: 15px;
  font-weight: 700;
  letter-spacing: 0.4px;
  color: {accent};
}}
.na-pile-shot {{
  border-radius: 8px;
  background-color: rgba(255,255,255,0.10);
  border: 1px solid rgba(255,255,255,0.22);
  box-shadow: 0 4px 10px rgba(0,0,0,0.45);
}}

.na-widget {{
  border-radius: 22px;
  padding: 16px 16px 14px 16px;
  background-color: {glass};
  border: 1px solid {border};
  box-shadow: inset 0 1px 0 rgba(255,255,255,0.05);
}}
.na-widget-item {{
  border-radius: 18px;
  padding: 16px 10px 12px 10px;
  background-color: {glass};
  border: 1px solid {border};
  transition: background-color 160ms {ease}, border-color 160ms {ease}, transform 160ms {ease};
}}
.na-widget-item:hover {{
  background-color: {glass_2};
}}
.na-widget-item.na-on {{
  background-color: rgba({accent_rgb},0.16);
  border-color: rgba({accent_rgb},0.45);
}}
.na-widget-item.na-on .na-dim {{ color: {fg}; }}
.na-widget-glyph {{
  font-family: "{icon_font}";
  font-size: 26px;
  color: {fg};
}}
.na-clock-big {{
  font-size: 36px;
  font-weight: 700;
  letter-spacing: -1.6px;
  font-feature-settings: "tnum";
}}

.na-tabbar {{
  padding: 4px;
  border-radius: 999px;
  background-color: rgba(0,0,0,0.5);
}}
.na-tab {{
  padding: 5px 14px;
  border-radius: 999px;
  font-weight: 600;
  font-size: 12px;
  color: {fg_dim};
  transition: background-color 160ms {ease}, color 160ms {ease};
}}
.na-tab:hover {{ background-color: {hover}; color: {fg}; }}
.na-tab.active {{ background-color: {accent}; color: #0d0e12; }}

.na-panel-pad {{ padding: 8px 6px 4px 6px; }}

.na-title {{
  font-size: 13px;
  font-weight: 600;
  letter-spacing: 0.4px;
  color: {fg_dim};
}}
.na-dim {{ color: {fg_dim}; font-size: 12px; }}
.na-mute {{ color: {fg_mute}; font-size: 11px; }}

.na-media-art {{
  border-radius: 16px;
  background-color: {glass_2};
  box-shadow: 0 8px 24px rgba(0,0,0,0.35);
}}
.na-media-art--small {{
  border-radius: 8px;
  background-color: #2a2a2c;
  box-shadow: none;
}}
.na-media-art--chip {{
  border-radius: 8px;
  background-color: #2a2a2c;
  box-shadow: none;
  min-width: 32px;
  min-height: 32px;
  max-width: 32px;
  max-height: 32px;
}}
.na-media-card {{
  border-radius: 12px;
  padding: 4px 0;
  background-color: transparent;
  border: none;
  box-shadow: none;
}}
.na-media-title {{
  font-size: 18px;
  font-weight: 700;
  letter-spacing: -0.4px;
}}
.na-media-title--compact {{
  font-size: 13px;
  font-weight: 700;
  letter-spacing: -0.2px;
  color: #ffffff;
}}
.na-media-artist {{ font-size: 13px; color: {fg_dim}; }}
.na-media-player {{
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.5px;
  color: rgba(255,255,255,0.45);
}}
.na-media-btn {{
  min-width: 32px;
  min-height: 32px;
  padding: 0;
  border-radius: 999px;
  background-color: rgba(255,255,255,0.10);
  color: #ffffff;
}}
.na-media-btn:hover {{ background-color: rgba(255,255,255,0.16); }}
.na-media-play {{
  background-color: #ffffff;
  color: #1a1a1c;
}}
.na-btn {{
  border-radius: 999px;
  min-width: 36px;
  min-height: 36px;
  padding: 0 12px;
  font-size: 14px;
  transition: background-color 160ms {ease}, color 160ms {ease};
}}
.na-btn:hover {{ background-color: {glass_2}; }}
.na-media-launchers {{
  min-height: 56px;
}}
.na-btn.na-media-launch {{
  min-width: 56px;
  min-height: 56px;
  padding: 0;
  border-radius: 14px;
  background-color: transparent;
}}
.na-btn.na-media-launch:hover {{
  background-color: rgba(255,255,255,0.10);
}}
.na-btn.play {{
  background-color: rgba(255,255,255,0.92);
  color: #111118;
  min-width: 44px;
  min-height: 44px;
  font-weight: 700;
}}
.na-btn.play:hover {{ background-color: #ffffff; }}
.na-btn.ghost {{
  background-color: {glass};
  color: {fg};
}}
.na-btn.active, .na-btn.rep-track, .na-btn.rep-all {{
  color: {accent};
  background-color: rgba({accent_rgb},0.16);
}}
.na-shelf-tile {{
  border-radius: 18px;
  padding: 10px 8px 8px 8px;
  background-color: {glass};
  border: 1px solid {border};
  transition: background-color 160ms {ease}, border-color 160ms {ease}, transform 160ms {ease};
}}
.na-shelf-tile:hover {{
  background-color: {glass_2};
  border-color: rgba({accent_rgb},0.45);
}}
.na-shelf-thumb {{
  border-radius: 12px;
  background-color: {glass_2};
}}
.na-shelf-name {{ font-size: 11px; font-weight: 600; }}
.na-drop-hint {{
  border-radius: 28px;
  border: 1.5px dashed {border};
  padding: 36px 20px;
}}

.na-clip-row {{
  border-radius: 14px;
  padding: 10px 12px;
  margin: 2px 0;
  background-color: {glass};
  border: 1px solid {border};
  transition: background-color 140ms {ease}, border-color 140ms {ease}, transform 140ms {ease};
}}
.na-clip-row:hover {{ background-color: {glass_2}; border-color: rgba({accent_rgb}, 0.22); }}
.na-clip-preview {{
  font-size: 13px;
  font-weight: 500;
  color: {fg};
}}
.na-pin {{ color: {accent}; font-weight: 800; }}
.na-kind {{
  font-family: "{icon_font}";
  font-size: 14px;
  color: {fg_dim};
  min-width: 18px;
}}
.na-clip-time {{
  font-size: 11px;
  font-weight: 600;
  color: {fg_dim};
  font-feature-settings: "tnum";
}}
.na-clip-list {{
  background-color: transparent;
}}

.na-cal-day {{
  min-width: 34px;
  min-height: 34px;
  border-radius: 50%;
  font-size: 12px;
  font-weight: 600;
  color: {fg};
  transition: background-color 140ms {ease}, color 140ms {ease};
}}
.na-cal-day:hover {{ background-color: {glass_2}; }}
.na-cal-day.other {{ color: {fg_mute}; }}
.na-cal-day.today {{
  background-color: {accent};
  color: #0d0e12;
  font-weight: 800;
}}
.na-cal-wd {{
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.6px;
  color: {fg_mute};
}}
.na-cal-head {{
  font-weight: 800;
  font-size: 18px;
  letter-spacing: 1.4px;
  color: {accent};
}}
.na-cal-row {{
  border-radius: 12px;
  padding: 8px 10px;
  background-color: {glass};
  border-left: 3px solid {accent};
}}
.na-cal-time {{
  font-feature-settings: "tnum";
  font-weight: 700;
  font-size: 12px;
}}
.na-cal-next {{
  font-weight: 700;
  font-size: 15px;
}}
.na-tier1 {{ color: {accent}; }}

.na-entry {{
  border-radius: 12px;
  padding: 8px 12px;
  background-color: {glass};
  color: {fg};
  font-size: 13px;
  caret-color: {accent};
  transition: border-color 160ms {ease}, background-color 160ms {ease};
  border: 1px solid {border};
}}
.na-entry:focus {{
  border-color: rgba({accent_rgb},0.7);
  background-color: {glass_2};
}}

.na-preset {{
  border-radius: 999px;
  min-width: 48px;
  min-height: 28px;
  padding: 0 12px;
  font-size: 12px;
  font-weight: 600;
  background-color: {glass};
  color: {fg};
  border: 1px solid transparent;
  transition: background-color 160ms {ease}, transform 160ms {ease}, border-color 160ms {ease};
}}
.na-preset:hover {{ background-color: {glass_3}; border-color: rgba({accent_rgb},0.35); }}

.na-timer-start {{
  border-radius: 999px;
  min-height: 32px;
  padding: 6px 14px;
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0.2px;
  background-color: {accent};
  color: #0d0e12;
}}
.na-timer-start:hover {{
  background-color: #ffffff;
}}
.na-timer-hms {{
  font-size: 26px;
  font-weight: 700;
  letter-spacing: -1.2px;
  font-feature-settings: "tnum";
  color: {accent};
}}
.na-timer-hms.na-timer-done {{
  animation: na-bell-pulse 900ms {ease} infinite alternate;
}}
@keyframes na-bell-pulse {{
  0% {{ opacity: 0.92; }}
  100% {{ opacity: 1.0; }}
}}

.na-settings-btn {{
  opacity: 0.88;
}}
.na-settings-btn:hover {{
  opacity: 1.0;
  background-color: {glass_2};
}}

.na-hud {{
  background-color: rgba(8,8,12,0.82);
  border-radius: 999px;
  color: #ffffff;
  border: 1px solid rgba(255,255,255,0.10);
  box-shadow: 0 12px 36px rgba(0,0,0,0.45), inset 0 1px 0 rgba(255,255,255,0.08);
}}

.na-banner {{
  background-color: {bg};
  color: {fg};
  border-radius: 18px;
  padding: 12px 16px;
  border: 1px solid {border};
  box-shadow: {shadow};
}}
.na-banner.critical {{ border-color: #ff5566; }}
.na-banner-action {{
  border-radius: 999px;
  padding: 4px 12px;
  background-color: {glass_2};
  font-size: 11px;
  font-weight: 700;
  color: {fg};
  transition: background-color 140ms {ease};
}}
.na-banner-action:hover {{ background-color: {accent}; color: #0d0e12; }}

.na-scroll > scrollbar {{
  opacity: 0;
  transition: opacity 180ms {ease};
}}
.na-scroll:hover > scrollbar {{ opacity: 1; }}
.na-scroll > scrollbar > range > trough {{
  background-color: transparent;
  min-width: 5px;
}}
.na-scroll > scrollbar > range > trough > slider {{
  border-radius: 4px;
  background-color: rgba(255,255,255,0.18);
  min-width: 5px;
  min-height: 24px;
}}

.na-glyph {{
  font-family: "{icon_font}";
  font-size: 16px;
}}
.na-glyph.lg {{ font-size: 22px; }}

.na-empty {{
  color: {fg_dim};
  font-size: 13px;
  font-weight: 500;
}}

popover.na-pop,
popover.na-pop > contents {{
  background-color: {bg2};
  color: {fg};
  border-radius: 14px;
  border: 1px solid {border};
  padding: 6px;
  box-shadow: {shadow};
}}
popover.na-pop button {{
  border-radius: 8px;
  padding: 6px 10px;
  color: {fg};
}}
popover.na-pop button:hover {{ background-color: {hover}; }}
"#,
        fg = fg,
        fg_dim = fg_dim,
        fg_mute = fg_mute,
        accent = accent,
        accent_rgb = accent_rgb,
        bg = bg,
        bg2 = bg2,
        border = border,
        hover = hover,
        glass = glass,
        glass_2 = glass_2,
        glass_3 = glass_3,
        shadow = shadow,
        icon_font = icon_font,
        ease = ease,
    )
}
