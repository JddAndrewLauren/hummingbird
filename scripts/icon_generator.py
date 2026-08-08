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


def _centroid(points) -> tuple:
    return (sum(p[0] for p in points) / len(points), sum(p[1] for p in points) / len(points))


def _lerp(a, b, t) -> tuple:
    return (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t)


# ---------------------------------------------------------------------------
# Generic simple-polygon triangulation (#64 round 2): a single off-center
# apex fanned to every envelope vertex -- the original approach here --
# only exactly tiles a *star-shaped* polygon (one where some interior point
# can "see" every edge in a straight line). CHEST_MASS_POINTS is concave
# (its top boundary sags down through (530,775)/(395,770) between the two
# sharp corners at (610,710) and (215,640), pinching the polygon into two
# lobes at the top), so no single apex sees every edge: review on PR #89
# measured 4 of 11 fan facets at <=1% actual visible area after the
# chest-clip trimmed away the part of each triangle that fell outside the
# real (concave) silhouette, and the one facet spanning the polygon's
# 460px-long bottom edge came out roughly double spec §19's 100-250px
# facet-size band.
#
# Ear-clip triangulation (standard algorithm for simple polygons, concave
# or convex, no interior Steiner points) sidesteps the star-shaped
# requirement entirely: every triangle it emits is *exactly* a piece of
# the source polygon, so 100% of each triangle's area is guaranteed inside
# the envelope by construction -- not just empirically likely. `_ear_clip`
# repeatedly finds a convex vertex whose triangle with its two neighbors
# contains no other remaining vertex (a valid "ear"), clips it off, and
# recurses; any simple polygon triangulates fully this way.
#
# Ear-clipping alone gives one triangle per interior diagonal (n-2 for an
# n-vertex polygon -- 9 for CHEST_MASS_POINTS's 11 vertices), which is
# below spec's 10-18 facet count and leaves some ears far outside the
# 100-250px size band (the polygon's long, sweeping edges produce long,
# thin ears). `_subdivide_to_size` then repeatedly bisects the
# largest-area triangle that still exceeds `max_dim` at its longest edge's
# midpoint -- a split that, like the ear itself, stays *exactly* inside
# its parent triangle (an affine combination of the parent's own
# vertices), so the 100%-inside guarantee carries through subdivision.
def _cross(o, a, b) -> float:
    return (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])


def _point_in_triangle(p, a, b, c) -> bool:
    d1, d2, d3 = _cross(a, b, p), _cross(b, c, p), _cross(c, a, p)
    has_neg = d1 < 0 or d2 < 0 or d3 < 0
    has_pos = d1 > 0 or d2 > 0 or d3 > 0
    return not (has_neg and has_pos)


def _signed_area(polygon) -> float:
    n = len(polygon)
    return sum(polygon[i][0] * polygon[(i + 1) % n][1] - polygon[(i + 1) % n][0] * polygon[i][1] for i in range(n)) / 2


def _ear_clip(polygon: list) -> list:
    """Triangulate a simple (non-self-intersecting) polygon, concave or
    convex, into a list of (a, b, c) triangles that exactly tile it --
    n-2 triangles for an n-vertex polygon, zero gaps, zero overlap."""
    points = list(polygon) if _signed_area(polygon) > 0 else list(reversed(polygon))
    remaining = list(range(len(points)))
    triangles = []
    while len(remaining) > 3:
        n = len(remaining)
        for i in range(n):
            i0, i1, i2 = remaining[(i - 1) % n], remaining[i], remaining[(i + 1) % n]
            a, b, c = points[i0], points[i1], points[i2]
            if _cross(a, b, c) <= 0:  # reflex vertex, not a candidate ear
                continue
            if any(_point_in_triangle(points[j], a, b, c) for j in remaining if j not in (i0, i1, i2)):
                continue  # another vertex sits inside this ear -- not valid yet
            triangles.append((a, b, c))
            remaining.pop(i)
            break
        else:
            break  # numerically degenerate input; stop rather than loop forever
    if len(remaining) == 3:
        triangles.append(tuple(points[i] for i in remaining))
    return triangles


