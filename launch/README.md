# Naarchy launch · October 6, 2026

A little island for everything on your Linux desktop.

Product Hunt is scheduled for **October 6 at 12:01 AM PDT** (3:01 AM EDT / 07:01 UTC). Michael Hurley is the hunter and maker. The launch has the new icon, four gallery images, Linux / Productivity / GitHub tags, free pricing, an open-source flag, and the first maker comment.

- Landing page: https://michaelchurley.com/portfolio/naarchy
- Product Hunt: https://www.producthunt.com/products/naarchy?launch=naarchy
- Manage launch: https://www.producthunt.com/products/naarchy/naarchy/prelaunch
- Software: https://github.com/michaelmonetized/naarchy/releases/tag/v0.4.0

## Files

| Folder | Contents |
| --- | --- |
| `brand/` | Editable five-shape Omadesign identity, native plugin, SVG and PNG |
| `motion/` | Editable 4-second, 30-fps Omadesign animation, animated SVG and Lottie |
| `gallery/` | Four Product Hunt images and social preview |
| `sources/` | Original 20.5-second recording of actual Omadesign use |
| `video/` | Editable Remotion compositions, pinned dependencies and lockfile |
| `artifacts/delivery/` | Three 38-second launch films and two 24-second Omadesign campaign films |
| `campaign-copy.md` | Launch and cross-promotion copy |
| `product-hunt-receipt.json` | Saved scheduling receipt |

The release film uses real Naarchy release screenshots and the existing Hyprland recording. It opens with frames rendered by Omadesign's native animation engine. It contains on-screen copy and no voiceover or supplied music. The campaign recording shows the real editor restoring the mark, selecting its five objects, opening Motion, applying Fade in, and playing the native timeline. This source is preserved at its original speed; the campaign uses an 80% playback rate.

Product Hunt accepts YouTube or Loom for its video field. The finished release film is hosted on the landing page; no external video upload is claimed.

## Rebuild

Use Bun for the video project. Omadesign 0.6.3 source at commit `c4813ef9a9628274778047e880a3aff6ea98d199` is the native rendering dependency. The helper's Cargo manifest points to the matching local checkout at `/home/michael/Projects/omadesign/.worktrees/release-063`; change that path to your own matching checkout when rebuilding elsewhere.

From the Naarchy repository root:

```sh
mkdir -p launch/artifacts/native-tmp
TMPDIR="$PWD/launch/artifacts/native-tmp" CARGO_TARGET_DIR="$PWD/launch/artifacts/cargo-target" cargo run --manifest-path launch/scripts/native-renderer/Cargo.toml --locked
bash launch/scripts/prepare-video.sh
cd launch/video
bun install --frozen-lockfile
bun run typecheck
cd ../..
bash launch/scripts/render.sh
```

`REMOTION_BROWSER` can override `/usr/bin/chromium`. Open `launch/brand/naarchy-logo.oma` or `launch/motion/naarchy-reveal.oma` in Omadesign to edit the artwork. Run `bun run studio` inside `launch/video` to edit the film.

## Campaign sequence

1. October 5: use the making film and the Omadesign story below to show the identity taking shape.
2. October 6, 12:01 AM PDT: publish the landscape launch film with the Product Hunt link. Use the square cut for feed posts and the vertical cut for short-video placements.
3. October 6 morning: show one actual feature per post, beginning with Inbox and local clipboard history. Answer questions with real screenshots and installation details.
4. October 7: publish the making film from Omadesign's account with the native editable files. Follow with real launch results when available.

Social posts are prepared, not sent or scheduled to an external social account. Never invent votes, testimonials, rankings, performance claims, or launch results.
