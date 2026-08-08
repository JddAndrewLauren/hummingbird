#!/usr/bin/env python3
"""Icon generator (#61): geometry model + palettes -> master light/dark SVGs.

This slice draws only the background, the outer bird silhouette, and flat
major color regions (crown, gorget, chest, side-body masses) -- see
design spec issue #59 §4/§6/§7/§9/§14/§20/§21/§23. One generator function
(`_build_svg`) takes a palette dict and emits both variants; dark differs
from light only by the values in DARK_PALETTE, never by separate drawing
code. Later slices (#62-#66) extend GEOMETRY/palettes for head, feathers,
facets and optical variants without restructuring this module.
"""

import argparse
import math
import random
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_OUT_DIR = REPO_ROOT / "design" / "icon"

# ---------------------------------------------------------------------------
# Geometry model (spec §4 composition landmarks, §21 outer silhouette, §9
# crown envelope, §14 gorget base envelope, §20 side-body region). Coordinates
# are 1024x1024 SVG user units. A later slice extends this dict (feather
# parameters, facet lists) rather than replacing it.
# ---------------------------------------------------------------------------

LANDMARKS = {
    "beak_tip": (135, 70),
    "beak_lower_tip": (144, 86),
    "beak_upper_base": (415, 355),
    "beak_lower_base": (470, 425),
    "crown_top": (350, 220),
    "rear_crown": (695, 285),
    "gorget_top": (250, 390),
}

# Spec §21: right/left outer-silhouette boundary chains.
RIGHT_BOUNDARY = [(720, 350), (780, 470), (825, 610), (875, 760), (925, 920), (955, 1024)]
LEFT_BOUNDARY = [(250, 390), (210, 520), (175, 680), (135, 850), (100, 1024)]


def _pt(p):
    x, y = p
    return f"{x} {y}"


def build_bird_silhouette_path() -> str:
    """Outer bird silhouette (spec §4/§21): a thin beak spike attached to
    the rounded head/gorget/chest/body mass at two points -- crown_top
    (where the spike's upper edge meets the forehead) and beak_lower_base
    (where its lower edge meets the cheek/throat front). Traced clockwise
    from the beak tip: upper beak edge up to the forehead, over the round
    crown bump, down the back of the head into the right boundary chain,
    across the canvas-edge bottom bleed, up the left boundary chain to
    the gorget top, across the cheek/throat front to the beak's lower
    base, then back along the beak's lower edge to the tip.

    The two spike edges (tip->crown_top and beak_lower_base->tip) must
    stay strictly on their own sides the whole way -- otherwise the path
    self-intersects and the fill rule bites a stray hole out of the
    silhouette right where the beak meets the head (caught by eyeballing
    the silhouette QC render, spec §50)."""
    tip = LANDMARKS["beak_tip"]
    lower_base = LANDMARKS["beak_lower_base"]
    crown_top = LANDMARKS["crown_top"]
    rear_crown = LANDMARKS["rear_crown"]
    gorget_top = LANDMARKS["gorget_top"]

    segments = [f"M {_pt(tip)}"]
    # Upper beak edge, slightly convex (spec §8), straight into the
    # forehead -- no vertex doubles back on itself here.
    segments.append(f"C 230 110, 320 175, {_pt(crown_top)}")
    # Round forehead bump across the crown top to the rear of the head
    # (spec §50: "round forehead" must survive the silhouette test).
    segments.append(f"C 430 190, 590 215, {_pt(rear_crown)}")
    # Down the back of the head into the right boundary chain.
    segments.append(f"C 705 305, 715 330, {_pt(RIGHT_BOUNDARY[0])}")
    for point in RIGHT_BOUNDARY[1:]:
        segments.append(f"L {_pt(point)}")
    # Canvas-edge bottom bleed (spec §5: body intentionally bleeds past
    # the bottom edge).
    segments.append(f"L {_pt(LEFT_BOUNDARY[-1])}")
    for point in reversed(LEFT_BOUNDARY[:-1]):
        segments.append(f"L {_pt(point)}")
    # Cheek/throat front, from the left boundary's top up to the beak's
    # lower base.
    segments.append(f"C 320 380, 410 405, {_pt(lower_base)}")
    # Back along the beak's lower edge to the tip, closing the loop --
    # stays below/right of the upper edge curve the whole way.
    segments.append(f"C 350 320, 220 180, {_pt(tip)}")
    segments.append("Z")
    return " ".join(segments)


# Spec §9: crown envelope, given verbatim as the gray crown mass.
CROWN_MASS_PATH = (
    "M 340 230 C 420 190, 560 210, 670 270 C 705 290, 718 330, 710 380 "
    "L 625 405 L 430 355 Z"
)

