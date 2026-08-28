# Theming

Naarchy follows the desktop. Override only what you want to fight.

## Omarchy follow (default)

`appearance.omarchy = true` reads:

- slug: `$XDG_STATE_HOME/omarchy/current/theme.name`
- palette: `~/.config/omarchy/themes/<slug>/colors.toml`
  (`accent`, `background`, `dark_background`, `foreground`, `mode`)

Icon font: `appearance.icon_font`, else foot / ghostty / kitty / alacritty,
else `JetBrainsMono Nerd Font`.

`theme = "auto"` uses the omarchy `mode` when a palette is found. The
xdg-desktop-portal `color-scheme` flag is the **fallback** when omarchy colors
cannot be read or `omarchy = false`.

Portal map: 1 = prefer dark, 2 = prefer light, 0 = no preference → dark.

## Overrides

```toml
[appearance]
omarchy = false
theme = "dark"
accent = "#89b4fa"
pill_bg = "#000000"
bg = "rgba(0,0,0,0.62)"
fg = "#cdd6f4"
icon_font = "JetBrainsMono Nerd Font"
radius = 24
opacity = 0.98
```

A leftover `accent = "#7aa2f7"` with `omarchy = true` is treated as unset so
the theme accent wins.

There is no user CSS file. CSS is generated into a `CssProvider` scoped to
`window.naarchy`.

## Glass

Hyprland does the blur. Naarchy paints a translucent capsule with a hairline
and a bottom fade.

```
layerrule = blur, naarchy
layerrule = ignorealpha 0.2, naarchy
```

The layer namespace is `naarchy`. That string is the `layerrule` target.

## Notch vs island

Island is the default. Physical notch:

```toml
[appearance]
notch_mode = true
pill_width_notch = 190
margin_top = 0
```
