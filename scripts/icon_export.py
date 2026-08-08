#!/usr/bin/env python3
"""Export matrix + platform packaging (#66) from the six generator SVGs.

Consumes `design/icon/hummingbird-icon-{master,small,micro}-{light,dark}.svg`
(icon_generator.py, #61-#65) and produces everything a platform actually
ships: the PNG export matrix (spec §41), a macOS `.icns` (§42), a Windows
`.ico` (§43) whose 16/24/32 entries are built from the micro/small SVGs
(not a master downscale), a dedicated favicon SVG (§45), and Android
adaptive-icon background/foreground SVG layers (§44, SVG only -- no APK/
resource packaging). See design/icon/README.md for the documented
one-command usage.

No new artwork -- every shape here is either a direct render of an
existing generator SVG or (favicon/Android foreground) a structural
transform of one (dropped detail, or a uniform scale+translate) built by
reusing `icon_generator._build_svg`.
"""

import argparse
import math
import re
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO_ROOT / "scripts"))

import icon_generator  # noqa: E402
import icon_harness  # noqa: E402

DEFAULT_ICON_DIR = icon_generator.DEFAULT_OUT_DIR  # design/icon
# Build outputs (PNG matrix, .icns, .ico) are regenerable rasters, not
# source of truth -- unlike the SVGs, they are not committed (see
# .gitignore and design/icon/README.md).
DEFAULT_EXPORT_DIR = REPO_ROOT / "design" / "icon" / "export"

# ---------------------------------------------------------------------------
# PNG export matrix (spec §41): each size rendered from its own optical
# source, not a single master downscaled to every size.
# ---------------------------------------------------------------------------

EXPORT_MATRIX = {
    1024: "master",
    512: "master",
    256: "master",
    128: "master",
    64: "small",
    48: "small",
    32: "small",
    24: "micro",
    16: "micro",
}


def source_svg_path(size: int, variant: str, icon_dir: Path = DEFAULT_ICON_DIR) -> Path:
    """Which committed SVG spec §41's matrix assigns to `size`."""
    profile = EXPORT_MATRIX[size]
    name = icon_generator.ALL_OUTPUT_NAMES[(profile, variant)]
    return Path(icon_dir) / name


def _normalize_png(png_path: Path) -> Path:
    """sRGB, RGBA, 8-bit, no embedded ICC profile (spec §41).

    `-type TrueColorAlpha` alone isn't enough: PNG's own color-type is
    chosen by ImageMagick's encoder from actual pixel content, so a fully
    opaque render (e.g. the 16px micro tile) gets silently written back
    down to a 3-channel RGB PNG with no alpha channel at all. The explicit
    `png:color-type=6` (RGBA) define forces the encoder's hand regardless
    of content.
    """
    subprocess.run(
        [
            "magick",
            str(png_path),
            "-strip",
            "-depth",
            "8",
            "-colorspace",
            "sRGB",
            "-define",
            "png:color-type=6",
            str(png_path),
        ],
        check=True,
        capture_output=True,
    )
    return png_path


def render_png_matrix(variant: str, out_dir: Path, icon_dir: Path = DEFAULT_ICON_DIR, sizes=EXPORT_MATRIX) -> dict:
    """Render every §41 matrix size for one variant, each from its own
    correct optical source. Returns {size: Path}."""
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    paths = {}
    for size in sizes:
        svg_path = source_svg_path(size, variant, icon_dir)
        out_path = out_dir / f"hummingbird-icon-{variant}-{size}.png"
        icon_harness.render_one(svg_path, size, out_path)
        _normalize_png(out_path)
        paths[size] = out_path
    return paths


def png_has_no_icc_profile(png_path: Path) -> bool:
    result = subprocess.run(
        ["magick", "identify", "-format", "%[profile:icc]", str(png_path)],
        check=True,
        capture_output=True,
        text=True,
    )
    return result.stdout.strip() == ""


def png_bit_depth(png_path: Path) -> int:
    result = subprocess.run(
        ["magick", "identify", "-format", "%z", str(png_path)],
        check=True,
        capture_output=True,
        text=True,
    )
    return int(result.stdout.strip())


def png_has_alpha(png_path: Path) -> bool:
    result = subprocess.run(
        ["magick", "identify", "-format", "%A", str(png_path)],
        check=True,
        capture_output=True,
        text=True,
    )
    return result.stdout.strip() in ("True", "Blend")