# Spec §14: gorget base silhouette, given verbatim as the orange gorget mass.
GORGET_MASS_PATH = (
    "M 250 390 C 330 340, 470 350, 585 405 C 685 450, 760 535, 785 650 "
    "C 740 705, 650 750, 530 775 C 395 770, 285 720, 215 640 "
    "C 195 540, 205 455, 250 390 Z"
)

# Cream chest mass (spec §19): approximate lower-left region beneath the
# gorget's lower edge, reusing that edge's own anchor points so the two
# masses share a border instead of leaving a gap.
CHEST_MASS_POINTS = [
    (610, 710),
    (530, 775),
    (395, 770),
    (285, 720),
    (215, 640),
    (175, 680),
    (135, 850),
    (100, 1024),
    (560, 1024),
    (650, 850),
    (660, 750),
]

# Warm side-body mass (spec §20): approximate X 650-960 / Y 600-1024 region,
# bounded on the right/bottom by the outer silhouette's own boundary chain.
SIDE_BODY_MASS_POINTS = [
    (660, 600),
    (825, 610),
    (875, 760),
    (925, 920),
    (955, 1024),
    (560, 1024),
    (650, 850),
    (660, 750),
]


def _points_attr(points) -> str:
    return " ".join(_pt(p) for p in points)


# ---------------------------------------------------------------------------
# Head identity (#62, spec §38 fidelity priorities 1-5): beak planes, eye,
# eye stripe, crown facets, forehead patch, cheek separator. Coordinates and
# colors are geometry-model data, same discipline as the masses above; none
# of it depends on `palette` (the spec gives no light/dark variants for
# these), so it renders identically in both masters -- only the underlying
# crown/gorget/chest/side-body masses shift per variant.
# ---------------------------------------------------------------------------

# Spec §8: the beak's own envelope, sharing the outer silhouette's exact
# upper/lower bezier control points (so beak facets hug the true silhouette
# edge) but stopping at the base landmarks instead of continuing on into the
# crown/gorget. Used both as the "beak-main" fill and as the clip boundary
# for the facet planes and highlight strip below, so those can't bleed past
# the beak's own silhouette into the crown or background.
BEAK_ENVELOPE_PATH = (
    "M 135 70 C 230 110, 320 175, 415 355 C 435 375, 455 400, 470 425 "
    "C 350 320, 220 180, 144 86 Z"
)

# Spec §8: deep black main fill, two narrow faceting planes, one reflective
# strip -- 4 shapes total (within the required 3-5), "never one black
# triangle." None of these are flat #000000 (spec §8: "do not use #000000
# extensively; near-black provides more visual richness").
BEAK_MAIN_FILL = "#101416"  # spec §8 deep black
BEAK_FACE_UPPER_FILL = "#202022"  # spec §8 upper-left face
BEAK_FACE_LOWER_FILL = "#332A25"  # spec §8 lower face
BEAK_HIGHLIGHT_FILL = "#C6C2BA"  # spec §8 reflective strip
BEAK_HIGHLIGHT_OPACITY = 0.68  # spec §8: opacity 60-75%

BEAK_FACE_UPPER_POINTS = [(135, 70), (415, 355), (300, 290), (200, 160)]
BEAK_FACE_LOWER_POINTS = [(144, 86), (470, 425), (350, 340), (220, 180)]
# Spec §8: narrow reflective strip running (146,74) -> (408,326), expressed
# as a thin quadrilateral (a stroke would read as a cartoon outline).
BEAK_HIGHLIGHT_POINTS = [(149, 71), (411, 323), (405, 329), (143, 77)]

# Spec §9: 10-16 irregular polygons inside the crown envelope, larger toward
# the rear (higher X). Clipped to CROWN_MASS_PATH so approximate/overshooting
# vertices never bleed past the crown's own silhouette. 14 facets, cycling
# through the 5-step gray ramp so no obvious repeated tiling emerges.
CROWN_GRAYS = (
    "#AAA9A6",  # highlight gray
    "#8C8B89",  # mid-light gray
    "#71716F",  # mid gray
    "#555654",  # dark gray
    "#3D403F",  # deep crown shadow
)

