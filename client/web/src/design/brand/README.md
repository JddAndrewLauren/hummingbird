# Brand mark

The app icon, in the places the client renders it. Every file is a
byte-identical copy of a generated artifact from
`archive/app-icon-svg-v1:design/icon/`:

| File | Generated source | Role |
| --- | --- | --- |
| `hummingbird-icon-micro-light.svg` | `hummingbird-icon-micro-light.svg` | Nav rail mark, light theme (22px) |
| `hummingbird-icon-micro-dark.svg` | `hummingbird-icon-micro-dark.svg` | Nav rail mark, dark theme (22px) |
| `../../../public/favicon.svg` | `hummingbird-icon-favicon.svg` | Browser tab icon (16px) |
| `../../../public/app-icon.svg` | `hummingbird-icon-master-light.svg` | Installed PWA icon (large) |

The two rail marks live here because `NavRail` imports them, so Vite
fingerprints and bundles them. The other two live in `public/` because
`index.html` and the PWA manifest reference them by literal path.

**Never hand-edit these.** They are generator output, not drawings. The
generator (`scripts/icon_generator.py`) lives on the `archive/app-icon-svg-v1`
tag along with its spec, tests and QC evidence; it is not on `main` — the
icon program (#59, slices #60–#66) was shelved to that tag rather than
merged. To change the mark, restore the generator, edit its geometry data,
re-run it, and re-copy the output here.

**Why a different profile per role.** The generator emits three optical
profiles — `master` (1024–128px), `small` (64–32px) and `micro` (24–16px) —
over the same geometry model at reduced facet and feather counts, plus a
dedicated further-simplified favicon variation. Each consumer takes the
profile built for the size it renders at: the rail mark at 22px takes
`micro`, the tab icon takes the favicon variation, and the installed PWA
icon — which is rendered large — takes the full-detail `master`. Picking
one file for all three would be wrong in both directions: a downscaled
`master` mushes at 16px, and the favicon variation is nearly bare at 512px.
That is the whole reason the profiles exist.

**Why the PWA icon is not `maskable`.** A maskable icon must keep its
content within a circle of 80% the icon's diameter, since the platform may
crop to any mask inside that. This artwork does not — measured against the
outer silhouette on its 1024 canvas (safe radius 409.6), 9 of 26 boundary
points fall outside, the beak tip (135,70) furthest at radius 580.9. A
circular mask would cut off the beak, which is the silhouette's defining
feature. Containment needs roughly a 0.60 scale, i.e. a separately
generated maskable variant. Until the generator emits one, the manifest
declares `purpose: "any"` and lets the platform pad the icon instead.

**Still missing: `apple-touch-icon`.** iOS Safari ignores the manifest for
home-screen installs and wants an `apple-touch-icon` PNG, which cannot be
SVG. The generator's export program produces that raster matrix, but the
PNGs are deliberately treated as regenerable build output and are not
committed to the tag — so restoring this needs the exporter run, not a file
copy.

**Why two files.** The variants differ in plate and mass colours (the
generator's `DARK_PALETTE`), not geometry — `light` is a cream plate for the
light theme, `dark` a slate plate for dark. `NavRail` picks by resolved
theme.

**Why `<img>` and not inline SVG.** Both files define the same element ids
(`background-gradient`, `eye-iris-gradient`, `beak-clip`, …). Inlining both
into one document would collide those ids, and whichever paint server or
clip path resolved first would win for both marks. Referencing them as image
URLs keeps each file its own document, so the ids stay scoped.

The full-bleed `background` rect makes each file a square plate; the rail
rounds it with `--radius-icon-app` (22.37%, the icon plate's own squircle
ratio). The files also define an unused `preview-rounded-square` clip path —
deliberately not applied, so the plate stays square and its corner radius is
the consumer's call.