# ---------------------------------------------------------------------------
# macOS .icns (spec §42)
# ---------------------------------------------------------------------------

# (iconset filename, render size) -- Apple's fixed iconutil naming
# convention. Every referenced size is already one of EXPORT_MATRIX's own
# sizes, so each iconset tile still comes from the §41-correct optical
# source (source_svg_path), never a single master stretched to every size.
ICNS_ICONSET_ENTRIES = (
    ("icon_16x16.png", 16),
    ("icon_16x16@2x.png", 32),
    ("icon_32x32.png", 32),
    ("icon_32x32@2x.png", 64),
    ("icon_128x128.png", 128),
    ("icon_128x128@2x.png", 256),
    ("icon_256x256.png", 256),
    ("icon_256x256@2x.png", 512),
    ("icon_512x512.png", 512),
    ("icon_512x512@2x.png", 1024),
)


def build_icns(variant: str, out_path: Path, icon_dir: Path = DEFAULT_ICON_DIR) -> Path:
    """Build a macOS .icns via `iconutil`, covering spec §42's 16-1024
    size ladder (as the @1x/@2x iconset pairs iconutil requires)."""
    out_path = Path(out_path)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        iconset_dir = Path(tmp) / "hummingbird.iconset"
        iconset_dir.mkdir()
        for filename, size in ICNS_ICONSET_ENTRIES:
            svg_path = source_svg_path(size, variant, icon_dir)
            icon_harness.render_one(svg_path, size, iconset_dir / filename)
        subprocess.run(
            ["iconutil", "-c", "icns", str(iconset_dir), "-o", str(out_path)],
            check=True,
            capture_output=True,
        )
    return out_path


# ---------------------------------------------------------------------------
# Windows .ico (spec §43): 16/24/32 must come from micro/small artwork, not
# a master downscale.
# ---------------------------------------------------------------------------

ICO_SIZES = (16, 24, 32, 48, 64, 128, 256)


def build_ico(variant: str, out_path: Path, icon_dir: Path = DEFAULT_ICON_DIR) -> Path:
    out_path = Path(out_path)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        tmp_dir = Path(tmp)
        pngs = []
        for size in ICO_SIZES:
            svg_path = source_svg_path(size, variant, icon_dir)
            png_path = icon_harness.render_one(svg_path, size, tmp_dir / f"ico-{size}.png")
            pngs.append(png_path)
        subprocess.run(
            ["magick", *[str(p) for p in pngs], str(out_path)],
            check=True,
            capture_output=True,
        )
    return out_path


def extract_ico_frame(ico_path: Path, size: int, out_path: Path) -> Path:
    """Pull the `size`px frame out of a built .ico by its ICO_SIZES index
    -- ICO_SIZES has one entry per size, so the index is unambiguous."""
    index = ICO_SIZES.index(size)
    out_path = Path(out_path)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        ["magick", f"{ico_path}[{index}]", str(out_path)],
        check=True,
        capture_output=True,
    )
    return out_path


# ---------------------------------------------------------------------------
# Favicon (spec §45): a dedicated, further-simplified variation -- gray
# head, black beak, black eye, orange throat, cream chest, <=15 visible
# shapes. Reuses the master's own geometry/palette data at zero facet/row
# counts (icon_generator._build_svg accepts any profile dict, not only the
# three registered in icon_generator.PROFILES) rather than hand-drawing
# new shapes.
# ---------------------------------------------------------------------------

FAVICON_PROFILE = {
    "crown_facets": [],
    "chest_facets": [],
    "side_body_facets": [],
    "gorget_rows": [],
    "beak_detail": "single",
    "eye_detail": "micro",
    "eye_stripe_detail": "single",
    "include_forehead": False,
    "include_cheek_separator": False,
}

# Browsers don't switch a plain <link rel="icon"> by OS theme without
# extra manifest wiring (out of scope, map #35), so the favicon is a
# single light-mode variation, matching spec §45's single (not light/dark
# split) color list.
FAVICON_OUTPUT_NAME = "hummingbird-icon-favicon.svg"

_VISIBLE_SHAPE_TAGS = {"path", "polygon", "ellipse", "circle", "rect"}


def _local_tag(el) -> str:
    tag = el.tag
    return tag.split("}", 1)[-1] if "}" in tag else tag


