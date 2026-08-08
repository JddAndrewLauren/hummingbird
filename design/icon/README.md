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
- `hummingbird-icon-small-{light,dark}.svg`,
  `hummingbird-icon-micro-{light,dark}.svg` -- generated small/micro
  optical variants (#65). Same 1024x1024 viewBox and geometry model as the
  master, at reduced facet/feather counts (spec §24). Never hand-edit;
  regenerate with `scripts/icon_generator.py` alongside the masters.
- `qc/contact-sheet-{light,dark}.png` -- committed gate evidence for the
  current generator output (spec §47 QC battery, size ladder beside the
  reference crop).
- `qc/contact-sheet-{small,micro}-{light,dark}.png` -- the same
  upscaled-for-legibility contact sheet, run against the small/micro
  profiles (#65) instead of the master.
- `qc/actual-size-{small,micro}-{light,dark}.png` -- the small/micro
  acceptance evidence itself: 64/32/24/16px renders at their own true
  pixel size, side by side, with no upscaling to flatter the read (unlike
  the contact sheet above). This is what spec §24's "small at 32px" and
  "micro at 16px" reads are actually judged against.
- `qc/grayscale-{light,dark}.png`, `qc/blur-{light,dark}.png`,
  `qc/silhouette-{light,dark}.png`, `qc/overlay-{light,dark}.png` --
  committed gate evidence for the remaining QC battery modes (spec
  §48-§50, §37) against the current generator output. `silhouette-*.png`
  now excludes the master's own full-bleed `background` rect (see #64
  below) and shows a real silhouette rather than a solid black square.
- `hummingbird-icon-favicon.svg` -- the dedicated, further-simplified
  favicon variation (#66, spec §45). Never hand-edit; regenerate with
  `scripts/icon_export.py svgs`.
- `android/background.svg`, `android/foreground.svg` -- Android
  adaptive-icon SVG layers (#66, spec §44; SVG only, no APK/resource
  packaging). Never hand-edit; regenerate with `scripts/icon_export.py svgs`.
- `qc/favicon-actual-16.png` -- the favicon's own 16px render, nearest-
  neighbor upscaled for legibility (same convention as the small/micro
  actual-size sheets). Regenerate with `scripts/icon_export.py qc`.
- `qc/android-adaptive-safe-zone.png` -- the composited background +
  foreground layers with Android's official 66/108dp safe-zone circle
  overlaid in red. Read this as "where the circle actually falls on the
  real artwork", not as evidence of full containment: the chest and
  side-body masses visibly extend past it, and the gorget bleeds past it
  on both sides (see the "Export matrix + platform packaging" section
  below for the full pixel measurement and the beak-tip landmark
  exception). Regenerate with `scripts/icon_export.py qc`.

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

## Gorget feather system (#63)

Replaces the flat gorget color block with the spec's main visual identity:
one unified `gorget-base` under-shape (§14, unchanged from #61/#62) plus
five overlapping rows (`gorget-top-row` through `gorget-bottom-row`,
`§16`/`§34` naming) of a seeded, jittered rounded-shield primitive (§15),
37 feathers total on the master. `scripts/icon_generator.py`'s
`ROW_SPECS` holds each row's count/width/height/x-span/color-ramp-span as
data (`_build_gorget_feathers`, seeded by the fixed `FEATHER_SEED`), so a
later taste pass or the small/micro slice (#65, reduced counts) changes
parameters, not drawing code. Every feather's own `d` path text is
distinct -- width, height, rotation and each Bezier control point's
curvature are independently jittered per instance, then baked into its
coordinates (not left as a separate `transform`), and the whole pass is
deterministic (same `FEATHER_SEED`, same output every run) so the
generator stays byte-stable.

Row-to-row vertical placement (`y_center` per row in `ROW_SPECS`) is
hand-placed, not derived from a single overlap constant -- §16's literal
per-row width/height figures only produce ~200px of legitimate row-to-row
overlap stacked from a chin-height start, well short of the ~390px the
gorget-base envelope actually spans top-to-bottom, so a purely
formula-derived layout (an earlier version of this file) left the lower
gorget essentially bare (review on PR #87 measured 50.7% feather coverage
overall, 0% in the lowest two 50px bands). Row 4/5 are also widened beyond
§16's literal numbers (still "the largest shapes", per §16's own steer)
and pushed down so the cascade reaches the chest overlap §16 calls for.
`GorgetFeatherTest.test_row_overlap_measured_from_generated_geometry`
verifies real overlap (15-95% intersection of each row's own actual
rendered y-extent, not a recomputation of the placement formula) between
every adjacent pair, and `GorgetLowerCoverageRenderTest` rasterizes the
real master and samples the lower gorget to confirm it's not bare base
color. The `gorget-feathers` group is clipped to `GORGET_MASS_PATH` (same
`clipPath` pattern as #62's `crown-clip`), so feathers this large can
overshoot the envelope freely at generation time and still never render
outside the mass's own silhouette -- `GorgetFeathersClippedToEnvelopeTest`
guards the clip is wired up.

Document order follows §34's z-stack -- `gorget-bottom-row` (Row 5,
nearest the chest, the largest feathers) painted first/bottommost, up
through `gorget-top-row` (Row 1, nearest the chin) painted last/topmost --
so each row's feathers overlap down into the row below, cascading like
shingled scales per §17 ("no outlines between feathers; separation from
color contrast only"). `GORGET_FEATHER_RAMP` deliberately excludes §7's
"primary orange" (#FF8500): it's also `LIGHT_PALETTE`'s `gorget_mass` fill
(and `DARK_PALETTE`'s is only ~4% brighter), so a feather using it would
be invisible against its own base -- review on PR #87 caught 6 such
feathers; `test_no_feather_fill_matches_either_variants_base_gorget_fill`
guards it.

Color follows §18's directional map via `GORGET_FEATHER_RAMP` (the §7
orange/gorget seven-step hex ramp, warmest yellow-orange to coolest
gorget-shadow): each feather's fill is chosen by its left-to-right column
position within its row against that row's own `color_index_range`, so
Row 1 ("darkest reds concentrated near right side") spans the ramp's red
end, Row 3 (visually dominant) spans the full ramp, and Row 4 ("more
gold/orange toward left") stays toward the warm end -- never a random
scatter.

## Chest + side-body facets, optical cleanup (#64)

Replaces the flat `chest-base`/`side-body-base` color blocks with a low-poly
facet overlay -- the last geometry this master needs; both masters now
pass the full QC battery (spec §47-50).

- **Chest (§19):** 18 facets (`chest-facet-01`..`18`), built by ear-clip
  triangulating `CHEST_MASS_POINTS` (`_ear_clip` -- the standard
  algorithm for triangulating a simple polygon, concave or convex, with
  no interior Steiner points), then bisecting any triangle whose bbox
  exceeds spec's ~100-250px facet-size band at its longest edge's
  midpoint (`_subdivide_to_size`), up to an 18-facet cap. Round-1 review
  on PR #89 caught an earlier version of this that fanned from a single
  off-center apex instead: `CHEST_MASS_POINTS` is concave (its top
  boundary sags between the two sharp top corners, pinching the envelope
  into two lobes), so no single apex can see every edge in a straight
  line, and 4 of 11 fan facets rendered <=1% actual visible area once
  the chest-clip trimmed away the part of each triangle that fell
  outside the real silhouette -- while the dominant facet came out
  roughly double the size band. Ear-clipping sidesteps the "can one
  point see every edge" requirement entirely: every triangle it emits
  is *exactly* a piece of the source polygon, and edge-midpoint
  subdivision is an affine combination of a parent triangle's own
  vertices, so that 100%-inside guarantee carries through subdivision
  too -- not just empirically likely, but true by construction. 18
  facets against the gorget's 37 feathers is still the "dramatically
  simpler" contrast §19 calls for.
- **Side-body (§20):** 9 facets (`side-body-facet-01`..`09`), built by
  banding `SIDE_BODY_MASS_POINTS`' own inner/outer edge chains -- derived
  as slices of that same point list, not separately hand-typed literals
  -- into 3 bands top-to-bottom, then each band into 3 further sub-quads
  along its length -- wide, elongated planes rather than small rounded
  shapes, per §20's explicit "long polygon planes rather than small
  feather shapes" and §28's "two different geometric languages"
  (angular/faceted head and chest vs. rounded gorget). Unlike the chest
  envelope, this strip is convex enough along its own length that
  banding it directly already keeps every quad exactly inside the
  envelope.
- Both facet groups are clipped to their own envelope (`chest-clip`,
  `side-body-clip`, same `clipPath` pattern as `crown-clip`/`gorget-clip`)
  as a structural safety net on top of the exact-tiling guarantee above,
  and both use only their spec-given color ramps (§19's cream ramp, §20's
  brown-orange ramp) with no strokes.
- `ChestAndSideBodyFacetVisibilityTest` measures each facet's actual
  generated-geometry visible-area fraction (the same technique
  `GorgetFeatherVisibilityTest` uses for feathers) rather than only
  counting SVG elements, so a facet clipped away to near-nothing can't
  silently count toward the 10-18/8-12 range the way it did before
  round-1 review -- and a companion test checks every chest facet's
  bounding box stays within the §19 size band.
- Layer order (§34): `chest-base` -> `chest-facets` -> `side-body-base` ->
  `side-body-facets` -> `gorget-base`, unchanged from #61-#63 otherwise.
- Total path/polygon/ellipse count is now 106 (both variants), inside
  spec §35's ~85-110 budget.
- Safe area (§26) and optical center (§27) were already satisfied by the
  #62 landmark placement (beak tip at (135,70), well outside the 65-unit
  edge margin) and are unchanged by this slice's chest/body-only
  additions; a regression test (`SafeAreaTest`) guards the beak-tip
  margin.
- The silhouette QC harness mode (`icon_harness.silhouette`) previously
  rendered solid black end-to-end for a master SVG, because it colorized
  every opaque pixel including the master's own full-bleed `background`
  rect -- the earlier note below about this is now stale. `silhouette()`
  strips the `id="background"` element before rasterizing, so only the
  bird itself flattens to black over a transparent canvas, which is what
  spec §50 actually wants to inspect. `qc/silhouette-{light,dark}.png`
  are regenerated against this fix and now show a real silhouette (long
  diagonal beak, round forehead, large upper body, cropped composition).

## Small/micro optical variants (#65)

Adds two more profiles over the *same* geometry model, rather than
separately redrawn artwork -- spec §24's "variant profiles that dial the
same geometry model down." `icon_generator.PROFILES` holds three parameter
sets (`master`/`small`/`micro`); `_build_svg(palette, profile)` renders any
of them through the identical template `_build_svg(palette)` (the master
call, unchanged) always used. A shape tweak to the master's own data
(`CROWN_FACETS`, `CHEST_MASS_POINTS`, `GORGET_FEATHER_RAMP`, ...)
propagates to small/micro on the next `generate_all()` run, because every
small/micro shape is built from that same data at a reduced parameter, not
a separately hand-typed shape set:

- **Gorget feathers ("throat shapes"):** small uses `SMALL_ROW_SPECS` (same
  5 rows, same x-range/y-center/color-ramp-span per row as the master's own
  `ROW_SPECS`, roughly half the count and modestly widened per-feather size)
  -- 19 feathers, spec §24's 16-22 band. Micro's `MICRO_ROW_SPECS` reduces
  further to 11, spec's 8-12 "throat shapes" band. Both run through the
  master's own `_build_gorget_feathers`/`_feather_path_d` (same primitive,
  same ramp), just seeded distinctly (`FEATHER_SEED + 1`/`+ 2`) so they
  don't replay a truncated prefix of the master's own sequence.
- **Crown facets ("crown planes"):** `SMALL_CROWN_FACETS`/
  `MICRO_CROWN_FACETS` are literal index subsets of the master's own
  14-facet `CROWN_FACETS` list (7 and 4 facets respectively, spec §24's
  6-8/3-4 bands), picked to spread across the crown's front-to-rear span
  and gray ramp rather than clustering. The gaps this leaves in the crown
  mass show the flat `crown-base` color underneath -- correct
  simplification, not missing geometry.
- **Chest facets ("chest planes"):** `_ear_clip` on the full 11-vertex
  `CHEST_MASS_POINTS` always yields exactly 9 triangles (n-2) before any
  subdivision -- already above small's 6-8 band, and `_subdivide_to_size`
  can only add more, never fewer. `CHEST_MASS_POINTS_SMALL`/`_MICRO` are
  ordered *subsets* of the master's own 11-point envelope (8 and 5 points),
  sized so ear-clipping them directly lands on 6 and 3 triangles. Each
  subset polygon is still a subset of the real chest silhouette, so it
  stays exactly inside `chest-clip` even though (like the crown facets
  above) it leaves part of that silhouette to the flat `chest-base` color.
- **Side-body facets:** not in spec §24's small/micro shape budgets at all.
  Per §38's fidelity-priority order (individual facet/polygon detail is
  priority 8-9, first to go), small keeps a reduced count
  (`_build_side_body_facets(subdivisions=2)`, 6 facets) and micro drops the
  overlay entirely (`subdivisions=0`), leaving the flat `side-body-base`
  color -- "warm orange body" (priority 7) still reads; only its own facet
  detail doesn't.
- **Beak:** small keeps the master's full 4-shape beak unchanged -- spec
  §24's "single beak highlight" is already true of the master's own single
  `beak-highlight` element, so nothing to reduce. Micro collapses to "one
  beak shape": the beak envelope itself, flat-filled, no facets or
  highlight.
- **Eye:** master keeps all four pieces (ring, gradient iris, two
  highlights). Small is "simplified" -- drops the secondary glint, keeps
  ring/iris/primary highlight. Micro is "one black eye, one white eye
  highlight" -- one flat-filled ring (no separate iris/gradient) plus the
  primary highlight only.
- **Eye stripe:** master/small keep both the main wedge and the secondary
  depth plane; micro is "one eye-stripe shape" -- the main wedge only.
- **Forehead patch / cheek separator:** neither is in spec §24's small or
  micro shape lists. Small keeps both unchanged (unlisted items stay
  unless the budget calls for their removal); micro omits both.

`generate_all()` emits all six SVGs (master/small/micro x light/dark) in
one run, per spec §40's naming
(`hummingbird-icon-{master,small,micro}-{light,dark}.svg`). `generate()`
is unchanged -- still emits only the two master SVGs -- so every existing
master-only call site keeps working untouched.

`icon_harness.actual_size_sheet` (`actual-size-sheet` CLI subcommand) is
the QC mode this slice needed that `contact_sheet` doesn't provide: the
64/32/24/16px ladder at each render's own true pixel size, side by side,
with no nearest-neighbor upscaling to a common display size. A small/micro
profile has to read correctly at the size it will actually ship at,
without an upscaled preview flattering (or unfairly penalizing) that read
-- `qc/actual-size-{small,micro}-{light,dark}.png` is the committed
evidence spec §24's small-at-32px / micro-at-16px acceptance reads are
judged against.

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
# Emit all six SVGs -- master/small/micro x light/dark (default: design/icon/).
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

# Small/micro acceptance evidence (#65): 64/32/24/16px at their own true
# pixel size, side by side, no upscaling.
python3 scripts/icon_harness.py actual-size-sheet \
    design/icon/hummingbird-icon-small-light.svg \
    --out /tmp/actual-size-small-light.png
```

`--variant` is `light` or `dark` wherever a mode needs a reference crop.
`grayscale`/`blur`/`silhouette` default to a 1024px render (`--size` to
override); `contact-sheet` always renders the full size ladder.

Later slices commit these PNGs as gate evidence -- the file names above
(`{mode}-{variant}.png`, or the render ladder's `{svg-stem}-{size}.png`)
are the predictable convention to follow.

## Export matrix + platform packaging (#66)

`scripts/icon_export.py` consumes the six committed SVGs and produces
everything a platform actually ships. One documented command regenerates
every export from a clean checkout:

```bash
# SVG sources: favicon + Android layers, into design/icon/ (committed).
python3 scripts/icon_export.py svgs

# Everything else: PNG matrix + .icns + .ico, both variants, into
# design/icon/export/ (regenerable build output -- gitignored, not
# committed; run this whenever you need the rasters, not once-and-commit).
python3 scripts/icon_export.py all --out-dir design/icon/export
```

- **PNG matrix (spec §41):** `icon_export.EXPORT_MATRIX` maps each of
  1024/512/256/128 (master), 64/48/32 (small) and 24/16 (micro) to its
  own correct optical source -- never one master image resized to every
  size. Every render is normalized to sRGB/RGBA/8-bit with no embedded
  ICC profile (`_normalize_png`; the `png:color-type=6` define matters
  here -- without it, a fully-opaque small render like the 16px tile gets
  silently written back down to 3-channel RGB with no alpha channel at
  all, since ImageMagick's PNG encoder picks color-type from actual pixel
  content unless told otherwise).
- **macOS `.icns` (spec §42):** built via `iconutil` from a real
  `.iconset` (`ICNS_ICONSET_ENTRIES`, Apple's fixed `icon_NxN[@2x].png`
  naming), covering 16 through 1024. Every iconset tile is rendered from
  the same §41-correct optical source as the PNG matrix.
- **Windows `.ico` (spec §43):** built via ImageMagick from renders at
  all seven `ICO_SIZES` (16-256). The 16/24/32 entries come from the
  micro/small SVGs, not a master downscale --
  `IcoTest.test_16_24_32_entries_match_micro_small_artwork` extracts each
  embedded frame back out of the built `.ico` and pixel-compares it
  against a fresh render of its intended source, and a companion test
  confirms the 16px entry is visibly different from what a master
  downscale would produce.
- **Favicon (spec §45):** `FAVICON_PROFILE` reuses the master's own
  geometry/palette data at zero facet/feather counts (crown/chest/
  side-body facets, gorget feather rows all empty; single-shape beak;
  micro eye; single eye-stripe; no forehead/cheek) through
  `icon_generator._build_svg` -- no new artwork. 9 visible shapes (well
  under the ~15 cap), the literal gray-head/black-beak/black-eye/
  orange-throat/cream-chest color list, reading cleanly at actual 16px
  (`qc/favicon-actual-16.png`).
- **Android adaptive layers (spec §44):** `background.svg` is the flat
  background gradient alone; `foreground.svg` is the *full* master bird
  (spec gives no reduced shape budget here) with its background rect
  dropped and its `id="bird"` group wrapped in a
  `translate/scale(0.9)/translate` transform about the canvas center --
  the spec's own "shrinking the bird about 8-12%" (`ANDROID_FOREGROUND_SCALE
  = 0.90`, the band's midpoint).

  What that scale is actually checked against, and what it isn't:
  - **Checked and true:** the seven named points in
    `icon_generator.LANDMARKS` plus `EYE_CENTER` -- a small set standing
    in for head/eye/gorget *identity*, not the whole silhouette -- clear
    Android's real 66/108dp safe-zone circle once scaled
    (`test_head_identity_landmarks_clear_the_safe_zone_after_scaling`).
    `crown_top` specifically clears it only *after* scaling, not before
    (`test_unscaled_landmark_would_fail_without_the_shrink`) -- that's the
    landmark the 8-12% figure is calibrated against. The beak-tip spike
    (`beak_tip`/`beak_lower_tip`, reaching to (135,70), close to the
    canvas corner) is the one landmark that still fails even at max
    shrink -- pulling it inside would need roughly a 0.61 scale, well
    past the spec's own "8-12%" estimate -- so it's a documented,
    tested exception (`SAFE_ZONE_EXEMPT_LANDMARKS`), on the same
    precedent as spec §26's "body bleed at bottom" exception for the app
    icon's safe *area*.
  - **Not checked by the landmarks, and not true:** literal full-pixel
    containment. Rendering the actual `foreground.svg` and measuring
    every opaque pixel against the same circle
    (`foreground_opaque_pixel_outside_safe_zone_fraction`) shows roughly
    a third to a half of the foreground's own opaque pixels fall outside
    it -- the chest and side-body masses are entirely outside the
    circle, and the gorget bleeds past it on both sides. Achieving full
    pixel containment would need close to a 0.61 scale, which would read
    as a much smaller, over-shrunk bird relative to what §44's "8-12%"
    describes. 0.90 is a deliberate reading of the spec text (protect the
    named identity, accept the rest bleeding) over pixel-perfect
    compliance -- the same tradeoff real Android adaptive icons make in
    practice (content outside the safe circle can get cropped by some
    launchers' masks; only the guaranteed-safe circle's content survives
    everywhere).

  `qc/android-adaptive-safe-zone.png` overlays the real circle on the
  composited layers so this tradeoff is visible directly, not just
  described -- the head mass sits inside it; the chest/side-body/gorget
  visibly don't.
- **Not committed:** the PNG matrix, `.icns` and `.ico` are regenerable
  rasters (`design/icon/export/`, gitignored) -- client work under map #35
  runs `icon_export.py all` when it needs them rather than pulling stale
  binaries out of git. The favicon and Android SVGs *are* committed
  (same "generated source of truth, never hand-edit" rule as the six
  masters, with byte-equality staleness tests).
- **Committed QC evidence:** `qc/favicon-actual-16.png` and
  `qc/android-adaptive-safe-zone.png` are also committed, generated
  gate evidence -- regenerate both with `python3 scripts/icon_export.py
  qc` (documented, one command, same convention as the harness's own
  `qc/*.png`), and `CommittedQcRendersUpToDateTest` in
  `tests/test_icon_export.py` byte-compares each against a fresh build.

## Tests

`python3 -m unittest discover -s tests` runs `tests/test_icon_harness.py`
(against the real `resvg`/`magick` binaries -- no mocking, the harness's
entire job is shelling out to them correctly; those tests skip themselves
if the binaries aren't on `PATH`), `tests/test_icon_generator.py`
(pure-Python structural checks -- valid SVG, only spec-permitted
elements, semantic IDs, light/dark geometry parity -- plus one harness
round-trip test that also skips without `resvg`), and
`tests/test_icon_export.py` (export matrix, `.icns`/`.ico` build +
pixel-verified source, favicon shape budget, Android safe-zone landmark
math -- render/pack tests skip without `resvg`/`magick`, and the `.icns`
tests additionally skip without `iconutil`, i.e. off macOS).