def _triangle_bbox_max_dim(triangle) -> float:
    xs = [p[0] for p in triangle]
    ys = [p[1] for p in triangle]
    return max(max(xs) - min(xs), max(ys) - min(ys))


def _triangle_area(triangle) -> float:
    (x1, y1), (x2, y2), (x3, y3) = triangle
    return abs(x1 * (y2 - y3) + x2 * (y3 - y1) + x3 * (y1 - y2)) / 2


def _split_longest_edge(triangle) -> tuple:
    a, b, c = triangle
    edges = ((a, b, c), (b, c, a), (c, a, b))  # (p, q, opposite vertex)
    p, q, opposite = max(edges, key=lambda edge: math.dist(edge[0], edge[1]))
    midpoint = _lerp(p, q, 0.5)
    return (p, midpoint, opposite), (midpoint, q, opposite)


def _subdivide_to_size(triangles: list, max_dim: float, max_count: int) -> list:
    """Bisect the largest-area over-sized triangle at its longest edge's
    midpoint, repeatedly, until every triangle's bounding-box longer side
    is within `max_dim` or `max_count` is reached (a hard cap protects
    spec §35's total path budget; the polygon's sharpest corners can stay
    slightly over `max_dim` rather than blow the count past it)."""
    result = list(triangles)
    while True:
        oversized = [t for t in result if _triangle_bbox_max_dim(t) > max_dim]
        if not oversized or len(result) >= max_count:
            return result
        target = max(oversized, key=_triangle_area)
        result.remove(target)
        result.extend(_split_longest_edge(target))


# ---------------------------------------------------------------------------
# Chest + side-body facets (#64, spec §19-20): the flat chest-base/
# side-body-base masses gain a low-poly facet overlay. Both are derived
# from the same envelope point lists used for the base fills above, so the
# facets always tile that exact silhouette with no separately-maintained
# coordinate set to drift out of sync -- and (like crown/gorget) the facet
# group is also clipped to its own envelope as a structural safety net
# (belt-and-braces on top of the exact-tiling guarantee above).
#
# Chest (spec §19): ear-clip CHEST_MASS_POINTS, then subdivide down toward
# spec's ~100-250px facet-size band (see `_subdivide_to_size` above) up to
# an 18-facet hard cap -- 18 facets against the gorget's 37 feathers is
# still the "dramatically simpler" contrast §19 calls for, and every facet
# is *exactly* inside the chest silhouette by construction (no
# fan-from-an-off-center-apex clipping loss). No outlines, no per-feather
# rounding, just flat planes.
CHEST_FACET_RAMP = ("#FFF9F0", "#F5ECDF", "#EADBC8", "#DDD0BF", "#CDBDA8")
CHEST_FACET_MAX_DIM = 260  # spec §19's 100-250px band, with a small allowance for sharp-corner ears
CHEST_FACET_MAX_COUNT = 18  # spec §19's upper facet-count bound


def _build_chest_facets(envelope: list = None, max_dim: float = CHEST_FACET_MAX_DIM, max_count: int = CHEST_FACET_MAX_COUNT) -> list:
    if envelope is None:
        envelope = CHEST_MASS_POINTS
    triangles = _subdivide_to_size(_ear_clip(envelope), max_dim, max_count)
    ramp = CHEST_FACET_RAMP
    return [(list(triangle), ramp[index % len(ramp)]) for index, triangle in enumerate(triangles)]


CHEST_FACETS = _build_chest_facets()

