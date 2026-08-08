#!/usr/bin/env python3
"""Render + QC harness for the app icon SVG slices (#60).

Wraps `resvg` (rasterize) and ImageMagick's `magick` (compositing/QC) so
every later icon slice (#61-#66) iterates against the same, deterministic
render path. No browser dependency. See design/icon/README.md for the
documented one-command-per-mode usage and tool dependencies.
"""

import argparse
import re
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
REFERENCE_DIR = REPO_ROOT / "design" / "icon" / "reference"

# Fixed, deterministic size ladder for the contact sheet / render matrix.
CONTACT_SHEET_SIZES = (1024, 128, 64, 32, 16)

# Small/micro optical variants (#65, spec §24) are only meant to be judged
# at their own real target sizes -- 32-64px (small) and 16-24px (micro) --
# so this ladder skips 1024/128 (no legibility upscaling to hide behind)
# and adds 24, matching spec §47's own inspection sizes at the small end.
ACTUAL_SIZE_LADDER = (64, 32, 24, 16)


def render_one(svg_path: Path, size: int, out_path: Path) -> Path:
    """Rasterize svg_path at size x size actual pixels via resvg."""
    out_path.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        [
            "resvg",
            "-w",
            str(size),
            "-h",
            str(size),
            str(svg_path),
            str(out_path),
        ],
        check=True,
        capture_output=True,
    )
    return out_path


def render(svg_path: Path, out_dir: Path, sizes=CONTACT_SHEET_SIZES) -> dict:
    """Rasterize svg_path at each of `sizes`, actual pixel dimensions.

    Returns {size: Path} for the produced PNGs.
    """
    svg_path = Path(svg_path)
    out_dir = Path(out_dir)
    stem = svg_path.stem
    paths = {}
    for size in sizes:
        out_path = out_dir / f"{stem}-{size}.png"
        paths[size] = render_one(svg_path, size, out_path)
    return paths


CONTACT_SHEET_DISPLAY_SIZE = 256  # tile height/width for the montage, px


def reference_path(variant: str) -> Path:
    if variant not in ("light", "dark"):
        raise ValueError(f"variant must be 'light' or 'dark', got {variant!r}")
    return REFERENCE_DIR / f"{variant}-1024.png"


def tile_filter(source_size: int, display_size: int) -> str:
    """Which ImageMagick -filter to use when resizing a rendered tile to
    the contact sheet's display size. Upscaling (source_size <=
    display_size) uses nearest-neighbor so small-size honesty is
    preserved -- no filter can invent detail that isn't in the actual
    render. Downscaling (the 1024 tile shrunk to fit the sheet) uses a
    quality filter instead; point-sampling a 1024px render down to 256px
    would alias real art rather than showing it."""
    return "point" if source_size <= display_size else "Lanczos"


def _resize_tile(src: Path, dst: Path, display_size: int, source_size: int) -> Path:
    dst.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        [
            "magick",
            str(src),
            "-filter",
            tile_filter(source_size, display_size),
            "-resize",
            f"{display_size}x{display_size}",
            str(dst),
        ],
        check=True,
        capture_output=True,
    )
    return dst


def contact_sheet(
    svg_path: Path,
    variant: str,
    out_path: Path,
    sizes=CONTACT_SHEET_SIZES,
    display_size: int = CONTACT_SHEET_DISPLAY_SIZE,
) -> Path:
    """One image: each rendered size (actual pixels, nearest-neighbor
    upscaled for display) beside the matching reference crop."""
    svg_path = Path(svg_path)
    out_path = Path(out_path)

    with tempfile.TemporaryDirectory() as tmp:
        tmp_dir = Path(tmp)
        renders = render(svg_path, tmp_dir, sizes)
        tiles = []
        for size in sizes:
            tile = _resize_tile(renders[size], tmp_dir / f"tile-{size}.png", display_size, size)
            tiles.append(tile)
        ref_tile = tmp_dir / "tile-reference.png"
        subprocess.run(
            [
                "magick",
                str(reference_path(variant)),
                "-resize",
                f"{display_size}x{display_size}",
                str(ref_tile),
            ],
            check=True,
            capture_output=True,
        )
        tiles.append(ref_tile)

        out_path.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(
            ["magick", *[str(t) for t in tiles], "-background", "white", "+append", str(out_path)],
            check=True,
            capture_output=True,
        )
    return out_path


