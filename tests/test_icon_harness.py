"""Tests for the icon render + QC harness (#60).

`python3 -m unittest discover -s tests`.

These exercise the harness through its actual subprocess-backed public
functions (resvg + ImageMagick must be on PATH) rather than mocking the
tools out -- the whole point of the harness is that those binaries do the
work, so a test that mocks them proves nothing about the real path.
"""

import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO_ROOT / "scripts"))

import icon_harness  # noqa: E402

STUB_SVG = REPO_ROOT / "design" / "icon" / "stub.svg"
REFERENCE_DIR = REPO_ROOT / "design" / "icon" / "reference"

REQUIRED_TOOLS = ("resvg", "magick")
MISSING_TOOLS = [tool for tool in REQUIRED_TOOLS if shutil.which(tool) is None]


@unittest.skipIf(MISSING_TOOLS, f"missing harness binaries: {MISSING_TOOLS}")
class RenderTest(unittest.TestCase):
    def test_renders_all_five_sizes_at_actual_pixel_dimensions(self):
        with tempfile.TemporaryDirectory() as tmp:
            out_dir = Path(tmp)
            paths = icon_harness.render(STUB_SVG, out_dir)

            self.assertEqual(sorted(paths.keys()), sorted(icon_harness.CONTACT_SHEET_SIZES))
            for size, path in paths.items():
                self.assertTrue(path.exists(), f"missing render for {size}px")
                self.assertEqual(icon_harness.png_dimensions(path), (size, size))


@unittest.skipIf(MISSING_TOOLS, f"missing harness binaries: {MISSING_TOOLS}")
class ContactSheetTest(unittest.TestCase):
    def test_produces_one_image_with_five_sizes_plus_reference(self):
        with tempfile.TemporaryDirectory() as tmp:
            out_path = Path(tmp) / "contact-sheet.png"
            icon_harness.contact_sheet(STUB_SVG, variant="light", out_path=out_path)

            self.assertTrue(out_path.exists())
            width, height = icon_harness.png_dimensions(out_path)
            # One row: five rendered sizes + one reference crop.
            self.assertGreater(width, height * 5)

    def test_small_renders_upscale_nearest_neighbor_but_the_1024_tile_does_not(self):
        # Small sizes must stay honestly blocky (point/nearest-neighbor);
        # downscaling the full 1024 render to a smaller display tile should
        # use a quality filter instead, or it aliases real detail.
        display_size = icon_harness.CONTACT_SHEET_DISPLAY_SIZE
        self.assertEqual(icon_harness.tile_filter(16, display_size), "point")
        self.assertEqual(icon_harness.tile_filter(32, display_size), "point")
        self.assertNotEqual(icon_harness.tile_filter(1024, display_size), "point")


@unittest.skipIf(MISSING_TOOLS, f"missing harness binaries: {MISSING_TOOLS}")
class GrayscaleTest(unittest.TestCase):
    def test_produces_a_desaturated_render(self):
        with tempfile.TemporaryDirectory() as tmp:
            out_path = Path(tmp) / "grayscale.png"
            icon_harness.grayscale(STUB_SVG, out_path=out_path, size=1024)

            self.assertTrue(out_path.exists())
            self.assertEqual(icon_harness.png_dimensions(out_path), (1024, 1024))
            self.assertTrue(icon_harness.is_grayscale(out_path))