# Side-body (spec §20): "long polygon planes rather than small feather
# shapes." SIDE_BODY_MASS_POINTS already traces an elongated top-to-bottom
# strip -- an inner/left edge (its own points 0, 7, 6, 5, top to bottom)
# and an outer/right edge (points 1-4, following the outer silhouette's
# own boundary chain) -- derived here rather than re-typed as separate
# literals, so the two chains can never drift out of sync with the base
# envelope they're read from. Band the strip into thirds top-to-bottom,
# then each band into three further sub-quads along its own length,
# giving 9 wide horizontal-ish planes (spec §35's "Body facets 8-12")
# that read as long structural mass rather than rounded feathers --
# distinct from the gorget's rounded-shield primitive per §28's "two
# different geometric languages." Unlike the chest envelope, this strip
# is convex enough along its own length that banding it directly (rather
# than ear-clipping) already keeps every quad exactly inside the
# envelope; `ChestAndSideBodyFacetsTest`'s geometric visibility check
# confirms that rather than assuming it.
SIDE_BODY_INNER_CHAIN = [SIDE_BODY_MASS_POINTS[0], *reversed(SIDE_BODY_MASS_POINTS[5:8])]
SIDE_BODY_OUTER_CHAIN = SIDE_BODY_MASS_POINTS[1:5]
SIDE_BODY_FACET_RAMP = ("#E47A16", "#D46A12", "#B85B18", "#A44E1B", "#8E451E")
SIDE_BODY_FACET_SUBDIVISIONS = 3


def _build_side_body_facets(subdivisions: int = SIDE_BODY_FACET_SUBDIVISIONS) -> list:
    if subdivisions <= 0:
        return []
    ramp = SIDE_BODY_FACET_RAMP
    sub = subdivisions
    facets = []
    index = 0
    for band in range(len(SIDE_BODY_INNER_CHAIN) - 1):
        inner_a, inner_b = SIDE_BODY_INNER_CHAIN[band], SIDE_BODY_INNER_CHAIN[band + 1]
        outer_a, outer_b = SIDE_BODY_OUTER_CHAIN[band], SIDE_BODY_OUTER_CHAIN[band + 1]
        for s in range(sub):
            t0, t1 = s / sub, (s + 1) / sub
            quad = [
                _lerp(inner_a, inner_b, t0),
                _lerp(outer_a, outer_b, t0),
                _lerp(outer_a, outer_b, t1),
                _lerp(inner_a, inner_b, t1),
            ]
            facets.append((quad, ramp[index % len(ramp)]))
            index += 1
    return facets


SIDE_BODY_FACETS = _build_side_body_facets()


def _chest_facets_svg(facets: list = None) -> str:
    if facets is None:
        facets = CHEST_FACETS
    out = []
    for index, (points, fill) in enumerate(facets, start=1):
        out.append(
            f'      <polygon id="chest-facet-{index:02d}" points="{_points_attr(points)}" fill="{fill}"/>'
        )
    return "\n".join(out)


def _side_body_facets_svg(facets: list = None) -> str:
    if facets is None:
        facets = SIDE_BODY_FACETS
    out = []
    for index, (points, fill) in enumerate(facets, start=1):
        out.append(
            f'      <polygon id="side-body-facet-{index:02d}" points="{_points_attr(points)}" fill="{fill}"/>'
        )
    return "\n".join(out)


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


def _build_gorget_feathers(row_specs: list = None, seed: int = FEATHER_SEED) -> list:
    """One deterministic pass (seeded by `seed`) over `row_specs` (defaults
    to the master's ROW_SPECS), producing {"id", "feathers": [{"d", "fill",
    "center", "y_extent"}]} per row, in Row-1..Row-5 (top-to-bottom,
    chin-to-chest) order. `y_extent` (the feather's own actual rendered
    (y_min, y_max), post-jitter/rotation) lets tests measure real row-to-row
    overlap from the generated geometry itself rather than recomputing the
    placement formula. The small/micro profiles (#65) pass their own
    reduced-count row_specs and a distinct fixed seed through this same
    function -- never a separately written feather-placement pass."""
    if row_specs is None:
        row_specs = ROW_SPECS
    rng = random.Random(seed)
    y_centers = [row["y_center"] for row in row_specs]
    rows = []
    for row_spec, y_center in zip(row_specs, y_centers):
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