def actual_size_sheet(
    svg_path: Path,
    out_path: Path,
    sizes=ACTUAL_SIZE_LADDER,
    background: str = "white",
) -> Path:
    """One image: each rendered size at its own true pixel dimensions,
    side by side -- no legibility upscaling (unlike `contact_sheet`, which
    nearest-neighbor-upscales every tile to a common display size). This is
    what the small/micro optical variants (#65, spec §24) are actually
    judged at, since a resized preview can make a design read better or
    worse than it will at its real target size."""
    svg_path = Path(svg_path)
    out_path = Path(out_path)

    with tempfile.TemporaryDirectory() as tmp:
        tmp_dir = Path(tmp)
        renders = render(svg_path, tmp_dir, sizes)
        tiles = [str(renders[size]) for size in sizes]

        out_path.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(
            ["magick", *tiles, "-background", background, "-gravity", "South", "+append", str(out_path)],
            check=True,
            capture_output=True,
        )
    return out_path


def png_dimensions(png_path: Path) -> tuple:
    """(width, height) of a PNG, via `magick identify`."""
    result = subprocess.run(
        ["magick", "identify", "-format", "%w %h", str(png_path)],
        check=True,
        capture_output=True,
        text=True,
    )
    width_str, height_str = result.stdout.split()
    return (int(width_str), int(height_str))


def is_grayscale(png_path: Path) -> bool:
    """True if every pixel's R, G and B channels are equal (spec §48)."""
    result = subprocess.run(
        ["magick", "identify", "-format", "%[type]", str(png_path)],
        check=True,
        capture_output=True,
        text=True,
    )
    return result.stdout.strip() in ("Grayscale", "GrayscaleAlpha", "GrayscaleMatte")


def grayscale(svg_path: Path, out_path: Path, size: int = 1024) -> Path:
    """QC mode (spec §48): full grayscale conversion of a 1024px render."""
    return _qc_render(svg_path, out_path, size, extra_args=["-colorspace", "Gray"])


def blur(svg_path: Path, out_path: Path, size: int = 1024, radius: int = 8) -> Path:
    """QC mode (spec §49): ~8px blur preview of a 1024px render."""
    return _qc_render(svg_path, out_path, size, extra_args=["-blur", f"0x{radius}"])


# Master SVGs carry a full-bleed opaque `id="background"` rect (spec §6/
# §21). Naively colorizing every opaque pixel black (as `_qc_render`
# does) would flatten that too, leaving the whole 1024x1024 canvas solid
# black -- telling a reviewer nothing about spec §50's "should still
# strongly suggest the approved icon" (the design/icon/README.md
# "Note" this superseded). Matches the rect's own opening tag verbatim
# (`icon_generator._build_svg` always emits it as one self-closing
# element with `id="background"` first in its attribute list) rather than
# every element with that id, since only the background rect -- not bird
# geometry -- should ever be excluded from the silhouette.
_BACKGROUND_RECT_RE = re.compile(r'<rect id="background"[^>]*/>')


def _strip_background(svg_path: Path, tmp_dir: Path) -> Path:
    """Write a copy of svg_path with its `id="background"` rect removed,
    so a silhouette render leaves that region transparent instead of
    flattening it to black alongside the bird. A no-op (returns the
    original text unchanged) for any SVG without that element -- e.g.
    the harness's own stub.svg, which is already transparent outside its
    opaque rect."""
    text = svg_path.read_text()
    stripped_text = _BACKGROUND_RECT_RE.sub("", text)
    stripped_path = tmp_dir / f"{svg_path.stem}-no-background.svg"
    stripped_path.write_text(stripped_text)
    return stripped_path


def silhouette(svg_path: Path, out_path: Path, size: int = 1024) -> Path:
    """QC mode (spec §50): every opaque fill flattened to black, alpha
    kept -- with the master's own full-bleed background rect excluded
    first (see `_strip_background`), so only the bird itself silhouettes."""
    with tempfile.TemporaryDirectory() as tmp:
        no_bg_svg = _strip_background(Path(svg_path), Path(tmp))
        return _qc_render(no_bg_svg, out_path, size, extra_args=["-fill", "black", "-colorize", "100%"])