def build_favicon_svg(palette: dict = None) -> str:
    palette = palette or icon_generator.LIGHT_PALETTE
    return icon_generator._build_svg(palette, FAVICON_PROFILE)


def visible_shape_count(svg_text: str, *, exclude_ids=("background",)) -> int:
    """Count of drawable (non-group, non-background) elements -- what
    spec §45's "<=15 visible shapes" is judged against."""
    import xml.etree.ElementTree as ET

    root = ET.fromstring(svg_text)
    count = 0
    for el in root.iter():
        if _local_tag(el) in _VISIBLE_SHAPE_TAGS and el.get("id") not in exclude_ids:
            count += 1
    return count


def generate_favicon(out_dir: Path = DEFAULT_ICON_DIR) -> Path:
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    out_path = out_dir / FAVICON_OUTPUT_NAME
    out_path.write_text(build_favicon_svg())
    return out_path


# ---------------------------------------------------------------------------
# Android adaptive-icon layers (spec §44): background.svg + foreground.svg,
# SVG only, no APK/resource packaging. The foreground is the master bird
# artwork, uniformly scaled ~8-12% and re-centered on the canvas so the
# head/eye/gorget identity mass clears Android's official safe zone (a
# circle 66dp in diameter, centered, within the 108dp adaptive canvas --
# ANDROID_SAFE_ZONE_RADIUS below). The beak-tip spike is the one
# exception: even at the low end of that 8-12% band it stays outside the
# safe-zone circle (it reaches to (135,70), close to the canvas corner),
# and pulling it inside would need a ~50% shrink well past what the spec
# text describes. This mirrors spec §26's own explicit "body bleed at
# bottom" exception for the app icon's safe *area* -- a thin extremity is
# allowed to bleed past the guaranteed-safe region by design; Android's
# own adaptive-icon convention tolerates the same for non-identity
# extremities (only some launchers' masks will crop it). See
# design/icon/README.md for this reasoning and the per-landmark numbers.
# ---------------------------------------------------------------------------

ANDROID_CANVAS_CENTER = (512, 512)
ANDROID_FOREGROUND_SCALE = 0.90  # spec §44: "shrinking the bird about 8-12%"
ANDROID_SAFE_ZONE_DIAMETER_DP = 66
ANDROID_CANVAS_DP = 108
ANDROID_SAFE_ZONE_RADIUS = 1024 * (ANDROID_SAFE_ZONE_DIAMETER_DP / ANDROID_CANVAS_DP) / 2

# The beak-tip spike's own two landmarks are the documented bleed
# exception above; every other composition landmark must clear the safe
# zone at ANDROID_FOREGROUND_SCALE.
SAFE_ZONE_EXEMPT_LANDMARKS = frozenset({"beak_tip", "beak_lower_tip"})

BACKGROUND_OUTPUT_NAME = "background.svg"
FOREGROUND_OUTPUT_NAME = "foreground.svg"

_BACKGROUND_RECT_RE = re.compile(r'<rect id="background"[^>]*/>')
_BIRD_GROUP_OPEN_RE = re.compile(r'<g id="bird">')


def landmark_after_transform(point, scale: float = ANDROID_FOREGROUND_SCALE, center=ANDROID_CANVAS_CENTER) -> tuple:
    cx, cy = center
    x, y = point
    return (cx + scale * (x - cx), cy + scale * (y - cy))


def distance_from_center(point, center=ANDROID_CANVAS_CENTER) -> float:
    cx, cy = center
    x, y = point
    return math.hypot(x - cx, y - cy)


def landmark_within_safe_zone(point, scale: float = ANDROID_FOREGROUND_SCALE, center=ANDROID_CANVAS_CENTER) -> bool:
    return distance_from_center(landmark_after_transform(point, scale, center), center) <= ANDROID_SAFE_ZONE_RADIUS


def build_android_background_svg(palette: dict = None) -> str:
    palette = palette or icon_generator.LIGHT_PALETTE
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024">\n'
        "  <defs>\n"
        '    <radialGradient id="background-gradient" cx="50%" cy="50%" r="75%">\n'
        f'      <stop offset="0%" stop-color="{palette["background_start"]}"/>\n'
        f'      <stop offset="100%" stop-color="{palette["background_end"]}"/>\n'
        "    </radialGradient>\n"
        "  </defs>\n"
        '  <rect id="background" x="0" y="0" width="1024" height="1024" fill="url(#background-gradient)"/>\n'
        "</svg>\n"
    )