def _gorget_feathers_svg(rows: list = None) -> str:
    """Document order follows spec §34's z-stack: gorget-bottom-row (Row 5,
    nearest the chest) painted first/bottommost, up through gorget-top-row
    (Row 1, nearest the chin) painted last/topmost -- the reverse of
    ROW_SPECS's chin-to-chest order, so each row overlaps down into the one
    below it (spec §17). `rows` defaults to the master's GORGET_FEATHER_ROWS;
    the small/micro profiles (#65) pass their own reduced-count rows through
    the same rendering path."""
    if rows is None:
        rows = GORGET_FEATHER_ROWS
    groups = []
    for row in reversed(rows):
        paths = [
            f'        <path id="{row["id"]}-{index:02d}" d="{feather["d"]}" fill="{feather["fill"]}"/>'
            for index, feather in enumerate(row["feathers"], start=1)
        ]
        groups.append(f'      <g id="{row["id"]}">\n' + "\n".join(paths) + "\n      </g>")
    return "\n".join(groups)


def _crown_facets_svg(facets: list = None) -> str:
    """`facets` defaults to the master's full CROWN_FACETS; the small/micro
    profiles (#65) pass a reduced subset of that same literal list through
    the same rendering path -- never a separately drawn facet set."""
    if facets is None:
        facets = CROWN_FACETS
    out = []
    for index, (points, fill) in enumerate(facets, start=1):
        out.append(
            f'      <polygon id="crown-facet-{index:02d}" points="{_points_attr(points)}" fill="{fill}"/>'
        )
    return "\n".join(out)


def _forehead_patches_svg() -> str:
    patches = []
    for index, (points, fill) in enumerate(FOREHEAD_PATCHES, start=1):
        patches.append(
            f'      <polygon id="forehead-{index:02d}" points="{_points_attr(points)}" fill="{fill}"/>'
        )
    return "\n".join(patches)


def _forehead_group_svg(include: bool) -> str:
    """Spec §24: the forehead patch isn't in either small/micro shape
    budget list. Small keeps it (unlisted items stay unless the budget
    calls for their removal); micro omits it -- consistent with §38's
    fidelity-priority ordering, where finer head-identity detail is the
    first to go under aggressive simplification."""
    if not include:
        return ""
    return f'\n        <g id="forehead">\n{_forehead_patches_svg()}\n        </g>'


def _cheek_separator_svg(include: bool) -> str:
    """Spec §24: like the forehead patch, the cheek separator isn't in
    either small/micro shape budget list. Small keeps it; micro omits it."""
    if not include:
        return ""
    return f'\n        <path id="cheek-separator" d="{CHEEK_SEPARATOR_PATH}" fill="{CHEEK_SEPARATOR_FILL}"/>'


def _beak_svg(detail: str) -> str:
    """Spec §24: master/small keep the full 4-shape beak (main + two facet
    planes + the single reflective highlight strip -- "single beak
    highlight" is already true of the master's own single `beak-highlight`
    element, so small needs no reduction here). Micro collapses to "one
    beak shape" -- the envelope itself, flat-filled, no facets or
    highlight."""
    if detail == "single":
        return f'        <path id="beak-main" d="{BEAK_ENVELOPE_PATH}" fill="{BEAK_MAIN_FILL}"/>'
    return (
        f'        <path id="beak-main" d="{BEAK_ENVELOPE_PATH}" fill="{BEAK_MAIN_FILL}"/>\n'
        f'        <polygon id="beak-face-upper" points="{_points_attr(BEAK_FACE_UPPER_POINTS)}" fill="{BEAK_FACE_UPPER_FILL}"/>\n'
        f'        <polygon id="beak-face-lower" points="{_points_attr(BEAK_FACE_LOWER_POINTS)}" fill="{BEAK_FACE_LOWER_FILL}"/>\n'
        f'        <polygon id="beak-highlight" points="{_points_attr(BEAK_HIGHLIGHT_POINTS)}" fill="{BEAK_HIGHLIGHT_FILL}" opacity="{BEAK_HIGHLIGHT_OPACITY}"/>'
    )


