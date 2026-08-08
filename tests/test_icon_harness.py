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


if __name__ == "__main__":
    unittest.main()