CROWN_FACETS = [
    # Front cluster (was a near-regular 2x3 quad grid -- flagged in review).
    # Jittered vertices and a triangle in the mix instead of six uniform
    # quads so it reads as irregular faceting, not tiling.
    ([(345, 235), (398, 207), (422, 251)], CROWN_GRAYS[1]),
    ([(398, 207), (452, 196), (447, 236), (422, 251)], CROWN_GRAYS[0]),
    ([(452, 196), (516, 209), (508, 243), (447, 236)], CROWN_GRAYS[2]),
    ([(345, 235), (422, 251), (435, 298), (378, 312)], CROWN_GRAYS[2]),
    ([(422, 251), (447, 236), (468, 283), (435, 298)], CROWN_GRAYS[0]),
    ([(447, 236), (508, 243), (520, 277), (468, 283)], CROWN_GRAYS[1]),
    ([(510, 245), (580, 225), (600, 260), (540, 275)], CROWN_GRAYS[2]),
    ([(580, 225), (650, 240), (670, 270), (600, 260)], CROWN_GRAYS[3]),
    ([(430, 300), (465, 290), (490, 330), (440, 345)], CROWN_GRAYS[2]),
    ([(465, 290), (515, 285), (535, 325), (490, 330)], CROWN_GRAYS[1]),
    ([(515, 285), (600, 260), (630, 300), (560, 320)], CROWN_GRAYS[2]),
    ([(630, 300), (700, 300), (710, 360), (650, 350)], CROWN_GRAYS[4]),
    ([(440, 345), (490, 330), (510, 370), (460, 380)], CROWN_GRAYS[3]),
    ([(600, 260), (670, 270), (700, 300), (630, 300)], CROWN_GRAYS[3]),
]

# Spec §10: restrained orange band between crown and eye, X 450-650 / Y
# 280-355, sweeping up over the eye -- 4 shapes, never displacing gray
# crown dominance (kept well inside the crown's own bounding box).
FOREHEAD_ORANGES = ("#F98400", "#F36D00", "#E9500D", "#C94719")

FOREHEAD_PATCHES = [
    ([(455, 300), (520, 282), (545, 315), (490, 330)], FOREHEAD_ORANGES[0]),
    ([(520, 282), (580, 285), (600, 320), (545, 315)], FOREHEAD_ORANGES[1]),
    ([(490, 330), (545, 315), (575, 345), (520, 352)], FOREHEAD_ORANGES[2]),
    ([(545, 315), (600, 320), (625, 350), (575, 345)], FOREHEAD_ORANGES[3]),
    # Sweeps on up over the eye's top edge (eye center 625,347, outer
    # radius ~50-54 -> top edge ~y293) rather than stopping short of it,
    # per spec §10 ("the patch should sweep upward over the eye").
    ([(590, 288), (655, 270), (678, 302), (632, 322)], FOREHEAD_ORANGES[1]),
]

# Spec §11: dark eye-stripe wedge, beak base -> beneath eye -> rear cheek,
# given verbatim, plus one secondary brown-black plane for depth.
EYE_STRIPE_MAIN_POINTS = [
    (425, 350),
    (570, 365),
    (675, 395),
    (750, 600),
    (700, 630),
    (610, 475),
    (470, 405),
]
EYE_STRIPE_MAIN_FILL = "#15191A"
EYE_STRIPE_SECONDARY_POINTS = [
    (470, 405),
    (610, 475),
    (700, 630),
    (660, 600),
    (590, 470),
    (490, 415),
]
EYE_STRIPE_SECONDARY_FILL = "#2A2421"

# Spec §12: four vector components, eye center (625, 347). The primary
# highlight (~25x18) must stay visible at 32px and collapse toward a single
# bright pixel at 16px -- verified visually via the harness, not unit tests.
EYE_CENTER = (625, 347)
EYE_OUTER_RADII = (50, 54)
EYE_OUTER_FILL = "#141719"
EYE_IRIS_CENTER = (625, 349)
EYE_IRIS_RADII = (37, 41)
EYE_IRIS_GRADIENT_CENTER_FILL = "#161A1C"
EYE_IRIS_GRADIENT_EDGE_FILL = "#050606"
EYE_HIGHLIGHT_PRIMARY_CENTER = (600, 316)
# Spec §12 literal size (~25x18). An earlier revision widened this to 16/12
# on a mistaken reading of a 32px pixel sample; re-sampled against the
# literal size, the highlight pixel comes in at luma ~113 against a
# surrounding eye of ~28-50 -- already clearly visible at 32px, so the
# widening wasn't needed and cost the highlight its "glint" read at 1024px
# (it started breaching the eye ring's upper-left outline instead).
EYE_HIGHLIGHT_PRIMARY_RADII = (12.5, 9)
EYE_HIGHLIGHT_FILL = "#F4F5F2"
EYE_HIGHLIGHT_SECONDARY_CENTER = (612, 330)
EYE_HIGHLIGHT_SECONDARY_RADII = (4, 4)
EYE_HIGHLIGHT_SECONDARY_OPACITY = 0.6