def _eye_svg(detail: str) -> str:
    """Spec §24: master keeps all four pieces (ring, gradient iris, two
    highlights). Small is "simplified" -- drops the secondary glint, keeps
    the ring/iris/primary highlight. Micro is "one black eye, one white eye
    highlight" -- one flat-filled ring (no separate iris) plus the primary
    highlight only."""
    if detail == "micro":
        return (
            f'          <ellipse id="eye-outer" cx="{EYE_CENTER[0]}" cy="{EYE_CENTER[1]}" rx="{EYE_OUTER_RADII[0]}" ry="{EYE_OUTER_RADII[1]}" fill="{EYE_OUTER_FILL}"/>\n'
            f'          <ellipse id="eye-highlight-primary" cx="{EYE_HIGHLIGHT_PRIMARY_CENTER[0]}" cy="{EYE_HIGHLIGHT_PRIMARY_CENTER[1]}" rx="{EYE_HIGHLIGHT_PRIMARY_RADII[0]}" ry="{EYE_HIGHLIGHT_PRIMARY_RADII[1]}" fill="{EYE_HIGHLIGHT_FILL}"/>'
        )
    base = (
        f'          <ellipse id="eye-outer" cx="{EYE_CENTER[0]}" cy="{EYE_CENTER[1]}" rx="{EYE_OUTER_RADII[0]}" ry="{EYE_OUTER_RADII[1]}" fill="{EYE_OUTER_FILL}"/>\n'
        f'          <ellipse id="eye-iris" cx="{EYE_IRIS_CENTER[0]}" cy="{EYE_IRIS_CENTER[1]}" rx="{EYE_IRIS_RADII[0]}" ry="{EYE_IRIS_RADII[1]}" fill="url(#eye-iris-gradient)"/>\n'
        f'          <ellipse id="eye-highlight-primary" cx="{EYE_HIGHLIGHT_PRIMARY_CENTER[0]}" cy="{EYE_HIGHLIGHT_PRIMARY_CENTER[1]}" rx="{EYE_HIGHLIGHT_PRIMARY_RADII[0]}" ry="{EYE_HIGHLIGHT_PRIMARY_RADII[1]}" fill="{EYE_HIGHLIGHT_FILL}"/>'
    )
    if detail == "simplified":
        return base
    return (
        base
        + f'\n          <ellipse id="eye-highlight-secondary" cx="{EYE_HIGHLIGHT_SECONDARY_CENTER[0]}" cy="{EYE_HIGHLIGHT_SECONDARY_CENTER[1]}" rx="{EYE_HIGHLIGHT_SECONDARY_RADII[0]}" ry="{EYE_HIGHLIGHT_SECONDARY_RADII[1]}" fill="{EYE_HIGHLIGHT_FILL}" opacity="{EYE_HIGHLIGHT_SECONDARY_OPACITY}"/>'
    )


def _eye_stripe_svg(detail: str) -> str:
    """Spec §24: master/small keep both the main wedge and the secondary
    depth plane. Micro is "one eye-stripe shape" -- the main wedge only."""
    main = f'        <polygon id="eye-stripe" points="{_points_attr(EYE_STRIPE_MAIN_POINTS)}" fill="{EYE_STRIPE_MAIN_FILL}"/>'
    if detail == "single":
        return main
    secondary = f'        <polygon id="eye-stripe-secondary" points="{_points_attr(EYE_STRIPE_SECONDARY_POINTS)}" fill="{EYE_STRIPE_SECONDARY_FILL}"/>'
    return main + "\n" + secondary


# ---------------------------------------------------------------------------
# Small/micro optical variants (#65, spec §24): parameter sets over the same
# geometry model above -- reduced feather/facet counts, a coarser subset of
# the same envelope points for chest faceting, fewer crown facets picked
# from the master's own CROWN_FACETS list, and simplified beak/eye/stripe
# detail levels. Nothing here draws new geometry; every shape a small/micro
# profile emits is the master's own primitive (feather shape, envelope path,
# facet ramp) at a reduced count, so a shape tweak to the master's own data
# (e.g. GORGET_FEATHER_RAMP, CHEST_MASS_POINTS) propagates to small/micro on
# the next regeneration -- exactly the "one generator run" / "shared model"
# requirement in the brief, and spec's "diverging small/micro geometry from
# the shared model" is explicitly out of scope.
# ---------------------------------------------------------------------------

