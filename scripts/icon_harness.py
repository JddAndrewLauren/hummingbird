#!/usr/bin/env python3
"""Render + QC harness for the app icon SVG slices (#60).

Wraps `resvg` (rasterize) and ImageMagick's `magick` (compositing/QC) so
every later icon slice (#61-#66) iterates against the same, deterministic
render path. No browser dependency. See design/icon/README.md for the
documented one-command-per-mode usage and tool dependencies.
"""

import argparse
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
REFERENCE_DIR = REPO_ROOT / "design" / "icon" / "reference"

# Fixed, deterministic size ladder for the contact sheet / render matrix.
CONTACT_SHEET_SIZES = (1024, 128, 64, 32, 16)


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


def _resize_point(src: Path, dst: Path, display_size: int) -> Path:
    """Nearest-neighbor resize -- keeps small renders honestly blocky."""
    dst.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        [
            "magick",
            str(src),
            "-filter",
            "point",
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
            tile = _resize_point(renders[size], tmp_dir / f"tile-{size}.png", display_size)
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


def silhouette(svg_path: Path, out_path: Path, size: int = 1024) -> Path:
    """QC mode (spec §50): every opaque fill flattened to black, alpha kept."""
    return _qc_render(svg_path, out_path, size, extra_args=["-fill", "black", "-colorize", "100%"])


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


def mean_pixel_difference(png_a: Path, png_b: Path) -> float:
    """Normalized (0-1) mean absolute-error pixel difference between two
    same-size PNGs, via `magick compare`. Non-zero means "visibly different";
    `compare` exits non-zero whenever a difference is found, so its return
    code is not itself an error here."""
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
