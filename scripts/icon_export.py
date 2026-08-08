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
import xml.etree.ElementTree as ET
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


def render_png_matrix(
    variant: str, out_dir: Path, icon_dir: Path = DEFAULT_ICON_DIR, sizes=tuple(EXPORT_MATRIX)
) -> dict:
    """Render each of `sizes` for one variant, every size from its own
    correct §41 optical source. Returns {size: Path}.

    `sizes` is an iterable of size ints (default: every EXPORT_MATRIX
    key) -- NOT a {size: profile} mapping. Each size's source profile
    always comes from the module-level EXPORT_MATRIX via
    `source_svg_path`; there's no per-call override of which profile a
    size sources from, so a `sizes` argument's *values* (if it happened
    to be a dict) would silently be ignored. Pass a subset of sizes to
    render fewer, not a dict pointing at different profiles."""
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


_SVG_NS = "http://www.w3.org/2000/svg"


def visible_shape_count(svg_text: str, *, exclude_ids=("background",)) -> int:
    """Count of drawable (non-group, non-background) elements that
    actually render -- what spec §45's "<=15 visible shapes" is judged
    against. `<defs>` is stripped first: its clipPath/gradient children
    (`<path>`/`<polygon>` inside `beak-clip`, `crown-clip`, etc.) match
    the same tag names as real artwork but never paint anything -- left
    in, they silently inflate the count (6 clipPath shapes on top of the
    favicon's real 9, for example)."""
    root = ET.fromstring(svg_text)
    for defs in list(root.findall(f"{{{_SVG_NS}}}defs")):
        root.remove(defs)
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
# artwork, uniformly scaled ~8-12% and re-centered on the canvas (spec
# §44's own "shrinking the bird about 8-12%", ANDROID_FOREGROUND_SCALE =
# 0.90, the band's midpoint, below).
#
# What that scale is actually checked against: the seven named landmark
# points in icon_generator.LANDMARKS plus EYE_CENTER -- a small, named set
# standing in for the head/eye/gorget *identity* -- clear Android's real
# safe zone (a circle 66dp in diameter, centered, within the 108dp
# adaptive canvas -- ANDROID_SAFE_ZONE_RADIUS below) once scaled, except
# for the beak-tip spike (SAFE_ZONE_EXEMPT_LANDMARKS below), which stays
# outside even at max shrink and is treated as a documented exception
# (mirroring spec §26's own "body bleed at bottom" exception for the app
# icon's safe *area*).
#
# What that scale does NOT achieve: literal full-silhouette containment.
# Measuring the actual rendered foreground.svg (not just its landmarks)
# against the same circle shows roughly 39% of its own opaque pixels
# render outside it -- the chest and side-body masses are entirely
# outside the circle, and the gorget bleeds past it on both sides. Full
# containment of every opaque pixel would need close to a 0.46 scale
# (measured: at a 512px render the current 0.90-scaled foreground's max
# opaque radius is ~304px vs a ~156px safe radius, so
# 0.90 * 156/304 ~ 0.463 -- not the 0.90 the spec's own "8-12%"
# describes), which
# would leave a much smaller, over-shrunk bird relative to what §44 asks
# for. 0.90 is a deliberate reading of the spec text over literal
# pixel-perfect compliance: Android's own adaptive-icon convention
# tolerates exactly this (content outside the safe circle gets cropped by
# some launchers' masks, not all; only the guaranteed-safe circle's
# content survives everywhere) -- see design/icon/README.md's "Export
# matrix + platform packaging" section for the full writeup and the
# committed qc/android-adaptive-safe-zone.png overlay.
# ---------------------------------------------------------------------------

ANDROID_CANVAS_CENTER = (512, 512)
ANDROID_FOREGROUND_SCALE = 0.90  # spec §44: "shrinking the bird about 8-12%"
ANDROID_SAFE_ZONE_DIAMETER_DP = 66
ANDROID_CANVAS_DP = 108
ANDROID_SAFE_ZONE_RADIUS = 1024 * (ANDROID_SAFE_ZONE_DIAMETER_DP / ANDROID_CANVAS_DP) / 2

# The beak-tip spike's own two landmarks are the documented bleed
# exception above; every other *named landmark* (not every rendered
# pixel -- see the module note above) clears the safe zone at
# ANDROID_FOREGROUND_SCALE.
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


_TXT_PIXEL_RE = re.compile(r"^(\d+),(\d+): \((\d+),(\d+),(\d+),(\d+)\)")