# Spec §24 small: "16-22 gorget feathers." Same 5 rows, same x-range/y-center/
# color-ramp-span per row as ROW_SPECS, roughly half each row's master count
# (and modestly widened width/height so fewer, larger feathers still read as
# covering the row's own span) -- 3+4+5+4+3 = 19.
SMALL_ROW_SPECS = [
    {"id": "gorget-top-row", "count": 3, "width_range": (70, 95), "height_range": (55, 72), "x_range": (295, 520), "color_index_range": (1, 5), "y_center": 421},
    {"id": "gorget-row-2", "count": 4, "width_range": (80, 105), "height_range": (62, 80), "x_range": (270, 610), "color_index_range": (0, 4), "y_center": 470},
    {"id": "gorget-row-3", "count": 5, "width_range": (90, 115), "height_range": (70, 90), "x_range": (270, 645), "color_index_range": (0, 5), "y_center": 525},
    {"id": "gorget-row-4", "count": 4, "width_range": (105, 135), "height_range": (88, 115), "x_range": (280, 665), "color_index_range": (0, 3), "y_center": 592},
    {"id": "gorget-bottom-row", "count": 3, "width_range": (120, 160), "height_range": (115, 150), "x_range": (320, 610), "color_index_range": (0, 2), "y_center": 672},
]

# Spec §24 micro: "8-12 throat shapes" -- the same row cascade, reduced
# further to a handful of large shapes per row. 2+2+3+2+2 = 11.
MICRO_ROW_SPECS = [
    {"id": "gorget-top-row", "count": 2, "width_range": (90, 120), "height_range": (70, 90), "x_range": (295, 520), "color_index_range": (1, 5), "y_center": 421},
    {"id": "gorget-row-2", "count": 2, "width_range": (100, 130), "height_range": (78, 98), "x_range": (270, 610), "color_index_range": (0, 4), "y_center": 470},
    {"id": "gorget-row-3", "count": 3, "width_range": (110, 145), "height_range": (86, 110), "x_range": (270, 645), "color_index_range": (0, 5), "y_center": 525},
    {"id": "gorget-row-4", "count": 2, "width_range": (130, 165), "height_range": (108, 140), "x_range": (280, 665), "color_index_range": (0, 3), "y_center": 592},
    {"id": "gorget-bottom-row", "count": 2, "width_range": (150, 190), "height_range": (140, 175), "x_range": (320, 610), "color_index_range": (0, 2), "y_center": 672},
]

# Different seeds per profile (still fixed/deterministic -- byte-stable on
# regeneration) so small/micro don't just replay a truncated prefix of the
# master's own feather sequence.
SMALL_GORGET_FEATHER_ROWS = _build_gorget_feathers(SMALL_ROW_SPECS, FEATHER_SEED + 1)
MICRO_GORGET_FEATHER_ROWS = _build_gorget_feathers(MICRO_ROW_SPECS, FEATHER_SEED + 2)

# Spec §24: small wants 6-8 crown facets, micro wants 3-4 "crown planes."
# Both are index subsets of the master's own CROWN_FACETS list (same
# polygons/gray ramp, just fewer of them) rather than a separately
# triangulated set, picked to spread across the crown's front-to-rear span
# and the gray ramp rather than clustering in one corner.
CROWN_FACET_INDICES_SMALL = (0, 2, 5, 7, 9, 11, 13)  # 7 facets
CROWN_FACET_INDICES_MICRO = (0, 7, 9, 11)  # 4 facets
SMALL_CROWN_FACETS = [CROWN_FACETS[i] for i in CROWN_FACET_INDICES_SMALL]
MICRO_CROWN_FACETS = [CROWN_FACETS[i] for i in CROWN_FACET_INDICES_MICRO]