# Spec §13: one tapered filled Bézier shape (not a stroke), route (430,390)
# -> (510,410) -> (590,430), width 8-18px master, narrow at each end and
# widest at the middle. Spec §34 lists cheek-light below crown-base, but
# also explicitly allows "eye stripe / crown ordering adjustments" -- at
# that position the crown/forehead/eye-stripe masses drawn afterward cover
# all but a sliver of it. Emitted after eye-stripe/eye-stripe-secondary
# (and before the eye group, so it still reads as passing near/under the
# eye rather than over it) so the streak is actually visible where it
# separates cheek from gorget, per the acceptance intent rather than the
# literal document-order default.
CHEEK_SEPARATOR_PATH = (
    "M 430 393 Q 470 397, 510 417 Q 550 427, 590 433 "
    "Q 550 437, 510 424 Q 470 407, 430 401 Z"
)
CHEEK_SEPARATOR_FILL = "#F5EFE7"


# ---------------------------------------------------------------------------
# Palettes (spec §6 backgrounds, §7 colors, §23 dark-mode shifts). Dark
# differs from light only by these values -- _build_svg takes a palette
# dict and is otherwise variant-agnostic.
# ---------------------------------------------------------------------------


def _brighten(hex_color: str, factor: float) -> str:
    """Lighten a #RRGGBB color by `factor` (e.g. 1.04 == 4% brighter),
    clamped to 255 per channel. Spec §23: dark-mode orange "may be ~3-5%
    brighter" than its light counterpart."""
    hex_color = hex_color.lstrip("#")
    r, g, b = (int(hex_color[i : i + 2], 16) for i in (0, 2, 4))
    r, g, b = (min(255, round(c * factor)) for c in (r, g, b))
    return f"#{r:02X}{g:02X}{b:02X}"


_LIGHT_GORGET = "#FF8500"  # spec §7 primary orange

LIGHT_PALETTE = {
    "background_start": "#FBF7F0",  # spec §6 light gradient center
    "background_end": "#F2E9DD",  # spec §6 light gradient edge
    "silhouette_base": "#181819",  # spec §8 warm black
    "crown_mass": "#747471",  # spec §23 light gray midtone
    "gorget_mass": _LIGHT_GORGET,
    # spec §23's light cream chest (#F4EADF) is visually indistinguishable
    # from the light background (243,235,224 vs 244,234,223 at y=950),
    # leaving only a forbidden (§46) anti-aliased hairline outline instead
    # of the required shape/tonal separation. Use §19's chest base fill
    # instead, which has real contrast against the background.
    "chest_mass": "#EFE5D7",  # spec §19 chest base fill
    "side_body_mass": "#CE6A16",  # spec §20 base
}

DARK_PALETTE = {
    "background_start": "#36444F",  # spec §6 dark gradient example
    "background_end": "#27323B",  # spec §6 dark gradient example
    "silhouette_base": "#181819",  # bird stays mostly identical (spec §23)
    "crown_mass": "#82827E",  # spec §23 dark gray midtone
    "gorget_mass": _brighten(_LIGHT_GORGET, 1.04),  # spec §23: ~3-5% brighter
    "chest_mass": "#F6EEE3",  # spec §23 dark cream chest
    "side_body_mass": "#CE6A16",  # unchanged: spec gives no dark variant
}

PALETTES = {"light": LIGHT_PALETTE, "dark": DARK_PALETTE}


# ---------------------------------------------------------------------------
# Gorget feather system (#63, spec §14-18): the flat gorget block becomes one
# base under-shape (already given above as GORGET_MASS_PATH/§14) plus ~5
# overlapping rows of a seeded, jittered rounded-shield primitive (§15-18).
# All of it is deterministic data computed once at import time from
# FEATHER_SEED -- same inputs, same SVG, satisfying the "regenerating with
# unchanged parameters is byte-stable" acceptance criterion for free, the
# same way CROWN_FACETS above is a literal list rather than randomized per
# call. Row counts/sizes/colors are parameters (ROW_SPECS) so taste feedback
# lands as data changes, not a rewrite -- and the later small/micro slice
# (#65) reuses these same parameters at reduced counts (out of scope here).
# ---------------------------------------------------------------------------

# Spec §15: the rounded-shield feather primitive, given verbatim as an
# ordered point list -- M, then three cubic Bezier segments' (c1, c2, end)
# triples. On-curve endpoints are indices 0/3/6/9 (9 repeats 0, closing the
# shape); the rest are off-curve control points that curvature jitter
# perturbs independently, so no two instances share an identical bulge.
FEATHER_BASE_POINTS = [
    (0, 0), (14, -7), (32, -7), (46, 0),
    (46, 22), (36, 36), (23, 44),
    (10, 36), (0, 22), (0, 0),
]
FEATHER_ENDPOINT_INDICES = {0, 3, 6, 9}
FEATHER_BASE_WIDTH = 46.0
FEATHER_BASE_HEIGHT = 51.0  # primitive's own bbox: y runs -7..44

