# Install Naarchy

## Requirements

Naarchy targets Linux with a Wayland compositor supporting the layer-shell
protocol. Hyprland is the primary desktop integration; hover and fullscreen
courtesy use its IPC. It is not a macOS application and does not run on X11.

Build requirements: Rust 1.92+, a C toolchain, `pkg-config`, GTK 4.14+, and
`gtk4-layer-shell` 1.0+. A session D-Bus is needed for media and desktop services.

On Arch Linux / Omarchy:

```bash
sudo pacman -S --needed base-devel gtk4 gtk4-layer-shell rust
git clone https://github.com/michaelmonetized/naarchy.git
cd naarchy
cargo install --path . --locked
```

The binary is installed to `~/.cargo/bin/naarchy`. Add `~/.cargo/bin` to your
session's `PATH`, or use the full path. Verify with `naarchy --version` and
`naarchy doctor`, then start `naarchy run` from a terminal in your desktop.

Optional packages:

| Package / command | Purpose |
|---|---|
| `pamixer` or `wpctl` | Detect the current volume for a HUD |
| `brightnessctl` | Detect brightness for a HUD |
| `pipewire-audio` (`pw-play`) | Timer alarm audio; other installed players may be used as fallbacks |
| `ttf-jetbrains-mono-nerd` | Default interface icons |
| `xdg-utils` (`xdg-open`) | Open files, meeting links, and directions |
| Spotify / cliamp | Optional media launchers; other MPRIS players also work |

## Start with your session

Choose one autostart method. The systemd unit is recommended for source installs.

For `cargo install`, generate a unit with the correct executable path:

```bash
mkdir -p ~/.config/systemd/user
sed 's|ExecStart=/usr/bin/naarchy run|ExecStart=%h/.cargo/bin/naarchy run|' \
  contrib/naarchy.service > ~/.config/systemd/user/naarchy.service
dbus-update-activation-environment --systemd WAYLAND_DISPLAY DISPLAY XDG_CURRENT_DESKTOP
systemctl --user daemon-reload
systemctl --user enable --now naarchy.service
```

For a package installing `/usr/bin/naarchy` and the unit, run only
`systemctl --user enable --now naarchy.service` after importing the compositor
environment. Ensure your desktop starts `graphical-session.target`.

Add this line to Hyprland so future logins import the same environment:

```ini
exec-once = dbus-update-activation-environment --systemd WAYLAND_DISPLAY DISPLAY XDG_CURRENT_DESKTOP
```

Alternatively, copy `contrib/naarchy.desktop` to `~/.config/autostart/` on a
session that supports XDG autostart. Its `Exec=naarchy run` needs the executable
on the session `PATH`. Do not enable both methods or add a second `exec-once`.

## Keyboard shortcuts and blur

`naarchy install-binds` prints a suggested Hyprland configuration. Review the
output, resolve any conflicts with your current shortcuts, and copy the desired
lines into a configuration file that Hyprland already sources. Reload Hyprland
with `hyprctl reload`.

The supplied shortcuts include `Super+N`, `Super+Shift+N`, and `Super+V`.
Blur rules in the generated snippet target the `naarchy` layer namespace.
Hyprland syntax varies by installed version; check reload errors if a rule is
rejected. The application remains usable without compositor blur.

## Physical notch and scaling

The default island width is 370 logical pixels. Use [appearance
settings](CONFIG.md) to match your display and compositor scale. For a physical
notch, adjust these values to fit your own hardware:

```toml
[appearance]
notch_mode = true
pill_width_notch = 190
margin_top = 0
```

## Release artifacts and Arch packaging

Tagged releases produce a source archive and native Linux packages for x86-64
and ARM64, including the binary, desktop launcher, icon, and user service.
[Download the latest release](https://github.com/michaelmonetized/naarchy/releases/latest)
and choose `naarchy-x86_64-unknown-linux-gnu.tar.gz` or
`naarchy-aarch64-unknown-linux-gnu.tar.gz` to match your machine.
Binaries dynamically link GTK and layer shell, so runtime dependencies must be
installed. Verify the accompanying SHA-256 checksums before installation.

After extracting the package, run `bash scripts/install.sh` from its directory.
This installs into `~/.local/bin`, your application menu, and the user systemd
unit directory. It does not start the service. Add `~/.local/bin` to your `PATH`,
then use the autostart steps above (the unit already contains the correct path).

The same installer works from a source checkout after
`cargo build --release --locked`. `PREFIX` changes the install prefix; `DESTDIR`
stages files for packaging without updating your desktop.

`contrib/PKGBUILD` builds from the corresponding Git tag using makepkg's VCS
source support. An unreleased checkout can be installed with the source command
above. Do not assume an AUR package or a release tag exists.

## Update or remove

Update your source checkout, repeat `cargo install --path . --locked`, and
restart with `systemctl --user restart naarchy.service`. Keep your configuration
and data unless you intentionally want to remove them.

To stop and uninstall a `cargo install` installation:

```bash
systemctl --user disable --now naarchy.service
cargo uninstall naarchy
```

For an installation made with `scripts/install.sh`, stop the service, then remove
`~/.local/bin/naarchy`, `~/.local/share/applications/naarchy.desktop`, and
`~/.local/share/icons/hicolor/scalable/apps/app.naarchy.Naarchy.svg` instead of
running `cargo uninstall`. Adjust these paths if you set a different `PREFIX`.

Remove the user unit or desktop autostart entry you installed, then reload
systemd with `systemctl --user daemon-reload`. The [configuration guide](CONFIG.md)
lists data locations if you separately choose to erase your history.

## Troubleshooting

Start with `naarchy doctor`. It reports local setup without printing feed URLs
or clipboard content. For a systemd installation, inspect
`journalctl --user -u naarchy.service -b`.

| Symptom | What to check |
|---|---|
| `status=203/EXEC` | Source installs need `%h/.cargo/bin/naarchy run` in the user unit |
| Missing `WAYLAND_DISPLAY` | Import the compositor environment and start from your graphical session |
| Already running | A daemon already owns the socket; the second launch exits successfully |
| No hover | Enable `hover_open` and run on Hyprland, or click / use `naarchy toggle` |
| Missing glyphs | Install the Nerd Font or configure `appearance.icon_font` |
| No clipboard capture | Enable the clipboard feature and use a compatible Wayland clipboard protocol |
| No calendar entries | Verify the ICS URL, restart after changing feeds, and check logs |
| Another notification daemon stops working | Leave `features.notifications = false` to retain mako or dunst |
| Flat background | Check the compositor blur rules; this does not affect functionality |

When reporting an issue, include `naarchy --version`, your compositor version,
display scale, and relevant logs with personal content and private feed URLs removed.