# Spec §24: small wants 6-8 chest facets, micro wants 3-4 "chest planes."
# `_ear_clip` on the full 11-vertex CHEST_MASS_POINTS always yields exactly
# 9 triangles (n-2) before any subdivision, which is already above small's
# 6-8 band and can only grow via `_subdivide_to_size` -- there's no way to
# *reduce* below the full envelope's own ear-clip count without ear-clipping
# a coarser vertex set. CHEST_MASS_POINTS_SMALL/MICRO are ordered subsets of
# the master's own point list (never new coordinates) sized to ear-clip
# straight to the target count; each subset's polygon is still a subset of
# the real chest silhouette, so it stays exactly inside the full chest-clip
# even though it now leaves some of that silhouette's area to the flat
# chest-base color underneath -- correct simplification, not a bug, per
# §24's "recognition, not fidelity."
CHEST_MASS_POINTS_SMALL = [CHEST_MASS_POINTS[i] for i in (0, 2, 4, 5, 6, 7, 8, 9)]  # 8 pts -> 6 triangles
CHEST_MASS_POINTS_MICRO = [CHEST_MASS_POINTS[i] for i in (0, 4, 6, 8, 9)]  # 5 pts -> 3 triangles
SMALL_CHEST_FACETS = _build_chest_facets(CHEST_MASS_POINTS_SMALL, max_dim=10_000, max_count=8)
MICRO_CHEST_FACETS = _build_chest_facets(CHEST_MASS_POINTS_MICRO, max_dim=10_000, max_count=4)

# Side-body facets aren't in spec §24's explicit small/micro shape budgets
# (only gorget/crown/chest/beak/eye are named) -- per §38's fidelity-priority
# ordering ("individual feather/polygon detail" is priority 8-9, the first
# thing to disappear under simplification), small keeps a reduced facet
# count and micro drops the overlay entirely, leaving the flat
# `side-body-base` color visible (still spec-accurate: "warm orange body,"
# priority 7, survives; only its own faceting detail doesn't).
SMALL_SIDE_BODY_FACETS = _build_side_body_facets(subdivisions=2)  # 6 facets
MICRO_SIDE_BODY_FACETS = _build_side_body_facets(subdivisions=0)  # none -- flat base only

MASTER_PROFILE = {
    "crown_facets": CROWN_FACETS,
    "chest_facets": CHEST_FACETS,
    "side_body_facets": SIDE_BODY_FACETS,
    "gorget_rows": GORGET_FEATHER_ROWS,
    "beak_detail": "full",
    "eye_detail": "full",
    "eye_stripe_detail": "full",
    "include_forehead": True,
    "include_cheek_separator": True,
}

SMALL_PROFILE = {
    "crown_facets": SMALL_CROWN_FACETS,
    "chest_facets": SMALL_CHEST_FACETS,
    "side_body_facets": SMALL_SIDE_BODY_FACETS,
    "gorget_rows": SMALL_GORGET_FEATHER_ROWS,
    "beak_detail": "full",  # spec §24: "single beak highlight" -- already true of the full beak
    "eye_detail": "simplified",
    "eye_stripe_detail": "full",
    "include_forehead": True,
    "include_cheek_separator": True,
}

MICRO_PROFILE = {
    "crown_facets": MICRO_CROWN_FACETS,
    "chest_facets": MICRO_CHEST_FACETS,
    "side_body_facets": MICRO_SIDE_BODY_FACETS,
    "gorget_rows": MICRO_GORGET_FEATHER_ROWS,
    "beak_detail": "single",
    "eye_detail": "micro",
    "eye_stripe_detail": "single",
    "include_forehead": False,  # not in spec §24's micro shape list
    "include_cheek_separator": False,  # not in spec §24's micro shape list
}

PROFILES = {"master": MASTER_PROFILE, "small": SMALL_PROFILE, "micro": MICRO_PROFILE}