def foreground_opaque_pixel_outside_safe_zone_fraction(size: int = 256) -> float:
    """The real pixel-level measurement behind the module docstring's
    "~39% of the foreground's own opaque pixels render outside the safe
    zone" claim -- unlike `landmark_within_safe_zone`, which only checks
    the seven named LANDMARKS + EYE_CENTER, this renders the actual
    foreground.svg and scans every pixel. Downsampled to `size` (the
    fraction is scale-invariant, so full 1024px isn't needed to be
    representative)."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp_dir = Path(tmp)
        fg_path = tmp_dir / "foreground.svg"
        fg_path.write_text(build_android_foreground_svg())
        png = icon_harness.render_one(fg_path, size, tmp_dir / "foreground.png")
        result = subprocess.run(["magick", str(png), "txt:"], check=True, capture_output=True, text=True)

    cx = cy = size / 2
    radius = size * (ANDROID_SAFE_ZONE_DIAMETER_DP / ANDROID_CANVAS_DP) / 2
    total_opaque = 0
    outside_opaque = 0
    for line in result.stdout.splitlines()[1:]:
        match = _TXT_PIXEL_RE.match(line)
        if not match:
            continue
        x, y, _r, _g, _b, alpha = (int(g) for g in match.groups())
        if alpha == 0:
            continue
        total_opaque += 1
        if math.hypot(x - cx, y - cy) > radius:
            outside_opaque += 1
    return outside_opaque / total_opaque if total_opaque else 0.0


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
# Committed QC evidence (design/icon/qc/favicon-actual-16.png,
# qc/android-adaptive-safe-zone.png) -- same "regenerate with one
# documented command" convention as the harness's own qc/*.png (grayscale,
# blur, silhouette, overlay, contact-sheet, actual-size-sheet). Regenerate
# both with `python3 scripts/icon_export.py qc`.
# ---------------------------------------------------------------------------

FAVICON_QC_NAME = "favicon-actual-16.png"
FAVICON_QC_DISPLAY_SIZE = 256  # nearest-neighbor upscale for legibility, same convention as actual-size sheets

ANDROID_SAFE_ZONE_QC_NAME = "android-adaptive-safe-zone.png"
ANDROID_SAFE_ZONE_QC_SIZE = 512  # display resolution for the composited preview


def build_favicon_qc_render(out_path: Path, display_size: int = FAVICON_QC_DISPLAY_SIZE) -> Path:
    """spec §45 acceptance evidence: the favicon at its own actual 16px,
    nearest-neighbor upscaled for legibility (never a quality filter --
    that would invent detail the real 16px render doesn't have, same
    honesty rule as icon_harness.tile_filter). `-strip` matters for
    byte-reproducibility, not just file size: ImageMagick otherwise embeds
    a `date:create`/`date:modify` tEXt chunk with the current wall-clock
    time, which would make CommittedQcRendersUpToDateTest fail on every
    run regardless of whether the actual pixels changed."""
    out_path = Path(out_path)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        tmp_dir = Path(tmp)
        svg_path = tmp_dir / "favicon.svg"
        svg_path.write_text(build_favicon_svg())
        raw = icon_harness.render_one(svg_path, 16, tmp_dir / "favicon-16.png")
        subprocess.run(
            ["magick", str(raw), "-filter", "point", "-resize", f"{display_size}x{display_size}", "-strip", str(out_path)],
            check=True,
            capture_output=True,
        )
    return out_path


def build_android_safe_zone_qc_render(out_path: Path, size: int = ANDROID_SAFE_ZONE_QC_SIZE) -> Path:
    """Composited background + foreground layers with Android's real
    66/108dp safe-zone circle overlaid in red. A visual aid for the
    landmark-based claim (`landmark_within_safe_zone`) -- NOT evidence
    that every opaque foreground pixel stays inside the circle; it
    doesn't (see the module docstring above /
    `foreground_opaque_pixel_outside_safe_zone_fraction`: ~39% of the
    foreground's own opaque pixels render outside it, chest and
    side-body entirely so)."""
    out_path = Path(out_path)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        tmp_dir = Path(tmp)
        bg_path = tmp_dir / "background.svg"
        fg_path = tmp_dir / "foreground.svg"
        bg_path.write_text(build_android_background_svg())
        fg_path.write_text(build_android_foreground_svg())
        bg_png = icon_harness.render_one(bg_path, 1024, tmp_dir / "background.png")
        fg_png = icon_harness.render_one(fg_path, 1024, tmp_dir / "foreground.png")
        composite = tmp_dir / "composite.png"
        subprocess.run(
            ["magick", str(bg_png), str(fg_png), "-composite", str(composite)],
            check=True,
            capture_output=True,
        )
        cx, cy = ANDROID_CANVAS_CENTER
        edge_y = cy - ANDROID_SAFE_ZONE_RADIUS
        overlay = tmp_dir / "overlay.png"
        subprocess.run(
            [
                "magick",
                str(composite),
                "-stroke",
                "red",
                "-strokewidth",
                "4",
                "-fill",
                "none",
                "-draw",
                f"circle {cx},{cy} {cx},{edge_y}",
                str(overlay),
            ],
            check=True,
            capture_output=True,
        )
        subprocess.run(
            ["magick", str(overlay), "-resize", f"{size}x{size}", "-strip", str(out_path)],
            check=True,
            capture_output=True,
        )
    return out_path


def generate_qc_renders(out_dir: Path = DEFAULT_ICON_DIR / "qc") -> dict:
    out_dir = Path(out_dir)
    favicon_path = build_favicon_qc_render(out_dir / FAVICON_QC_NAME)
    android_path = build_android_safe_zone_qc_render(out_dir / ANDROID_SAFE_ZONE_QC_NAME)
    return {"favicon": favicon_path, "android_safe_zone": android_path}


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

    sub.add_parser("svgs", help="(re)generate favicon.svg + android background/foreground SVGs")

    p_qc = sub.add_parser("qc", help="(re)generate committed QC evidence: favicon-actual-16.png + android-adaptive-safe-zone.png")
    p_qc.add_argument("--out-dir", type=Path, default=None, help="default: <icon-dir>/qc")

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
    elif args.command == "qc":
        out_dir = args.out_dir or (args.icon_dir / "qc")
        paths = generate_qc_renders(out_dir)
        print(paths["favicon"])
        print(paths["android_safe_zone"])
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
