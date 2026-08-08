"""Tests for the icon generator (#61): geometry + palette data -> master
light/dark SVGs.

`python3 -m unittest discover -s tests`.
"""

import sys
import tempfile
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO_ROOT / "scripts"))

import icon_generator  # noqa: E402

SVG_NS = "http://www.w3.org/2000/svg"


def local(tag: str) -> str:
    """Strip the XML namespace off an ElementTree tag."""
    return tag.split("}", 1)[-1] if "}" in tag else tag


class GenerateTest(unittest.TestCase):
    def test_emits_a_light_and_a_dark_master_svg(self):
        with tempfile.TemporaryDirectory() as tmp:
            out_dir = Path(tmp)
            paths = icon_generator.generate(out_dir)

            self.assertEqual(sorted(paths.keys()), ["dark", "light"])
            for variant, path in paths.items():
                self.assertTrue(path.exists(), f"missing {variant} SVG at {path}")

    def test_each_master_is_a_1024_viewbox_svg_document(self):
        with tempfile.TemporaryDirectory() as tmp:
            paths = icon_generator.generate(Path(tmp))
            for variant, path in paths.items():
                with self.subTest(variant=variant):
                    root = ET.parse(path).getroot()
                    self.assertEqual(local(root.tag), "svg")
                    self.assertEqual(root.get("viewBox"), "0 0 1024 1024")
                    self.assertEqual(root.get("width"), "1024")
                    self.assertEqual(root.get("height"), "1024")


ALLOWED_ELEMENTS = {
    "svg",
    "g",
    "path",
    "polygon",
    "ellipse",
    "circle",
    "rect",
    "defs",
    "linearGradient",
    "radialGradient",
    "clipPath",
    "stop",  # required child of a gradient; not itself visible content
}

PROHIBITED_ELEMENTS = {"filter", "image", "mask", "script", "foreignObject", "text"}


class SelfContainedTest(unittest.TestCase):
    def test_uses_only_spec_permitted_elements(self):
        with tempfile.TemporaryDirectory() as tmp:
            paths = icon_generator.generate(Path(tmp))
            for variant, path in paths.items():
                with self.subTest(variant=variant):
                    root = ET.parse(path).getroot()
                    tags = {local(el.tag) for el in root.iter()}
                    self.assertTrue(tags.issubset(ALLOWED_ELEMENTS), tags - ALLOWED_ELEMENTS)
                    self.assertFalse(tags & PROHIBITED_ELEMENTS)

    def test_has_no_external_references(self):
        # The xmlns declaration itself is required boilerplate, not an
        # external resource load -- only href/url() references count.
        with tempfile.TemporaryDirectory() as tmp:
            paths = icon_generator.generate(Path(tmp))
            for variant, path in paths.items():
                with self.subTest(variant=variant):
                    svg_text = path.read_text()
                    self.assertNotIn("xlink:href", svg_text)
                    self.assertNotIn('href="http', svg_text)
                    self.assertNotIn("url(http", svg_text)


class SemanticIdTest(unittest.TestCase):
    def test_top_level_and_mass_groups_carry_semantic_ids(self):
        with tempfile.TemporaryDirectory() as tmp:
            paths = icon_generator.generate(Path(tmp))
            for variant, path in paths.items():
                with self.subTest(variant=variant):
                    root = ET.parse(path).getroot()
                    ids = {el.get("id") for el in root.iter() if el.get("id")}
                    for expected in (
                        "icon",
                        "background",
                        "bird",
                        "bird-silhouette",
                        "crown-base",
                        "gorget-base",
                        "chest-base",
                        "side-body-base",
                    ):
                        self.assertIn(expected, ids)


class DarkDiffersOnlyByPaletteTest(unittest.TestCase):
    def test_geometry_is_identical_between_light_and_dark(self):
        with tempfile.TemporaryDirectory() as tmp:
            paths = icon_generator.generate(Path(tmp))
            light_root = ET.parse(paths["light"]).getroot()
            dark_root = ET.parse(paths["dark"]).getroot()

            def geometry_by_id(root):
                out = {}
                for el in root.iter():
                    element_id = el.get("id")
                    if not element_id:
                        continue
                    out[element_id] = (el.get("d"), el.get("points"))
                return out

            self.assertEqual(geometry_by_id(light_root), geometry_by_id(dark_root))

    def test_at_least_one_mass_color_differs_between_light_and_dark(self):
        with tempfile.TemporaryDirectory() as tmp:
            paths = icon_generator.generate(Path(tmp))
            light_text = paths["light"].read_text()
            dark_text = paths["dark"].read_text()
            self.assertNotEqual(light_text, dark_text)
            self.assertIn(icon_generator.LIGHT_PALETTE["crown_mass"], light_text)
            self.assertIn(icon_generator.DARK_PALETTE["crown_mass"], dark_text)
            self.assertNotIn(icon_generator.DARK_PALETTE["crown_mass"], light_text)


class CommittedMastersUpToDateTest(unittest.TestCase):
    """The committed design/icon/hummingbird-icon-master-*.svg files are
    generated artifacts under a "never hand-edit" rule -- they must be
    byte-identical to what a fresh generator run produces, or the
    committed source of truth has silently drifted from the generator."""

    def test_committed_masters_match_a_fresh_generate(self):
        committed = icon_generator.OUTPUT_NAMES
        for variant, name in committed.items():
            with self.subTest(variant=variant):
                committed_path = icon_generator.DEFAULT_OUT_DIR / name
                self.assertTrue(committed_path.exists(), f"missing committed {committed_path}")
                committed_text = committed_path.read_text()
                fresh_text = icon_generator._build_svg(icon_generator.PALETTES[variant])
                self.assertEqual(
                    committed_text,
                    fresh_text,
                    f"{committed_path} is stale -- regenerate with "
                    "`python3 scripts/icon_generator.py --out-dir design/icon`",
                )


class RasterizesTest(unittest.TestCase):
    def test_both_masters_rasterize_via_the_harness(self):
        import shutil

        if shutil.which("resvg") is None:
            self.skipTest("resvg not on PATH")
        sys.path.insert(0, str(REPO_ROOT / "scripts"))
        import icon_harness

        with tempfile.TemporaryDirectory() as tmp:
            tmp_dir = Path(tmp)
            paths = icon_generator.generate(tmp_dir)
            for variant, svg_path in paths.items():
                with self.subTest(variant=variant):
                    png = icon_harness.render_one(svg_path, 1024, tmp_dir / f"{variant}.png")
                    self.assertTrue(png.exists())
                    self.assertEqual(icon_harness.png_dimensions(png), (1024, 1024))


if __name__ == "__main__":
    unittest.main()