def is_pure_black_and_transparent(png_path: Path) -> bool:
    """True if every pixel is either black (any alpha) or fully transparent."""
    result = subprocess.run(
        ["magick", str(png_path), "-format", "%c", "-depth", "8", "histogram:info:"],
        check=True,
        capture_output=True,
        text=True,
    )
    for line in result.stdout.splitlines():
        line = line.strip()
        if not line:
            continue
        # Lines look like: "N: (r,g,b[,a]) #RRGGBB[AA] color-name"
        paren = line.split("(", 1)[1].split(")", 1)[0]
        channels = [c.strip() for c in paren.split(",")]
        rgb = channels[:3]
        alpha = channels[3] if len(channels) > 3 else None
        if alpha is not None and alpha in ("0", "0.0"):
            continue  # fully transparent pixels may be any RGB
        if any(c not in ("0", "0.0") for c in rgb):
            return False
    return True


def overlay(svg_path: Path, variant: str, out_path: Path, size: int = 1024, opacity: int = 50) -> Path:
    """QC mode (spec §37 / brief): render composited over its reference
    crop at ~50% opacity, for eyeballing alignment/silhouette drift."""
    out_path = Path(out_path)
    out_path.parent.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory() as tmp:
        raw = render_one(Path(svg_path), size, Path(tmp) / "raw.png")
        base = Path(tmp) / "base.png"
        subprocess.run(
            ["magick", str(reference_path(variant)), "-resize", f"{size}x{size}", str(base)],
            check=True,
            capture_output=True,
        )
        subprocess.run(
            [
                "magick",
                str(base),
                str(raw),
                "-compose",
                "dissolve",
                "-define",
                f"compose:args={opacity}",
                "-composite",
                str(out_path),
            ],
            check=True,
            capture_output=True,
        )
    return out_path


# The concept sheet's page background, sampled outside the icon squares.
# A reference crop containing any of this color at its outer edges means
# the crop bbox missed and dragged in page margin.
PAGE_BACKGROUND_RGB = (246, 246, 247)
# Tight on purpose: the page background is a near-neutral off-white and the
# light-mode icon's own background is a warm cream close in brightness but
# with a real R/G/B spread (e.g. ~240,232,226). A loose fuzz here would
# flag the icon's legitimate background as "page margin".
PAGE_BACKGROUND_FUZZ = 10


def pixel_rgba(png_path: Path, x: int, y: int) -> tuple:
    """(r, g, b, a) of one pixel, 0-255 each, via `magick`'s `fx:` operator.
    fx always reports normalized 0-1 floats regardless of the PNG's actual
    bit depth (a fully binary alpha channel -- as ImageMagick sometimes
    writes for a flat silhouette -- would otherwise round-trip through the
    `pixel:p{}}` format at 1-bit precision and misreport as 0/1)."""
    result = subprocess.run(
        [
            "magick",
            str(png_path),
            "-alpha",
            "on",
            "-format",
            f"%[fx:p{{{x},{y}}}.r] %[fx:p{{{x},{y}}}.g] %[fx:p{{{x},{y}}}.b] %[fx:p{{{x},{y}}}.a]",
            "info:",
        ],
        check=True,
        capture_output=True,
        text=True,
    )
    r, g, b, a = (round(float(v) * 255) for v in result.stdout.split())
    return (r, g, b, a)


def pixel_rgb(png_path: Path, x: int, y: int) -> tuple:
    """(r, g, b) of one pixel, via `magick`."""
    return pixel_rgba(png_path, x, y)[:3]


def has_no_page_background_margin(
    png_path: Path,
    page_bg=PAGE_BACKGROUND_RGB,
    fuzz: int = PAGE_BACKGROUND_FUZZ,
) -> bool:
    """True if none of the four mid-row/mid-col edge pixels match the
    concept sheet's page background -- i.e. the crop bbox is exact and
    didn't drag in a strip of page margin on any side."""
    width, height = png_dimensions(png_path)
    mid_x, mid_y = width // 2, height // 2
    edge_points = [
        (mid_x, 0),  # top-mid
        (mid_x, height - 1),  # bottom-mid
        (0, mid_y),  # left-mid
        (width - 1, mid_y),  # right-mid
    ]
    for x, y in edge_points:
        r, g, b = pixel_rgb(png_path, x, y)
        if all(abs(c - bg) <= fuzz for c, bg in zip((r, g, b), page_bg)):
            return False
    return True


