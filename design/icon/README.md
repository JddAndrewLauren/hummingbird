# App icon render + QC harness

Supports the app-icon SVG build (plan #59, slices #60-#66). Slice #60
committed the approved reference and the render/QC harness. Slice #61
adds the generator: the silhouette + major color blocks, light and dark,
from one Python program.

## Contents

- `reference/concept-sheet.png` -- the full approved concept sheet (final
  light/dark renders, size ladder, small-size previews), committed as
  the durable design reference.
- `reference/light-1024.png`, `reference/dark-1024.png` -- the light- and
  dark-mode "FINAL ICONS" squares cropped out of the concept sheet, each
  resized to a clean 1024x1024 PNG. These are what the harness overlays
  and contact-sheets renders against. The source crops (out of
  `concept-sheet.png`, ImageMagick geometry `WxH+X+Y`) are
  `303x329+37+64` for light and `297x329+385+64` for dark -- both are
  stretch-to-square normalizations of non-square source regions, so
  light and dark disagree by ~2% in internal art proportions. Worth
  knowing if a later slice's overlay QC shows a small, consistent
  alignment drift between variants that isn't in the generator's geometry.
- `stub.svg` -- a placeholder SVG (colored rectangle) for exercising the
  harness before any real icon geometry exists (#60 only; the generator
  below produces the real masters).
- `hummingbird-icon-master-light.svg`, `hummingbird-icon-master-dark.svg`
  -- generated master SVGs (#61). 1024x1024, full square, self-contained.
  Regenerate with `scripts/icon_generator.py`; never hand-edit.
- `qc/contact-sheet-{light,dark}.png` -- committed gate evidence for the
  current generator output (spec §47 QC battery, size ladder beside the
  reference crop).
- `qc/grayscale-{light,dark}.png`, `qc/blur-{light,dark}.png`,
  `qc/silhouette-{light,dark}.png`, `qc/overlay-{light,dark}.png` --
  committed gate evidence for the remaining QC battery modes (spec
  §48-§50, §37) against the current generator output. Note:
  `silhouette-*.png` renders solid black end to end -- the harness's
  silhouette mode colorizes every opaque fill, including the full-bleed
  `background` rect, to black (spec §50 assumes a transparent-background
  render); it can't be run meaningfully on the masters as-is.

## Head identity (#62)

Adds the spec's fidelity priorities 1-5 (§38) on top of #61's silhouette
and major color blocks: the beak as 4 shapes (main + two facet planes +
a narrow reflective strip, all clipped to the beak's own envelope so
nothing bleeds into the crown/background), the eye as ring + iris (with
a subtle radial gradient) + two highlights, the dark eye-stripe wedge,
14 irregular crown facets (clipped to the crown envelope, cycling
through the spec's 5-step gray ramp), the restrained orange forehead
patch, and the cheek separator as one filled tapered Bézier shape. All
of this geometry is palette-independent (the spec gives no light/dark
split for it), so light and dark masters share it byte-for-byte; only
the underlying mass colors (crown/gorget/chest/side-body) still vary by
variant.

The eye's primary highlight uses the spec's literal ~25x18 master figure
(rx 12.5/ry 9). An earlier revision widened it, on a mistaken pixel
sample; re-sampling the 32px render at the literal size shows the
highlight pixel at luma ~113 against a surrounding eye of ~14-60 --
already clearly visible there, so the literal size was kept. Widening
it further breaches the eye ring's own outline at 1024px, reading as a
blown-out patch rather than a glint.

The cheek separator (§13) is emitted after `eye-stripe`/
`eye-stripe-secondary` rather than in §34's literal `cheek-light`
position below `crown-base` -- at that position the crown, forehead and
eye-stripe masses drawn afterward cover all but a sliver of it. Spec §34
explicitly allows "eye stripe / crown ordering adjustments"; raising it
here is what makes the streak actually visible, separating cheek from
gorget near the eye instead of reading as stray notches.

## Generator

`scripts/icon_generator.py` holds the geometry model (spec §4 composition
landmarks, §21 outer-silhouette boundaries, §9 crown envelope, §14 gorget
envelope) and both palettes (§7 colors, §23 dark-mode shifts) as data, and
emits both master SVGs from one code path -- dark differs from light only
by the values in `DARK_PALETTE`, never by separate drawing code. This
slice (#61) draws only the background, the outer bird silhouette, and
flat major color regions (crown, gorget, chest, side-body masses); head
identity, feathers and facets are later slices (#62-#64).

```bash
# Emit both master SVGs (default: design/icon/).
python3 scripts/icon_generator.py --out-dir design/icon
```

## Tool dependencies

- [`resvg`](https://github.com/RazrFalcon/resvg) -- deterministic,
  browser-free SVG rasterization. Verified against 0.48.1.
- ImageMagick's `magick` CLI -- compositing, resizing, colorspace and QC
  operations. Verified against 7.1.2.

Both are plain CLI binaries; nothing here needs a browser, a display, or
network access. On macOS: `brew install resvg imagemagick`.

## Usage

The harness is `scripts/icon_harness.py`, invoked from the repo root. Every
mode is one subcommand, one documented command:

```bash
# Rasterize at the full 1024/128/64/32/16 ladder, actual pixel dimensions.
python3 scripts/icon_harness.py render design/icon/stub.svg \
    --out-dir /tmp/renders

# One image: all five sizes (nearest-neighbor upscaled for display) beside
# the matching 1024px reference crop.
python3 scripts/icon_harness.py contact-sheet design/icon/stub.svg \
    --variant light --out /tmp/contact-sheet-light.png

# QC mode (spec §48): full grayscale conversion.
python3 scripts/icon_harness.py grayscale design/icon/stub.svg \
    --out /tmp/qc-grayscale.png

# QC mode (spec §49): ~8px blur preview.
python3 scripts/icon_harness.py blur design/icon/stub.svg \
    --out /tmp/qc-blur.png

# QC mode (spec §50): every fill flattened to black (silhouette).
python3 scripts/icon_harness.py silhouette design/icon/stub.svg \
    --out /tmp/qc-silhouette.png

# ~50% opacity overlay of the render on its reference crop, for eyeballing
# alignment and silhouette drift against the approved concept (spec §37).
python3 scripts/icon_harness.py overlay design/icon/stub.svg \
    --variant dark --out /tmp/qc-overlay-dark.png
```

`--variant` is `light` or `dark` wherever a mode needs a reference crop.
`grayscale`/`blur`/`silhouette` default to a 1024px render (`--size` to
override); `contact-sheet` always renders the full size ladder.

Later slices commit these PNGs as gate evidence -- the file names above
(`{mode}-{variant}.png`, or the render ladder's `{svg-stem}-{size}.png`)
are the predictable convention to follow.

## Tests

`python3 -m unittest discover -s tests` runs `tests/test_icon_harness.py`
(against the real `resvg`/`magick` binaries -- no mocking, the harness's
entire job is shelling out to them correctly; those tests skip themselves
if the binaries aren't on `PATH`) and `tests/test_icon_generator.py`
(pure-Python structural checks -- valid SVG, only spec-permitted
elements, semantic IDs, light/dark geometry parity -- plus one harness
round-trip test that also skips without `resvg`).