def build_android_foreground_svg(
    palette: dict = None,
    profile: dict = None,
    scale: float = ANDROID_FOREGROUND_SCALE,
    center=ANDROID_CANVAS_CENTER,
) -> str:
    """Master bird artwork (full detail, spec §44 gives no reduced shape
    budget), background rect dropped (adaptive foreground is transparent
    outside the bird), wrapped in a scale-about-`center` transform on the
    existing `id="bird"` group -- the same geometry the master SVG already
    emits, not separately redrawn."""
    palette = palette or icon_generator.LIGHT_PALETTE
    profile = profile or icon_generator.MASTER_PROFILE
    full_svg = icon_generator._build_svg(palette, profile)
    without_background = _BACKGROUND_RECT_RE.sub("", full_svg, count=1)
    cx, cy = center
    transform = f"translate({cx},{cy}) scale({scale}) translate({-cx},{-cy})"
    return _BIRD_GROUP_OPEN_RE.sub(f'<g id="bird" transform="{transform}">', without_background, count=1)


def generate_android_layers(out_dir: Path = DEFAULT_ICON_DIR / "android") -> dict:
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    background_path = out_dir / BACKGROUND_OUTPUT_NAME
    foreground_path = out_dir / FOREGROUND_OUTPUT_NAME
    background_path.write_text(build_android_background_svg())
    foreground_path.write_text(build_android_foreground_svg())
    return {"background": background_path, "foreground": foreground_path}


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def _all(icon_dir: Path, out_dir: Path) -> None:
    icon_dir = Path(icon_dir)
    out_dir = Path(out_dir)
    generate_favicon(icon_dir)
    generate_android_layers(icon_dir / "android")
    for variant in ("light", "dark"):
        render_png_matrix(variant, out_dir / "png" / variant, icon_dir)
        build_icns(variant, out_dir / "icns" / f"hummingbird-icon-{variant}.icns", icon_dir)
        build_ico(variant, out_dir / "ico" / f"hummingbird-icon-{variant}.ico", icon_dir)


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(
        prog="icon_export.py",
        description="Export matrix + platform packaging from the six generator SVGs (#66). "
        "See design/icon/README.md.",
    )
    parser.add_argument("--icon-dir", type=Path, default=DEFAULT_ICON_DIR, help="source SVG directory")
    sub = parser.add_subparsers(dest="command", required=True)

    p_svgs = sub.add_parser("svgs", help="(re)generate favicon.svg + android background/foreground SVGs")

    p_png = sub.add_parser("png-matrix", help="spec §41 PNG export matrix, one variant")
    p_png.add_argument("--variant", required=True, choices=("light", "dark"))
    p_png.add_argument("--out-dir", type=Path, required=True)

    p_icns = sub.add_parser("icns", help="spec §42 macOS .icns, one variant")
    p_icns.add_argument("--variant", required=True, choices=("light", "dark"))
    p_icns.add_argument("--out", type=Path, required=True)

    p_ico = sub.add_parser("ico", help="spec §43 Windows .ico, one variant")
    p_ico.add_argument("--variant", required=True, choices=("light", "dark"))
    p_ico.add_argument("--out", type=Path, required=True)

    p_all = sub.add_parser("all", help="everything: svgs + png matrix + icns + ico, both variants")
    p_all.add_argument("--out-dir", type=Path, default=DEFAULT_EXPORT_DIR)

    args = parser.parse_args(argv)

    if args.command == "svgs":
        favicon = generate_favicon(args.icon_dir)
        layers = generate_android_layers(args.icon_dir / "android")
        print(favicon)
        print(layers["background"])
        print(layers["foreground"])
    elif args.command == "png-matrix":
        paths = render_png_matrix(args.variant, args.out_dir, args.icon_dir)
        for size, path in sorted(paths.items()):
            print(f"{size}px -> {path}")
    elif args.command == "icns":
        print(build_icns(args.variant, args.out, args.icon_dir))
    elif args.command == "ico":
        print(build_ico(args.variant, args.out, args.icon_dir))
    elif args.command == "all":
        _all(args.icon_dir, args.out_dir)
        print(f"exported everything under {args.out_dir} (svgs under {args.icon_dir})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