def mean_pixel_difference(png_a: Path, png_b: Path) -> float:
    """Fraction (0-1) of pixels that differ at all between two same-size
    PNGs -- ImageMagick's normalized AE ("absolute error count") metric,
    via `magick compare`. This is a differing-pixel fraction, not a true
    per-channel mean absolute error; it's a cheap "are these visibly
    different" signal, not a perceptual distance. Non-zero means visibly
    different; `compare` exits non-zero whenever any difference is found,
    so its return code is not itself an error here."""
    result = subprocess.run(
        ["magick", "compare", "-metric", "AE", str(png_a), str(png_b), "null:"],
        capture_output=True,
        text=True,
    )
    # AE reports "<absolute pixel count> (<normalized 0-1>)" on stderr.
    stderr = result.stderr.strip()
    if "(" in stderr:
        return float(stderr.split("(")[-1].rstrip(")"))
    return float(stderr)


def _qc_render(svg_path: Path, out_path: Path, size: int, extra_args) -> Path:
    out_path = Path(out_path)
    out_path.parent.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory() as tmp:
        raw = render_one(Path(svg_path), size, Path(tmp) / "raw.png")
        subprocess.run(
            ["magick", str(raw), *extra_args, str(out_path)],
            check=True,
            capture_output=True,
        )
    return out_path


def _add_common_args(parser, *, needs_variant=False, needs_size=True):
    parser.add_argument("svg", type=Path, help="path to the source SVG")
    if needs_variant:
        parser.add_argument("--variant", required=True, choices=("light", "dark"))
    if needs_size:
        parser.add_argument("--size", type=int, default=1024)
    parser.add_argument("--out", type=Path, required=True, help="output PNG path")


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(
        prog="icon_harness.py",
        description="Render + QC harness for the app icon SVG (#60). "
        "Each mode is one subcommand; see design/icon/README.md.",
    )
    sub = parser.add_subparsers(dest="command", required=True)

    p_render = sub.add_parser("render", help="rasterize at the full 1024/128/64/32/16 ladder")
    p_render.add_argument("svg", type=Path)
    p_render.add_argument("--out-dir", type=Path, required=True)

    p_sheet = sub.add_parser("contact-sheet", help="one image: five sizes + matching reference crop")
    _add_common_args(p_sheet, needs_variant=True, needs_size=False)

    p_actual = sub.add_parser(
        "actual-size-sheet", help="one image: 64/32/24/16px at their own true pixel size, no upscaling (#65)"
    )
    p_actual.add_argument("svg", type=Path)
    p_actual.add_argument("--out", type=Path, required=True)

    p_gray = sub.add_parser("grayscale", help="spec §48 monochrome QC render")
    _add_common_args(p_gray)

    p_blur = sub.add_parser("blur", help="spec §49 ~8px blur QC render")
    _add_common_args(p_blur)
    p_blur.add_argument("--radius", type=int, default=8)

    p_sil = sub.add_parser("silhouette", help="spec §50 all-black silhouette QC render")
    _add_common_args(p_sil)

    p_overlay = sub.add_parser("overlay", help="~50%% opacity overlay on the reference crop")
    _add_common_args(p_overlay, needs_variant=True)
    p_overlay.add_argument("--opacity", type=int, default=50)

    args = parser.parse_args(argv)

    if args.command == "render":
        paths = render(args.svg, args.out_dir)
        for size, path in sorted(paths.items()):
            print(f"{size}px -> {path}")
    elif args.command == "contact-sheet":
        out = contact_sheet(args.svg, args.variant, args.out)
        print(out)
    elif args.command == "actual-size-sheet":
        out = actual_size_sheet(args.svg, args.out)
        print(out)
    elif args.command == "grayscale":
        out = grayscale(args.svg, args.out, size=args.size)
        print(out)
    elif args.command == "blur":
        out = blur(args.svg, args.out, size=args.size, radius=args.radius)
        print(out)
    elif args.command == "silhouette":
        out = silhouette(args.svg, args.out, size=args.size)
        print(out)
    elif args.command == "overlay":
        out = overlay(args.svg, args.variant, args.out, size=args.size, opacity=args.opacity)
        print(out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