# Spec §7/§18 orange/gorget ramp, warmest (yellow-orange) to coolest (gorget
# shadow). Each feather's fill is an index into this ramp, chosen by its
# left-to-right column position within its row (§18: warm left, red/deep-red
# right) -- never a random rainbow pick. §7's "primary orange" (#FF8500) is
# deliberately left out: it's also LIGHT_PALETTE's gorget-base fill (and
# DARK_PALETTE's is only ~4% brighter, #FF8A00), so a feather using it would
# be invisible against its own base -- violating §17's "separation comes
# entirely from color contrast" (review on #63/PR #87 caught 6 such
# feathers). GorgetFeatherFillsAreDistinctFromBaseTest guards this.
GORGET_FEATHER_RAMP = (
    "#FFA000",  # 0 yellow-orange
    "#F46B00",  # 1 deep orange
    "#F4470D",  # 2 vermilion
    "#D93A18",  # 3 red-orange
    "#B62E22",  # 4 deep red
    "#8D2A23",  # 5 gorget shadow
)

# Spec §16-17: five rows, ids following spec §34's layer-order names (Row 1
# "near chin" -> gorget-top-row ... Row 5 "overlaps the chest" ->
# gorget-bottom-row). color_index_range picks a sub-span of
# GORGET_FEATHER_RAMP per §16's per-row color notes (Row 1 "darkest reds
# concentrated near right side", Row 4 "more gold/orange toward left",
# Row 3 "visually dominant" gets the full ramp).
#
# y_center/width_range/height_range are hand-placed (not derived from a
# single overlap constant) so the cascade can actually reach the gorget
# envelope's own extent -- §16's literal per-row figures (e.g. Row 5
# "Height: 70-90") only produce ~200px of legitimate row-to-row overlap
# stacked from a chin-height start, well short of the ~390px the base
# envelope (GORGET_MASS_PATH, §14) actually spans; review on #63/PR #87
# measured the result as 50.7% feather coverage in the lower gorget with
# 0% in the bottom two 50px bands. Row 4/5 are widened beyond §16's literal
# numbers (still "the largest shapes", per §16's own steer) so the cascade
# reaches the chest overlap §16 calls for. The feather group is clipped to
# GORGET_MASS_PATH (same pattern as crown-clip, #62) so any feather
# overshooting the envelope is trimmed to the mass's own silhouette, never
# bleeding into the background or beak -- GorgetFeathersClippedToEnvelopeTest
# guards this structurally.
#
# x_ranges are budgeted against the envelope's measured x-span at each row's
# actual y-extent (GORGET_MASS_PATH's interior narrows sharply toward the
# chin: right edge ~x574 at y=400, ~x660 at y=450, ~x785 at y=650), minus
# each row's max feather half-width and position jitter. Round-2 review on
# PR #87 measured the previous ranges (top row reaching x=760 at y~420,
# where the envelope ends near x~615) as 7 of 37 feathers clipped to ZERO
# rendered pixels and 3 more nearly gone -- only ~27 visible against §15's
# 35-45, with §16's per-row counts missed and ramp step #8D2A23 (the "darkest
# reds near right side" of Row 1) occupying no pixels at all. Every emitted
# feather now renders >=~80% of its own area inside the envelope, measured
# by GorgetFeatherVisibilityTest against the same generated geometry.
# Measured aggregate row-to-row overlap (row_y_extent) lands mid-band in
# §17's 28-38%: 32.2/28.9/33.7/31.5%.
ROW_SPECS = [
    {
        "id": "gorget-top-row", "count": 6, "width_range": (55, 75), "height_range": (45, 60),
        "x_range": (295, 520), "color_index_range": (1, 5), "y_center": 421,
    },
    {
        "id": "gorget-row-2", "count": 8, "width_range": (65, 85), "height_range": (52, 68),
        "x_range": (270, 610), "color_index_range": (0, 4), "y_center": 470,
    },
    {
        "id": "gorget-row-3", "count": 9, "width_range": (75, 96), "height_range": (60, 78),
        "x_range": (270, 645), "color_index_range": (0, 5), "y_center": 525,
    },
    {
        "id": "gorget-row-4", "count": 8, "width_range": (88, 115), "height_range": (75, 100),
        "x_range": (280, 665), "color_index_range": (0, 3), "y_center": 592,
    },
    {
        "id": "gorget-bottom-row", "count": 6, "width_range": (100, 140), "height_range": (100, 135),
        "x_range": (320, 610), "color_index_range": (0, 2), "y_center": 672,
    },
]
ROW_WIDTH_RANGES = [row["width_range"] for row in ROW_SPECS]
ROW_HEIGHT_RANGES = [row["height_range"] for row in ROW_SPECS]