def _build_svg(palette: dict, profile: dict = None) -> str:
    """Emit one self-contained SVG from a palette dict and an optional
    variant profile (#65, spec §24) -- `profile` defaults to MASTER_PROFILE,
    so every existing master call site is unchanged. Geometry is identical
    for every call at a given profile; only `palette` values vary within it
    -- this is the single code path light and dark both run through (spec:
    dark differs from light only via palette data, never separate drawing
    code). Head identity geometry (#62) does not vary by palette at all --
    the spec gives no light/dark split for beak/eye/stripe/forehead/cheek."""
    if profile is None:
        profile = MASTER_PROFILE
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
    <clipPath id="chest-clip">
      <polygon points="{_points_attr(CHEST_MASS_POINTS)}"/>
    </clipPath>
    <clipPath id="side-body-clip">
      <polygon points="{_points_attr(SIDE_BODY_MASS_POINTS)}"/>
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
      <g id="chest-facets" clip-path="url(#chest-clip)">
{_chest_facets_svg(profile["chest_facets"])}
      </g>
      <polygon id="side-body-base" points="{_points_attr(SIDE_BODY_MASS_POINTS)}" fill="{palette['side_body_mass']}"/>
      <g id="side-body-facets" clip-path="url(#side-body-clip)">
{_side_body_facets_svg(profile["side_body_facets"])}
      </g>
      <path id="gorget-base" d="{GORGET_MASS_PATH}" fill="{palette['gorget_mass']}"/>
      <g id="gorget-feathers" clip-path="url(#gorget-clip)">
{_gorget_feathers_svg(profile["gorget_rows"])}
      </g>
      <g id="head">
        <path id="crown-base" d="{CROWN_MASS_PATH}" fill="{palette['crown_mass']}"/>
        <g id="crown-facets" clip-path="url(#crown-clip)">
{_crown_facets_svg(profile["crown_facets"])}
        </g>{_forehead_group_svg(profile["include_forehead"])}
{_eye_stripe_svg(profile["eye_stripe_detail"])}{_cheek_separator_svg(profile["include_cheek_separator"])}
        <g id="eye">
{_eye_svg(profile["eye_detail"])}
        </g>
      </g>
      <g id="beak" clip-path="url(#beak-clip)">
{_beak_svg(profile["beak_detail"])}
      </g>
    </g>
  </g>
</svg>
"""


# Stable, documented output paths (spec §40 naming). Kept as the master-only
# {variant: name} shape it has always been -- `generate()` below is
# unchanged (still emits only the two master SVGs), so every existing call
# site keeps working. ALL_OUTPUT_NAMES (below) is the full six-file map
# small/micro (#65) add alongside it.
OUTPUT_NAMES = {
    "light": "hummingbird-icon-master-light.svg",
    "dark": "hummingbird-icon-master-dark.svg",
}

# Spec §24/§40: one generator run emits all six SVGs -- master/small/micro x
# light/dark. Keyed by (profile, variant) so `generate_all` can return one
# flat dict without colliding on the "light"/"dark" keys `generate` already
# uses for the master pair.
ALL_OUTPUT_NAMES = {
    ("master", "light"): "hummingbird-icon-master-light.svg",
    ("master", "dark"): "hummingbird-icon-master-dark.svg",
    ("small", "light"): "hummingbird-icon-small-light.svg",
    ("small", "dark"): "hummingbird-icon-small-dark.svg",
    ("micro", "light"): "hummingbird-icon-micro-light.svg",
    ("micro", "dark"): "hummingbird-icon-micro-dark.svg",
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


def generate_all(out_dir: Path = DEFAULT_OUT_DIR) -> dict:
    """Emit all six SVGs (master/small/micro x light/dark) into out_dir in
    one run (spec §24/§40). Returns {(profile, variant): Path}."""
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    paths = {}
    for profile_name, profile in PROFILES.items():
        for variant, palette in PALETTES.items():
            out_path = out_dir / ALL_OUTPUT_NAMES[(profile_name, variant)]
            out_path.write_text(_build_svg(palette, profile))
            paths[(profile_name, variant)] = out_path
    return paths


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(
        prog="icon_generator.py",
        description="Generate all six (master/small/micro x light/dark) hummingbird icon SVGs (#61/#65).",
    )
    parser.add_argument(
        "--out-dir",
        type=Path,
        default=DEFAULT_OUT_DIR,
        help=f"output directory (default: {DEFAULT_OUT_DIR})",
    )
    args = parser.parse_args(argv)
    paths = generate_all(args.out_dir)
    for (profile_name, variant), path in sorted(paths.items()):
        print(f"{profile_name}-{variant} -> {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
