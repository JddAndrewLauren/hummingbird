# App icon render + QC harness

Supports the app-icon SVG build (plan #59, slices #60-#66). This slice
(#60) commits the approved reference and the harness every later slice
iterates against. It draws no part of the bird -- `stub.svg` here is a
colored rectangle used only to exercise the harness.

## Contents

- `reference/concept-sheet.png` -- the full approved concept sheet (final
  light/dark renders, size ladder, small-size previews), committed as
  the durable design reference.
- `reference/light-1024.png`, `reference/dark-1024.png` -- the light- and
  dark-mode "FINAL ICONS" squares cropped out of the concept sheet, each
  resized to a clean 1024x1024 PNG. These are what the harness overlays
  and contact-sheets renders against.
- `stub.svg` -- a placeholder SVG (colored rectangle) for exercising the
  harness before any real icon geometry exists.

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
against the real `resvg`/`magick` binaries (no mocking -- the harness's
entire job is shelling out to them correctly). Those tests skip themselves
if the binaries aren't on `PATH`.