FEATHER_SEED = 20260808  # fixed -- regenerating with unchanged parameters must be byte-stable
ROTATION_JITTER_DEG = 14.0  # spec §15 rotation variation
CURVATURE_JITTER = 0.15  # spec §15 curvature variation (of a control point's own scaled offset)
POSITION_JITTER = 0.06  # column-position jitter, as a fraction of the row's x-span
ROW_Y_JITTER = 0.08  # per-feather y jitter, as a fraction of the row's avg height


def _row_y_centers() -> list:
    return [row["y_center"] for row in ROW_SPECS]


def _rotate_point(point, angle_deg: float, origin) -> tuple:
    ox, oy = origin
    x, y = point
    theta = math.radians(angle_deg)
    dx, dy = x - ox, y - oy
    cos_t, sin_t = math.cos(theta), math.sin(theta)
    return (ox + dx * cos_t - dy * sin_t, oy + dx * sin_t + dy * cos_t)


def _feather_path_d(rng: random.Random, width: float, height: float, rotation_deg: float, cx: float, cy: float) -> str:
    """One feather instance: scale the base primitive to width/height,
    jitter its control points (curvature), rotate, then translate its
    centroid to (cx, cy). Coordinates are baked into the returned `d`
    string (not left as a `transform` attribute) so every instance's own
    path text differs -- no two feathers share geometry, satisfying "no
    visible cloning" (spec §15) even under XML-level inspection."""
    sx, sy = width / FEATHER_BASE_WIDTH, height / FEATHER_BASE_HEIGHT
    points = []
    for index, (x, y) in enumerate(FEATHER_BASE_POINTS):
        px, py = x * sx, y * sy
        if index not in FEATHER_ENDPOINT_INDICES:
            px *= 1 + rng.uniform(-CURVATURE_JITTER, CURVATURE_JITTER)
            py *= 1 + rng.uniform(-CURVATURE_JITTER, CURVATURE_JITTER)
        points.append((px, py))

    centroid = (sum(p[0] for p in points) / len(points), sum(p[1] for p in points) / len(points))
    rotated = [_rotate_point(p, rotation_deg, centroid) for p in points]
    dx, dy = cx - centroid[0], cy - centroid[1]
    placed = [(round(px + dx, 2), round(py + dy, 2)) for px, py in rotated]

    m = placed[0]
    c1a, c1b, e1 = placed[1:4]
    c2a, c2b, e2 = placed[4:7]
    c3a, c3b, e3 = placed[7:10]
    d = (
        f"M {_pt(m)} "
        f"C {_pt(c1a)}, {_pt(c1b)}, {_pt(e1)} "
        f"C {_pt(c2a)}, {_pt(c2b)}, {_pt(e2)} "
        f"C {_pt(c3a)}, {_pt(c3b)}, {_pt(e3)} Z"
    )
    y_min = min(p[1] for p in placed)
    y_max = max(p[1] for p in placed)
    return d, (y_min, y_max)


def _build_gorget_feathers() -> list:
    """One deterministic pass (seeded by FEATHER_SEED) over ROW_SPECS,
    producing {"id", "feathers": [{"d", "fill", "center", "y_extent"}]} per
    row, in Row-1..Row-5 (top-to-bottom, chin-to-chest) order. `y_extent`
    (the feather's own actual rendered (y_min, y_max), post-jitter/rotation)
    lets tests measure real row-to-row overlap from the generated geometry
    itself rather than recomputing the placement formula."""
    rng = random.Random(FEATHER_SEED)
    y_centers = _row_y_centers()
    rows = []
    for row_spec, y_center in zip(ROW_SPECS, y_centers):
        count = row_spec["count"]
        x0, x1 = row_spec["x_range"]
        w0, w1 = row_spec["width_range"]
        h0, h1 = row_spec["height_range"]
        c0, c1 = row_spec["color_index_range"]
        avg_h = (h0 + h1) / 2
        feathers = []
        for i in range(count):
            # Evenly spaced base column position, then jittered so the row
            # doesn't read as a uniform grid (spec §15: "no visible
            # cloning" applies to placement, not just shape).
            col_fraction = i / (count - 1) if count > 1 else 0.5
            cx = x0 + col_fraction * (x1 - x0) + rng.uniform(-POSITION_JITTER, POSITION_JITTER) * (x1 - x0)
            cy = y_center + rng.uniform(-ROW_Y_JITTER, ROW_Y_JITTER) * avg_h
            width = rng.uniform(w0, w1)
            height = rng.uniform(h0, h1)
            rotation = rng.uniform(-ROTATION_JITTER_DEG, ROTATION_JITTER_DEG)
            color_index = round(c0 + col_fraction * (c1 - c0))
            fill = GORGET_FEATHER_RAMP[color_index]
            d, y_extent = _feather_path_d(rng, width, height, rotation, cx, cy)
            feathers.append({"d": d, "fill": fill, "center": (cx, cy), "y_extent": y_extent})
        rows.append({"id": row_spec["id"], "feathers": feathers})
    return rows


