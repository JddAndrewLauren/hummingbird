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


def elements_by_id(root):
    return {el.get("id"): el for el in root.iter() if el.get("id")}


def document_order_ids(root):
    return [el.get("id") for el in root.iter() if el.get("id")]


class HeadIdentityTest(unittest.TestCase):
    """Head identity (#62, spec fidelity priorities 1-5, §38): beak planes,
    eye, eye stripe, crown facets, forehead patch, cheek separator."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.paths = icon_generator.generate(Path(self.tmp.name))
        self.roots = {variant: ET.parse(p).getroot() for variant, p in self.paths.items()}

    def test_beak_is_three_to_five_shapes_not_one_triangle(self):
        for variant, root in self.roots.items():
            with self.subTest(variant=variant):
                ids = elements_by_id(root)
                beak_ids = [i for i in ids if i.startswith("beak-")]
                self.assertGreaterEqual(len(beak_ids), 3)
                self.assertLessEqual(len(beak_ids), 5)

    def test_beak_highlight_strip_is_narrow_and_translucent(self):
        for variant, root in self.roots.items():
            with self.subTest(variant=variant):
                ids = elements_by_id(root)
                strip = ids["beak-highlight"]
                opacity = float(strip.get("opacity"))
                self.assertTrue(0.60 <= opacity <= 0.75)

    def test_beak_uses_no_flat_black(self):
        for variant, path in self.paths.items():
            with self.subTest(variant=variant):
                text = path.read_text()
                self.assertNotIn("#000000", text)
                self.assertNotIn("#000\"", text)

    def test_crown_has_ten_to_sixteen_facets(self):
        for variant, root in self.roots.items():
            with self.subTest(variant=variant):
                ids = elements_by_id(root)
                facet_ids = [i for i in ids if i.startswith("crown-facet-")]
                self.assertGreaterEqual(len(facet_ids), 10)
                self.assertLessEqual(len(facet_ids), 16)

    def test_crown_facets_use_only_gray_palette(self):
        gray_hexes = set(icon_generator.CROWN_GRAYS)
        for variant, root in self.roots.items():
            with self.subTest(variant=variant):
                ids = elements_by_id(root)
                for facet_id in (i for i in ids if i.startswith("crown-facet-")):
                    self.assertIn(ids[facet_id].get("fill"), gray_hexes)

    def test_forehead_patch_is_three_to_five_restrained_shapes(self):
        for variant, root in self.roots.items():
            with self.subTest(variant=variant):
                ids = elements_by_id(root)
                forehead_ids = [i for i in ids if i.startswith("forehead-")]
                self.assertGreaterEqual(len(forehead_ids), 3)
                self.assertLessEqual(len(forehead_ids), 5)
                for forehead_id in forehead_ids:
                    self.assertIn(ids[forehead_id].get("fill"), icon_generator.FOREHEAD_ORANGES)

    def test_eye_has_ring_iris_and_two_highlights(self):
        for variant, root in self.roots.items():
            with self.subTest(variant=variant):
                ids = elements_by_id(root)
                for expected in (
                    "eye-outer",
                    "eye-iris",
                    "eye-highlight-primary",
                    "eye-highlight-secondary",
                ):
                    self.assertIn(expected, ids)
                outer_rx = float(ids["eye-outer"].get("rx"))
                highlight_rx = float(ids["eye-highlight-primary"].get("rx"))
                self.assertLess(highlight_rx, outer_rx)

    def test_eye_stripe_is_a_dark_wedge_not_pure_black(self):
        for variant, root in self.roots.items():
            with self.subTest(variant=variant):
                ids = elements_by_id(root)
                self.assertIn("eye-stripe", ids)
                self.assertIn("eye-stripe-secondary", ids)
                self.assertNotEqual(ids["eye-stripe"].get("fill"), "#000000")

    def test_cheek_separator_is_a_filled_shape_not_a_stroke(self):
        for variant, root in self.roots.items():
            with self.subTest(variant=variant):
                ids = elements_by_id(root)
                cheek = ids["cheek-separator"]
                self.assertEqual(local(cheek.tag), "path")
                self.assertIsNotNone(cheek.get("fill"))
                self.assertIsNone(cheek.get("stroke"))

    def test_layer_order_follows_spec_section_34(self):
        for variant, root in self.roots.items():
            with self.subTest(variant=variant):
                order = document_order_ids(root)

                def idx(element_id):
                    return order.index(element_id)

                self.assertLess(idx("crown-base"), idx("crown-facet-01"))
                self.assertLess(idx("crown-facet-01"), idx("forehead-01"))
                self.assertLess(idx("forehead-01"), idx("eye-stripe"))
                self.assertLess(idx("eye-stripe"), idx("eye-stripe-secondary"))
                # Spec §34 explicitly allows eye-stripe/crown ordering
                # adjustments; cheek-separator is raised above eye-stripe
                # (and kept below the eye group) so it's actually visible
                # instead of buried under every mass drawn after it.
                self.assertLess(idx("eye-stripe-secondary"), idx("cheek-separator"))
                self.assertLess(idx("cheek-separator"), idx("eye-outer"))
                self.assertLess(idx("eye-outer"), idx("eye-iris"))
                self.assertLess(idx("eye-iris"), idx("eye-highlight-primary"))
                self.assertLess(idx("eye-highlight-primary"), idx("beak-main"))


class GorgetFeatherTest(unittest.TestCase):
    """Gorget feather system (#63, spec §14-18): the flat gorget block is
    replaced by one base under-shape plus ~5 overlapping rows of a seeded,
    jittered rounded-shield primitive."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.paths = icon_generator.generate(Path(self.tmp.name))
        self.roots = {variant: ET.parse(p).getroot() for variant, p in self.paths.items()}

    def _feather_elements(self, root):
        return [el for el in root.iter() if (el.get("id") or "").startswith("gorget-") and local(el.tag) == "path" and el.get("id") != "gorget-base"]

    def test_gorget_base_under_shape_still_present(self):
        for variant, root in self.roots.items():
            with self.subTest(variant=variant):
                ids = elements_by_id(root)
                self.assertIn("gorget-base", ids)
                self.assertEqual(local(ids["gorget-base"].tag), "path")

    def test_master_has_35_to_45_feathers(self):
        for variant, root in self.roots.items():
            with self.subTest(variant=variant):
                feathers = self._feather_elements(root)
                self.assertGreaterEqual(len(feathers), 35)
                self.assertLessEqual(len(feathers), 45)

    def test_five_rows_with_spec_section_16_counts(self):
        expected_ranges = {
            "gorget-top-row": (5, 6),
            "gorget-row-2": (7, 8),
            "gorget-row-3": (8, 9),
            "gorget-row-4": (7, 8),
            "gorget-bottom-row": (5, 6),
        }
        for variant, root in self.roots.items():
            with self.subTest(variant=variant):
                ids = elements_by_id(root)
                for row_id, (low, high) in expected_ranges.items():
                    self.assertIn(row_id, ids)
                    count = len([i for i in ids if i.startswith(row_id + "-")])
                    self.assertGreaterEqual(count, low, row_id)
                    self.assertLessEqual(count, high, row_id)

    def test_no_two_feather_instances_are_identical(self):
        for variant, root in self.roots.items():
            with self.subTest(variant=variant):
                feathers = self._feather_elements(root)
                d_values = [el.get("d") for el in feathers]
                self.assertEqual(len(d_values), len(set(d_values)))

    def test_no_outlines_between_feathers(self):
        for variant, root in self.roots.items():
            with self.subTest(variant=variant):
                for el in self._feather_elements(root):
                    self.assertIsNone(el.get("stroke"))

    def test_row_sizes_grow_toward_the_bottom_row(self):
        widths = [sum(w) / 2 for w in icon_generator.ROW_WIDTH_RANGES]
        heights = [sum(h) / 2 for h in icon_generator.ROW_HEIGHT_RANGES]
        self.assertEqual(widths, sorted(widths))
        self.assertEqual(heights, sorted(heights))

    def test_row_overlap_measured_from_generated_geometry(self):
        # Independent of the placement formula (review on #63/PR #87 flagged
        # the previous version of this test as tautological -- it recomputed
        # the same constants the generator used to place the rows). This
        # measures each row's *actual* aggregate (y_min, y_max) across its
        # generated feathers -- real post-jitter/rotation geometry -- and
        # asserts consecutive rows genuinely overlap (share a nonzero y-band,
        # not just an idealized center-to-center gap), by a fraction of the
        # smaller row's own real span roughly in the spec §17 neighborhood.
        rows = icon_generator.GORGET_FEATHER_ROWS
        for i in range(len(rows) - 1):
            upper_min, upper_max = icon_generator.row_y_extent(rows[i])
            lower_min, lower_max = icon_generator.row_y_extent(rows[i + 1])
            intersection = upper_max - lower_min
            self.assertGreater(intersection, 0, f"{rows[i]['id']}/{rows[i + 1]['id']} do not overlap at all")
            smaller_span = min(upper_max - upper_min, lower_max - lower_min)
            overlap_fraction = intersection / smaller_span
            self.assertGreaterEqual(overlap_fraction, 0.15, rows[i]["id"])
            self.assertLessEqual(overlap_fraction, 0.95, rows[i]["id"])

    def test_gorget_feathers_clipped_to_the_base_envelope(self):
        # Review on #63/PR #87: 9/37 feathers (4 entirely) rendered outside
        # gorget-base, reading as detached color blobs. The feather group is
        # clipped to GORGET_MASS_PATH (same pattern as #62's crown-clip), so
        # no feather geometry -- however far a seeded instance overshoots --
        # can render past the mass's own silhouette.
        for variant, root in self.roots.items():
            with self.subTest(variant=variant):
                ids = elements_by_id(root)
                feather_group = ids["gorget-feathers"]
                self.assertEqual(feather_group.get("clip-path"), "url(#gorget-clip)")
                clip_paths = [el for el in root.iter() if local(el.tag) == "clippath" or local(el.tag) == "clipPath"]
                gorget_clip = next(el for el in clip_paths if el.get("id") == "gorget-clip")
                clip_shape = next(iter(gorget_clip))
                self.assertEqual(clip_shape.get("d"), icon_generator.GORGET_MASS_PATH)

    def test_no_feather_fill_matches_either_variants_base_gorget_fill(self):
        # Review on #63/PR #87: 6 feathers used #FF8500, exactly
        # LIGHT_PALETTE's gorget-base fill, so they were invisible against
        # their own base (§17 requires separation from color contrast).
        base_fills = {icon_generator.LIGHT_PALETTE["gorget_mass"], icon_generator.DARK_PALETTE["gorget_mass"]}
        for fill in icon_generator.GORGET_FEATHER_RAMP:
            self.assertNotIn(fill, base_fills)
        for row in icon_generator.GORGET_FEATHER_ROWS:
            for feather in row["feathers"]:
                self.assertNotIn(feather["fill"], base_fills)

    def test_color_distribution_follows_spec_section_18_directional_map(self):
        ramp = icon_generator.GORGET_FEATHER_RAMP
        # Scarlet zone (spec §18): X 620-740, Y 480-650 -- expect a color
        # near the red/deep-red end of the ramp there.
        scarlet_index = ramp.index("#B62E22")
        # Warmest zone (spec §18): X 250-420, Y 500-700 -- expect a color
        # near the yellow-orange end of the ramp there.
        warm_index = ramp.index("#FFA000")
        self.assertGreater(scarlet_index, warm_index)

        for variant, root in self.roots.items():
            with self.subTest(variant=variant):
                fills_in_scarlet_zone = []
                fills_in_warm_zone = []
                for row in icon_generator.GORGET_FEATHER_ROWS:
                    for feather in row["feathers"]:
                        cx, cy = feather["center"]
                        if 620 <= cx <= 740 and 480 <= cy <= 650:
                            fills_in_scarlet_zone.append(ramp.index(feather["fill"]))
                        if 250 <= cx <= 420 and 500 <= cy <= 700:
                            fills_in_warm_zone.append(ramp.index(feather["fill"]))
                self.assertTrue(fills_in_scarlet_zone, "no feathers sampled in the scarlet zone")
                self.assertTrue(fills_in_warm_zone, "no feathers sampled in the warm zone")
                avg_scarlet = sum(fills_in_scarlet_zone) / len(fills_in_scarlet_zone)
                avg_warm = sum(fills_in_warm_zone) / len(fills_in_warm_zone)
                self.assertGreater(avg_scarlet, avg_warm)

    def test_regenerating_with_unchanged_parameters_is_byte_stable(self):
        with tempfile.TemporaryDirectory() as tmp_a, tempfile.TemporaryDirectory() as tmp_b:
            paths_a = icon_generator.generate(Path(tmp_a))
            paths_b = icon_generator.generate(Path(tmp_b))
            for variant in paths_a:
                self.assertEqual(paths_a[variant].read_text(), paths_b[variant].read_text())

    def test_layer_order_gorget_rows_sit_between_gorget_base_and_crown(self):
        for variant, root in self.roots.items():
            with self.subTest(variant=variant):
                order = document_order_ids(root)

                def idx(element_id):
                    return order.index(element_id)

                self.assertLess(idx("gorget-base"), idx("gorget-bottom-row"))
                self.assertLess(idx("gorget-bottom-row"), idx("gorget-row-4"))
                self.assertLess(idx("gorget-row-4"), idx("gorget-row-3"))
                self.assertLess(idx("gorget-row-3"), idx("gorget-row-2"))
                self.assertLess(idx("gorget-row-2"), idx("gorget-top-row"))
                self.assertLess(idx("gorget-top-row"), idx("crown-base"))


class GorgetLowerCoverageRenderTest(unittest.TestCase):
    """Review on #63/PR #87: the lower gorget rendered essentially bare
    (50.7% feather coverage overall, 0% in the two lowest 50px bands) --
    the bottom row's y_center didn't reach far enough down. Rasterize the
    real master and sample pixels in the lower gorget/chest-overlap region;
    each must show a feather color (not the flat gorget-base fill showing
    through)."""

    def setUp(self):
        import shutil

        if shutil.which("resvg") is None or shutil.which("magick") is None:
            self.skipTest("resvg/magick not on PATH")
        sys.path.insert(0, str(REPO_ROOT / "scripts"))
        global icon_harness
        import icon_harness

        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.paths = icon_generator.generate(Path(self.tmp.name))

    def test_lower_gorget_shows_feather_color_not_bare_base(self):
        # Points inside the lower gorget / chest-overlap region (spec §16
        # Row 5), picked from the actual Row 4/Row 5 x-ranges and y-centers.
        sample_points = [(455, 600), (435, 650), (435, 685), (500, 700)]
        for variant, svg_path in self.paths.items():
            base_fill = icon_generator.PALETTES[variant]["gorget_mass"]
            png = icon_harness.render_one(svg_path, 1024, Path(self.tmp.name) / f"{variant}-1024.png")
            with self.subTest(variant=variant):
                for x, y in sample_points:
                    r, g, b, a = icon_harness.pixel_rgba(png, x, y)
                    self.assertGreater(a, 0, f"({x},{y}) is transparent -- outside the bird entirely")
                    base_r, base_g, base_b = (int(base_fill.lstrip("#")[i : i + 2], 16) for i in (0, 2, 4))
                    is_bare_base = all(abs(c - bc) <= 6 for c, bc in zip((r, g, b), (base_r, base_g, base_b)))
                    self.assertFalse(is_bare_base, f"({x},{y}) is bare gorget-base color, not a feather")


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
