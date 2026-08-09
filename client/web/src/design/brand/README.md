# Brand mark

Every app-icon raster the client ships, derived from the **reference plate** —
the approved app icon artwork, which is also where the whole colour palette was
sampled from.

## Regenerating

    client/web/scripts/derive-app-icons.sh <dir with light-1024.png and dark-1024.png>

Requires ImageMagick. The outputs are committed; the script is how you
regenerate them, not part of the build. Never hand-edit the PNGs.

**The reference plates are not in this repo** — they are binary and were
omitted from the design-mirror pull. Get them from the Hummingbird Design
System project on claude.ai/design, which holds them twice:

| Path in the design project | Note |
| --- | --- |
| `assets/app-icon-{light,dark}-1024.png` | What the design system's own Brand cards and web UI kit reference. Prefer these. |
| `uploads/{light,dark}-1024.png` | The original uploads. |

The copies currently committed here were derived from
`archive/app-icon-svg-v1:design/icon/reference/{light,dark}-1024.png`, which is
the same artwork but reached by a different route: that tag cropped it out of
the concept sheet and stretched it to square. That crop is slightly off — on
the dark plate the squircle sits 161px from the left edge but 134px from the
right, a ~2.6% asymmetry the tag's own README documents. At 16–78px this is a
fraction of a pixel and invisible, but re-derive from the design project's
`assets/` when convenient and this note can go.

## What is generated, and why each size exists

| File | Consumer |
| --- | --- |
| `app-icon-{light,dark}-{26,52,78}.png` | Nav rail mark — 26px CSS, with 2x/3x for hidpi |
| `../../../public/favicon-{16,32,48}.png` | Browser tab |
| `../../../public/app-icon-{192,512}.png` | Installed PWA (manifest) |

Real pixels per size rather than one image the browser rescales: at 16px this
artwork is mostly gorget and beak, and a downscale of a larger plate muddies
exactly those.

The rail marks live here because `NavRail` imports them, so Vite fingerprints
and bundles them. The rest live in `public/` because `index.html` and the PWA
manifest reference them by literal path.

## How the corners are cut

The reference plates are opaque: the squircle is drawn on an opaque **white**
surround, so the corners are white rather than transparent. The script
flood-fills inward from all four corners to lift exactly that surround out,
which a geometric mask could not do — the corner is a superellipse, not a
circular arc, and the token's 22.37% is a squircle ratio, not a radius a
circular mask would reproduce.

The flood fill's fuzz has to clear the light plate's smallest separation from
white (blue channel, 231 vs 248 = 6.7%) without reaching it. 4% sits inside
that. It cannot eat the bird's own cream chest, which is enclosed by the plate
and so unreachable from a corner.

## Rules the design system sets

The Brand › App icon card and the design system's own `ui_kits/web/NavRail.jsx`
both settle questions this code would otherwise have to guess at:

- The rail mark is **26px**, squircled with `--radius-icon-app` (22.37%) in
  CSS, and carries **no border or plate of its own** — the card's own wording
  is "never on a coloured plate of its own".
- It is decorative: the wordmark beside it already names the app, so `alt=""`
  keeps screen readers from announcing the brand twice.
- Light and dark plates are both first-class; the rail picks by resolved theme.

## Known gaps

- **`apple-touch-icon`.** iOS Safari ignores the manifest for home-screen
  installs and wants an `apple-touch-icon` PNG. Unlike before, nothing
  technical blocks this now that the pipeline emits PNGs — but iOS applies its
  own mask to a full-bleed square, so it wants a plate whose corners are
  *filled*, not the transparent-cornered derivation used everywhere else. That
  is a deliberate extra output, not a size to add to the loop.
- **No maskable variant.** See the reasoning in `vite.config.ts`: this artwork
  puts 33.6% of the bird outside the maskable safe circle, so the manifest
  declares `purpose: "any"` only.
