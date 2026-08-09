# Brand mark

The app icon, as the nav rail's brand mark. Two files, byte-identical
copies of generated artifacts:

| File | Source |
| --- | --- |
| `hummingbird-icon-micro-light.svg` | `archive/app-icon-svg-v1:design/icon/` |
| `hummingbird-icon-micro-dark.svg` | same |

**Never hand-edit these.** They are generator output, not drawings. The
generator (`scripts/icon_generator.py`) lives on the `archive/app-icon-svg-v1`
tag along with its spec, tests and QC evidence; it is not on `main` — the
icon program (#59, slices #60–#66) was shelved to that tag rather than
merged. To change the mark, restore the generator, edit its geometry data,
re-run it, and re-copy the output here.

**Why the `micro` profile.** The generator emits three optical profiles —
`master` (1024–128px), `small` (64–32px) and `micro` (24–16px) — over the
same geometry model at reduced facet and feather counts. The rail mark
renders at 22px, so `micro` is the profile whose detail budget was designed
for this read. A downscaled `master` would mush at this size; that is the
whole reason the profiles exist.

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