# Computed once at import time -- pure function of FEATHER_SEED/ROW_SPECS,
# so every generate() call (light and dark both) sees the identical rows.
GORGET_FEATHER_ROWS = _build_gorget_feathers()


def row_y_extent(row: dict) -> tuple:
    """A row's actual aggregate (y_min, y_max) across all its generated
    feathers -- real geometry, not the row_spec's nominal center/height."""
    y_mins = [f["y_extent"][0] for f in row["feathers"]]
    y_maxes = [f["y_extent"][1] for f in row["feathers"]]
    return (min(y_mins), max(y_maxes))


def _gorget_feathers_svg() -> str:
    """Document order follows spec §34's z-stack: gorget-bottom-row (Row 5,
    nearest the chest) painted first/bottommost, up through gorget-top-row
    (Row 1, nearest the chin) painted last/topmost -- the reverse of
    ROW_SPECS's chin-to-chest order, so each row overlaps down into the one
    below it (spec §17)."""
    groups = []
    for row in reversed(GORGET_FEATHER_ROWS):
        paths = [
            f'        <path id="{row["id"]}-{index:02d}" d="{feather["d"]}" fill="{feather["fill"]}"/>'
            for index, feather in enumerate(row["feathers"], start=1)
        ]
        groups.append(f'      <g id="{row["id"]}">\n' + "\n".join(paths) + "\n      </g>")
    return "\n".join(groups)


def _crown_facets_svg() -> str:
    facets = []
    for index, (points, fill) in enumerate(CROWN_FACETS, start=1):
        facets.append(
            f'      <polygon id="crown-facet-{index:02d}" points="{_points_attr(points)}" fill="{fill}"/>'
        )
    return "\n".join(facets)


def _forehead_patches_svg() -> str:
    patches = []
    for index, (points, fill) in enumerate(FOREHEAD_PATCHES, start=1):
        patches.append(
            f'      <polygon id="forehead-{index:02d}" points="{_points_attr(points)}" fill="{fill}"/>'
        )
    return "\n".join(patches)