@unittest.skipIf(MISSING_TOOLS, f"missing harness binaries: {MISSING_TOOLS}")
class BlurTest(unittest.TestCase):
    def test_produces_a_blurred_render_that_differs_from_the_sharp_one(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_dir = Path(tmp)
            sharp = icon_harness.render_one(STUB_SVG, 1024, tmp_dir / "sharp.png")
            blurred = icon_harness.blur(STUB_SVG, out_path=tmp_dir / "blur.png", size=1024)

            self.assertTrue(blurred.exists())
            self.assertEqual(icon_harness.png_dimensions(blurred), (1024, 1024))
            # A ~8px blur must actually change pixel data at a hard edge.
            self.assertGreater(icon_harness.mean_pixel_difference(sharp, blurred), 0)


@unittest.skipIf(MISSING_TOOLS, f"missing harness binaries: {MISSING_TOOLS}")
class SilhouetteTest(unittest.TestCase):
    def test_flattens_every_opaque_fill_to_black(self):
        with tempfile.TemporaryDirectory() as tmp:
            out_path = Path(tmp) / "silhouette.png"
            icon_harness.silhouette(STUB_SVG, out_path=out_path, size=1024)

            self.assertTrue(out_path.exists())
            self.assertEqual(icon_harness.png_dimensions(out_path), (1024, 1024))
            self.assertTrue(icon_harness.is_pure_black_and_transparent(out_path))

    def test_preserves_the_stub_transparent_margin(self):
        # STUB_SVG's opaque rect only covers x/y 128..896; outside that is
        # transparent canvas. A silhouette that flattened alpha away too
        # would turn that margin solid black instead of leaving it clear.
        with tempfile.TemporaryDirectory() as tmp:
            out_path = Path(tmp) / "silhouette.png"
            icon_harness.silhouette(STUB_SVG, out_path=out_path, size=1024)

            r, g, b, a = icon_harness.pixel_rgba(out_path, 10, 10)
            self.assertEqual(a, 0, "corner of the transparent margin should stay transparent")
            r, g, b, a = icon_harness.pixel_rgba(out_path, 512, 512)
            self.assertEqual((r, g, b), (0, 0, 0))
            self.assertGreater(a, 0, "center of the opaque rect should stay opaque")


@unittest.skipIf(MISSING_TOOLS, f"missing harness binaries: {MISSING_TOOLS}")
class OverlayTest(unittest.TestCase):
    def test_composites_render_over_reference_at_half_opacity(self):
        with tempfile.TemporaryDirectory() as tmp:
            out_path = Path(tmp) / "overlay.png"
            icon_harness.overlay(STUB_SVG, variant="dark", out_path=out_path, size=1024)

            self.assertTrue(out_path.exists())
            self.assertEqual(icon_harness.png_dimensions(out_path), (1024, 1024))
            # Blended output must differ from both the reference alone and
            # the sharp render alone -- i.e. it's a genuine composite.
            reference = icon_harness.reference_path("dark")
            sharp = icon_harness.render_one(STUB_SVG, 1024, Path(tmp) / "sharp.png")
            self.assertGreater(icon_harness.mean_pixel_difference(reference, out_path), 0)
            self.assertGreater(icon_harness.mean_pixel_difference(sharp, out_path), 0)


@unittest.skipIf(MISSING_TOOLS, f"missing harness binaries: {MISSING_TOOLS}")
class CliTest(unittest.TestCase):
    def test_contact_sheet_subcommand_runs_from_the_command_line(self):
        with tempfile.TemporaryDirectory() as tmp:
            out_path = Path(tmp) / "contact-sheet.png"
            result = subprocess.run(
                [
                    sys.executable,
                    str(REPO_ROOT / "scripts" / "icon_harness.py"),
                    "contact-sheet",
                    str(STUB_SVG),
                    "--variant",
                    "light",
                    "--out",
                    str(out_path),
                ],
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertTrue(out_path.exists())


@unittest.skipIf(MISSING_TOOLS, f"missing harness binaries: {MISSING_TOOLS}")
class ReferenceCropTest(unittest.TestCase):
    """Regression for a mis-crop: dark-1024.png once carried a strip of
    concept-sheet page background (#F6F6F7) down its right edge, squashing
    the icon inside the 1024x1024 canvas. Both reference crops must be
    exact -- they're the alignment target for overlay/contact-sheet in
    every downstream slice."""

    def test_light_and_dark_crops_have_no_page_background_margin(self):
        for variant in ("light", "dark"):
            with self.subTest(variant=variant):
                path = icon_harness.reference_path(variant)
                self.assertTrue(
                    icon_harness.has_no_page_background_margin(path),
                    f"{path} has a page-background margin on at least one edge",
                )


if __name__ == "__main__":
    unittest.main()
