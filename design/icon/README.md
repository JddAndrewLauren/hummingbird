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
