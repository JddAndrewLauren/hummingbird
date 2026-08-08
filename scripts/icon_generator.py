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


def _build_svg(palette: dict) -> str:
    """Emit one self-contained master SVG from a palette dict. Geometry is
    identical for every call; only `palette` values vary -- this is the
    single code path both light and dark run through (spec: dark differs
    from light only via palette data, never separate drawing code)."""
    silhouette_d = build_bird_silhouette_path()
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024">
  <defs>
    <radialGradient id="background-gradient" cx="50%" cy="50%" r="75%">
      <stop offset="0%" stop-color="{palette['background_start']}"/>
      <stop offset="100%" stop-color="{palette['background_end']}"/>
    </radialGradient>
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
      <path id="crown-base" d="{CROWN_MASS_PATH}" fill="{palette['crown_mass']}"/>
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