def _build_svg(palette: dict) -> str:
    """Emit one self-contained master SVG from a palette dict. Geometry is
    identical for every call; only `palette` values vary -- this is the
    single code path both light and dark run through (spec: dark differs
    from light only via palette data, never separate drawing code). Head
    identity geometry (#62) does not vary by palette at all -- the spec
    gives no light/dark split for beak/eye/stripe/forehead/cheek."""
    silhouette_d = build_bird_silhouette_path()
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024">
  <defs>
    <radialGradient id="background-gradient" cx="50%" cy="50%" r="75%">
      <stop offset="0%" stop-color="{palette['background_start']}"/>
      <stop offset="100%" stop-color="{palette['background_end']}"/>
    </radialGradient>
    <radialGradient id="eye-iris-gradient" cx="35%" cy="35%" r="70%">
      <stop offset="0%" stop-color="{EYE_IRIS_GRADIENT_CENTER_FILL}"/>
      <stop offset="100%" stop-color="{EYE_IRIS_GRADIENT_EDGE_FILL}"/>
    </radialGradient>
    <clipPath id="beak-clip">
      <path d="{BEAK_ENVELOPE_PATH}"/>
    </clipPath>
    <clipPath id="crown-clip">
      <path d="{CROWN_MASS_PATH}"/>
    </clipPath>
    <clipPath id="gorget-clip">
      <path d="{GORGET_MASS_PATH}"/>
    </clipPath>
    <!-- Preview-only mask (spec §3): the master itself stays full-square. -->
    <clipPath id="preview-rounded-square">
      <rect x="0" y="0" width="1024" height="1024" rx="220" ry="220"/>
    </clipPath>
  </defs>
  <g id="icon">
    <rect id="background" x="0" y="0" width="1024" height="1024" fill="url(#background-gradient)"/>
    <g id="bird">
      <path id="bird-silhouette" d="{silhouette_d}" fill="{palette['silhouette_base']}"/>
      <polygon id="chest-base" points="{_points_attr(CHEST_MASS_POINTS)}" fill="{palette['chest_mass']}"/>
      <polygon id="side-body-base" points="{_points_attr(SIDE_BODY_MASS_POINTS)}" fill="{palette['side_body_mass']}"/>
      <path id="gorget-base" d="{GORGET_MASS_PATH}" fill="{palette['gorget_mass']}"/>
      <g id="gorget-feathers" clip-path="url(#gorget-clip)">
{_gorget_feathers_svg()}
      </g>
      <g id="head">
        <path id="crown-base" d="{CROWN_MASS_PATH}" fill="{palette['crown_mass']}"/>
        <g id="crown-facets" clip-path="url(#crown-clip)">
{_crown_facets_svg()}
        </g>
        <g id="forehead">
{_forehead_patches_svg()}
        </g>
        <polygon id="eye-stripe" points="{_points_attr(EYE_STRIPE_MAIN_POINTS)}" fill="{EYE_STRIPE_MAIN_FILL}"/>
        <polygon id="eye-stripe-secondary" points="{_points_attr(EYE_STRIPE_SECONDARY_POINTS)}" fill="{EYE_STRIPE_SECONDARY_FILL}"/>
        <path id="cheek-separator" d="{CHEEK_SEPARATOR_PATH}" fill="{CHEEK_SEPARATOR_FILL}"/>
        <g id="eye">
          <ellipse id="eye-outer" cx="{EYE_CENTER[0]}" cy="{EYE_CENTER[1]}" rx="{EYE_OUTER_RADII[0]}" ry="{EYE_OUTER_RADII[1]}" fill="{EYE_OUTER_FILL}"/>
          <ellipse id="eye-iris" cx="{EYE_IRIS_CENTER[0]}" cy="{EYE_IRIS_CENTER[1]}" rx="{EYE_IRIS_RADII[0]}" ry="{EYE_IRIS_RADII[1]}" fill="url(#eye-iris-gradient)"/>
          <ellipse id="eye-highlight-primary" cx="{EYE_HIGHLIGHT_PRIMARY_CENTER[0]}" cy="{EYE_HIGHLIGHT_PRIMARY_CENTER[1]}" rx="{EYE_HIGHLIGHT_PRIMARY_RADII[0]}" ry="{EYE_HIGHLIGHT_PRIMARY_RADII[1]}" fill="{EYE_HIGHLIGHT_FILL}"/>
          <ellipse id="eye-highlight-secondary" cx="{EYE_HIGHLIGHT_SECONDARY_CENTER[0]}" cy="{EYE_HIGHLIGHT_SECONDARY_CENTER[1]}" rx="{EYE_HIGHLIGHT_SECONDARY_RADII[0]}" ry="{EYE_HIGHLIGHT_SECONDARY_RADII[1]}" fill="{EYE_HIGHLIGHT_FILL}" opacity="{EYE_HIGHLIGHT_SECONDARY_OPACITY}"/>
        </g>
      </g>
      <g id="beak" clip-path="url(#beak-clip)">
        <path id="beak-main" d="{BEAK_ENVELOPE_PATH}" fill="{BEAK_MAIN_FILL}"/>
        <polygon id="beak-face-upper" points="{_points_attr(BEAK_FACE_UPPER_POINTS)}" fill="{BEAK_FACE_UPPER_FILL}"/>
        <polygon id="beak-face-lower" points="{_points_attr(BEAK_FACE_LOWER_POINTS)}" fill="{BEAK_FACE_LOWER_FILL}"/>
        <polygon id="beak-highlight" points="{_points_attr(BEAK_HIGHLIGHT_POINTS)}" fill="{BEAK_HIGHLIGHT_FILL}" opacity="{BEAK_HIGHLIGHT_OPACITY}"/>
      </g>
    </g>
  </g>
</svg>
"""


# Stable, documented output paths (spec §40 naming).
OUTPUT_NAMES = {
    "light": "hummingbird-icon-master-light.svg",
    "dark": "hummingbird-icon-master-dark.svg",
}


def generate(out_dir: Path = DEFAULT_OUT_DIR) -> dict:
    """Emit both master SVGs into out_dir. Returns {variant: Path}."""
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    paths = {}
    for variant, palette in PALETTES.items():
        out_path = out_dir / OUTPUT_NAMES[variant]
        out_path.write_text(_build_svg(palette))
        paths[variant] = out_path
    return paths


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(
        prog="icon_generator.py",
        description="Generate the master light/dark hummingbird icon SVGs (#61).",
    )
    parser.add_argument(
        "--out-dir",
        type=Path,
        default=DEFAULT_OUT_DIR,
        help=f"output directory (default: {DEFAULT_OUT_DIR})",
    )
    args = parser.parse_args(argv)
    paths = generate(args.out_dir)
    for variant, path in sorted(paths.items()):
        print(f"{variant} -> {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
