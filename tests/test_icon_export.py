"""Tests for the icon export matrix + platform packaging (#66).

`python3 -m unittest discover -s tests`.

Structural tests (favicon shape budget, Android safe-zone landmark math,
committed-SVG staleness) run unconditionally, pure Python. Render/pack
tests exercise the real `resvg`/`magick`/`iconutil` binaries (no mocking,
same convention as tests/test_icon_harness.py) and skip themselves if a
binary isn't on PATH.
"""

import shutil
import subprocess
import sys
import tempfile
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO_ROOT / "scripts"))

import icon_export  # noqa: E402
import icon_generator  # noqa: E402
import icon_harness  # noqa: E402

REQUIRED_TOOLS = ("resvg", "magick")
MISSING_TOOLS = [tool for tool in REQUIRED_TOOLS if shutil.which(tool) is None]
MISSING_ICONUTIL = shutil.which("iconutil") is None


def local(tag: str) -> str:
    return tag.split("}", 1)[-1] if "}" in tag else tag


# ---------------------------------------------------------------------------
# PNG export matrix (spec §41)
# ---------------------------------------------------------------------------


class ExportMatrixSourceTest(unittest.TestCase):
    def test_matrix_covers_1024_down_to_16(self):
        self.assertEqual(
            sorted(icon_export.EXPORT_MATRIX.keys()),
            [16, 24, 32, 48, 64, 128, 256, 512, 1024],
        )

    def test_large_sizes_source_from_master(self):
        for size in (1024, 512, 256, 128):
            with self.subTest(size=size):
                self.assertEqual(icon_export.EXPORT_MATRIX[size], "master")

    def test_mid_sizes_source_from_small(self):
        for size in (64, 48, 32):
            with self.subTest(size=size):
                self.assertEqual(icon_export.EXPORT_MATRIX[size], "small")

    def test_small_sizes_source_from_micro(self):
        for size in (24, 16):
            with self.subTest(size=size):
                self.assertEqual(icon_export.EXPORT_MATRIX[size], "micro")

    def test_source_svg_path_resolves_to_a_committed_file(self):
        for size in icon_export.EXPORT_MATRIX:
            for variant in ("light", "dark"):
                with self.subTest(size=size, variant=variant):
                    path = icon_export.source_svg_path(size, variant)
                    self.assertTrue(path.exists(), f"missing {path}")


@unittest.skipIf(MISSING_TOOLS, f"missing harness binaries: {MISSING_TOOLS}")
class PngMatrixTest(unittest.TestCase):
    def test_renders_every_matrix_size_at_correct_dimensions(self):
        with tempfile.TemporaryDirectory() as tmp:
            paths = icon_export.render_png_matrix("light", Path(tmp))
            self.assertEqual(sorted(paths.keys()), sorted(icon_export.EXPORT_MATRIX.keys()))
            for size, path in paths.items():
                with self.subTest(size=size):
                    self.assertTrue(path.exists())
                    self.assertEqual(icon_harness.png_dimensions(path), (size, size))

    def test_renders_are_srgb_8bit_rgba_no_icc_profile(self):
        with tempfile.TemporaryDirectory() as tmp:
            paths = icon_export.render_png_matrix("dark", Path(tmp), sizes=(1024, 32, 16))
            for size, path in paths.items():
                with self.subTest(size=size):
                    self.assertEqual(icon_export.png_bit_depth(path), 8)
                    self.assertTrue(icon_export.png_has_alpha(path))
                    self.assertTrue(icon_export.png_has_no_icc_profile(path))

    def test_64px_is_visibly_different_from_a_master_downscale(self):
        """The §41 matrix's 64px tile must come from the small optical
        source, not a straight master downscale -- a real optical
        difference (facet counts differ, spec §24), not just a filename
        convention."""
        with tempfile.TemporaryDirectory() as tmp:
            tmp_dir = Path(tmp)
            matrix_paths = icon_export.render_png_matrix("light", tmp_dir, sizes=(64,))
            master_svg = icon_export.source_svg_path(1024, "light")
            master_downscale = icon_harness.render_one(master_svg, 64, tmp_dir / "master-downscale-64.png")
            diff = icon_harness.mean_pixel_difference(matrix_paths[64], master_downscale)
            self.assertGreater(diff, 0.01, "64px matrix tile is indistinguishable from a master downscale")


# ---------------------------------------------------------------------------
# macOS .icns (spec §42)
# ---------------------------------------------------------------------------


@unittest.skipIf(MISSING_TOOLS, f"missing harness binaries: {MISSING_TOOLS}")
@unittest.skipIf(MISSING_ICONUTIL, "iconutil not on PATH (macOS only)")
class IcnsTest(unittest.TestCase):
    def test_builds_an_icns_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            out_path = icon_export.build_icns("light", Path(tmp) / "hummingbird-icon-light.icns")
            self.assertTrue(out_path.exists())
            self.assertGreater(out_path.stat().st_size, 0)

    def test_icns_round_trips_through_iconutil_with_every_apple_size(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_dir = Path(tmp)
            icns_path = icon_export.build_icns("dark", tmp_dir / "hummingbird-icon-dark.icns")
            roundtrip_dir = tmp_dir / "roundtrip.iconset"
            subprocess.run(
                ["iconutil", "-c", "iconset", str(icns_path), "-o", str(roundtrip_dir)],
                check=True,
                capture_output=True,
            )
            produced = {p.name for p in roundtrip_dir.iterdir()}
            expected = {filename for filename, _ in icon_export.ICNS_ICONSET_ENTRIES}
            self.assertEqual(produced, expected)


# ---------------------------------------------------------------------------
# Windows .ico (spec §43): 16/24/32 must be micro/small artwork, not a
# master downscale.
# ---------------------------------------------------------------------------


@unittest.skipIf(MISSING_TOOLS, f"missing harness binaries: {MISSING_TOOLS}")
class IcoTest(unittest.TestCase):
    def test_builds_an_ico_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            out_path = icon_export.build_ico("light", Path(tmp) / "hummingbird-icon-light.ico")
            self.assertTrue(out_path.exists())
            self.assertGreater(out_path.stat().st_size, 0)

    def test_16_24_32_entries_match_micro_small_artwork(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_dir = Path(tmp)
            ico_path = icon_export.build_ico("light", tmp_dir / "hummingbird-icon-light.ico")
            for size in (16, 24, 32):
                with self.subTest(size=size):
                    extracted = icon_export.extract_ico_frame(ico_path, size, tmp_dir / f"extracted-{size}.png")
                    expected_svg = icon_export.source_svg_path(size, "light")
                    expected = icon_harness.render_one(expected_svg, size, tmp_dir / f"expected-{size}.png")
                    diff = icon_harness.mean_pixel_difference(extracted, expected)
                    self.assertLess(diff, 0.02, f"{size}px .ico entry doesn't match its micro/small source render")

    def test_16px_entry_differs_from_a_master_downscale(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_dir = Path(tmp)
            ico_path = icon_export.build_ico("light", tmp_dir / "hummingbird-icon-light.ico")
            extracted = icon_export.extract_ico_frame(ico_path, 16, tmp_dir / "extracted-16.png")
            master_svg = icon_export.source_svg_path(1024, "light")
            master_downscale = icon_harness.render_one(master_svg, 16, tmp_dir / "master-downscale-16.png")
            diff = icon_harness.mean_pixel_difference(extracted, master_downscale)
            self.assertGreater(diff, 0.01, "16px .ico entry looks like a master downscale, not micro artwork")


# ---------------------------------------------------------------------------
# Favicon (spec §45)
# ---------------------------------------------------------------------------


class FaviconTest(unittest.TestCase):
    def test_is_a_1024_viewbox_svg_document(self):
        svg_text = icon_export.build_favicon_svg()
        root = ET.fromstring(svg_text)
        self.assertEqual(local(root.tag), "svg")
        self.assertEqual(root.get("viewBox"), "0 0 1024 1024")

    def test_at_most_fifteen_visible_shapes(self):
        svg_text = icon_export.build_favicon_svg()
        count = icon_export.visible_shape_count(svg_text)
        # Pinned to the real count (background-silhouette, chest-base,
        # side-body-base, gorget-base, crown-base, eye-stripe, eye-outer,
        # eye-highlight-primary, beak-main) -- not just "under the spec §45
        # cap", so a regression that starts double-counting (e.g. <defs>
        # clipPath shapes leaking back in) fails loudly instead of hiding
        # under a loose upper bound.
        self.assertEqual(count, 9, f"favicon shape count changed to {count}; update this pinned figure if intentional")
        self.assertLessEqual(count, 15, f"favicon has {count} visible shapes, spec §45 caps at ~15")

    def test_visible_shape_count_excludes_defs_clip_path_shapes(self):
        # Regression guard for the bug the reviewer caught: <defs> holds
        # clipPath <path>/<polygon> children that share tag names with
        # real artwork but never render. Confirm the favicon's own <defs>
        # really does contain shape-tagged elements, and that they don't
        # get counted.
        svg_text = icon_export.build_favicon_svg()
        root = ET.fromstring(svg_text)
        defs = next(el for el in root.iter() if local(el.tag) == "defs")
        defs_shape_tags = [local(el.tag) for el in defs.iter() if local(el.tag) in icon_export._VISIBLE_SHAPE_TAGS]
        self.assertGreater(len(defs_shape_tags), 0, "test fixture assumption broke: favicon <defs> has no shape-tagged children")
        self.assertEqual(icon_export.visible_shape_count(svg_text), 9)

    def test_uses_spec_45s_literal_color_list(self):
        svg_text = icon_export.build_favicon_svg()
        palette = icon_generator.LIGHT_PALETTE
        self.assertIn(palette["crown_mass"], svg_text)  # gray head
        self.assertIn(icon_generator.BEAK_MAIN_FILL, svg_text)  # black beak
        self.assertIn(icon_generator.EYE_OUTER_FILL, svg_text)  # black eye
        self.assertIn(palette["gorget_mass"], svg_text)  # orange throat
        self.assertIn(palette["chest_mass"], svg_text)  # cream chest

    def test_prioritizes_orange_throat_and_beak_over_facet_detail(self):
        """Spec §45: "prioritize the bird's orange throat and diagonal
        beak" -- the favicon profile drops every facet/feather overlay
        (crown/chest/side-body/gorget) but keeps the beak and gorget-base
        mass shapes."""
        for key in ("crown_facets", "chest_facets", "side_body_facets", "gorget_rows"):
            self.assertEqual(icon_export.FAVICON_PROFILE[key], [])
        svg_text = icon_export.build_favicon_svg()
        self.assertIn('id="beak-main"', svg_text)
        self.assertIn('id="gorget-base"', svg_text)

    @unittest.skipIf(MISSING_TOOLS, f"missing harness binaries: {MISSING_TOOLS}")
    def test_renders_cleanly_at_actual_16px(self):
        with tempfile.TemporaryDirectory() as tmp:
            svg_path = Path(tmp) / "favicon.svg"
            svg_path.write_text(icon_export.build_favicon_svg())
            out_path = icon_harness.render_one(svg_path, 16, Path(tmp) / "favicon-16.png")
            self.assertEqual(icon_harness.png_dimensions(out_path), (16, 16))


class CommittedFaviconUpToDateTest(unittest.TestCase):
    def test_committed_favicon_matches_a_fresh_build(self):
        committed_path = icon_export.DEFAULT_ICON_DIR / icon_export.FAVICON_OUTPUT_NAME
        self.assertTrue(committed_path.exists(), f"missing committed {committed_path}")
        self.assertEqual(
            committed_path.read_text(),
            icon_export.build_favicon_svg(),
            f"{committed_path} is stale -- regenerate with `python3 scripts/icon_export.py svgs`",
        )


# ---------------------------------------------------------------------------
# Android adaptive-icon layers (spec §44)
# ---------------------------------------------------------------------------


class AndroidSafeZoneTest(unittest.TestCase):
    def test_foreground_scale_is_within_the_spec_44_8_to_12_percent_band(self):
        self.assertGreaterEqual(icon_export.ANDROID_FOREGROUND_SCALE, 0.88)
        self.assertLessEqual(icon_export.ANDROID_FOREGROUND_SCALE, 0.92)

    def test_head_identity_landmarks_clear_the_safe_zone_after_scaling(self):
        for name, point in icon_generator.LANDMARKS.items():
            if name in icon_export.SAFE_ZONE_EXEMPT_LANDMARKS:
                continue
            with self.subTest(landmark=name):
                self.assertTrue(
                    icon_export.landmark_within_safe_zone(point),
                    f"{name}={point} falls outside Android's safe-zone circle after the §44 shrink",
                )
        self.assertTrue(icon_export.landmark_within_safe_zone(icon_generator.EYE_CENTER))

    def test_beak_tip_spike_is_the_documented_bleed_exception(self):
        # Confirms the exemption is real (not silently masking a bug): the
        # beak-tip landmarks genuinely fail the safe-zone check even at
        # the spec's own scale.
        for name in icon_export.SAFE_ZONE_EXEMPT_LANDMARKS:
            point = icon_generator.LANDMARKS[name]
            with self.subTest(landmark=name):
                self.assertFalse(icon_export.landmark_within_safe_zone(point))

    def test_unscaled_landmark_would_fail_without_the_shrink(self):
        # crown_top is the landmark the §44 shrink is calibrated against --
        # it clears the safe zone only after scaling, not before.
        crown_top = icon_generator.LANDMARKS["crown_top"]
        self.assertFalse(icon_export.landmark_within_safe_zone(crown_top, scale=1.0))
        self.assertTrue(icon_export.landmark_within_safe_zone(crown_top))

    @unittest.skipIf(MISSING_TOOLS, f"missing harness binaries: {MISSING_TOOLS}")
    def test_the_landmark_check_does_not_mean_full_pixel_containment(self):
        """The named-landmark check above is real but narrow -- it only
        covers 8 points standing in for head/eye/gorget identity. This
        renders the actual foreground.svg and measures every opaque pixel
        against the same circle: a large, genuine fraction (roughly a
        third to a half -- chest and side-body are entirely outside,
        gorget bleeds both sides) renders outside the safe zone. This is
        the accepted tradeoff (see the module docstring in
        scripts/icon_export.py and design/icon/README.md), not a bug --
        this test exists so that tradeoff can't silently drift into an
        unnoticed regression (e.g. a future edit that claims full
        containment without re-measuring)."""
        fraction = icon_export.foreground_opaque_pixel_outside_safe_zone_fraction()
        self.assertGreater(fraction, 0.25, "measured outside-safe-zone fraction dropped a lot -- update the docs/PR claim if this is real")
        self.assertLess(fraction, 0.55, "measured outside-safe-zone fraction rose a lot -- update the docs/PR claim if this is real")


class AndroidLayersTest(unittest.TestCase):
    def test_background_is_a_1024_viewbox_svg_with_no_bird(self):
        svg_text = icon_export.build_android_background_svg()
        root = ET.fromstring(svg_text)
        self.assertEqual(root.get("viewBox"), "0 0 1024 1024")
        self.assertNotIn('id="bird"', svg_text)
        self.assertIn('id="background"', svg_text)

    def test_foreground_has_no_background_rect_and_a_transformed_bird_group(self):
        svg_text = icon_export.build_android_foreground_svg()
        self.assertNotIn('id="background"', svg_text)
        root = ET.fromstring(svg_text)
        bird_group = next(el for el in root.iter() if el.get("id") == "bird")
        self.assertIn("scale(0.9", bird_group.get("transform"))

    def test_foreground_carries_full_master_detail(self):
        # Spec §44 gives no reduced shape budget for Android -- the
        # foreground should be the master's full facet/feather detail.
        svg_text = icon_export.build_android_foreground_svg()
        self.assertIn('id="chest-facet-01"', svg_text)
        self.assertIn('id="gorget-top-row"', svg_text)

    @unittest.skipIf(MISSING_TOOLS, f"missing harness binaries: {MISSING_TOOLS}")
    def test_layers_render_cleanly(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_dir = Path(tmp)
            bg_path = tmp_dir / "background.svg"
            fg_path = tmp_dir / "foreground.svg"
            bg_path.write_text(icon_export.build_android_background_svg())
            fg_path.write_text(icon_export.build_android_foreground_svg())
            bg_png = icon_harness.render_one(bg_path, 1024, tmp_dir / "background.png")
            fg_png = icon_harness.render_one(fg_path, 1024, tmp_dir / "foreground.png")
            self.assertEqual(icon_harness.png_dimensions(bg_png), (1024, 1024))
            self.assertEqual(icon_harness.png_dimensions(fg_png), (1024, 1024))


class CommittedAndroidLayersUpToDateTest(unittest.TestCase):
    def test_committed_layers_match_a_fresh_build(self):
        android_dir = icon_export.DEFAULT_ICON_DIR / "android"
        background_path = android_dir / icon_export.BACKGROUND_OUTPUT_NAME
        foreground_path = android_dir / icon_export.FOREGROUND_OUTPUT_NAME
        self.assertTrue(background_path.exists())
        self.assertTrue(foreground_path.exists())
        self.assertEqual(background_path.read_text(), icon_export.build_android_background_svg())
        self.assertEqual(foreground_path.read_text(), icon_export.build_android_foreground_svg())


# ---------------------------------------------------------------------------
# Committed QC evidence (design/icon/qc/favicon-actual-16.png,
# qc/android-adaptive-safe-zone.png): regenerate with
# `python3 scripts/icon_export.py qc`, same convention as the harness's
# own qc/*.png (grayscale/blur/silhouette/overlay/contact-sheet, each with
# a documented one-command regeneration path).
# ---------------------------------------------------------------------------


@unittest.skipIf(MISSING_TOOLS, f"missing harness binaries: {MISSING_TOOLS}")
class QcRenderTest(unittest.TestCase):
    def test_favicon_qc_render_is_a_nearest_neighbor_upscaled_16px_image(self):
        with tempfile.TemporaryDirectory() as tmp:
            out_path = icon_export.build_favicon_qc_render(Path(tmp) / "favicon-actual-16.png")
            self.assertTrue(out_path.exists())
            self.assertEqual(icon_harness.png_dimensions(out_path), (256, 256))

    def test_android_safe_zone_qc_render_shows_the_composited_layers(self):
        with tempfile.TemporaryDirectory() as tmp:
            out_path = icon_export.build_android_safe_zone_qc_render(Path(tmp) / "android-adaptive-safe-zone.png")
            self.assertTrue(out_path.exists())
            self.assertEqual(icon_harness.png_dimensions(out_path), (512, 512))

    def test_cli_qc_subcommand_writes_both_files(self):
        with tempfile.TemporaryDirectory() as tmp:
            icon_dir = Path(tmp) / "icon"
            icon_dir.mkdir()
            icon_generator.generate_all(icon_dir)
            result = subprocess.run(
                [
                    sys.executable,
                    str(REPO_ROOT / "scripts" / "icon_export.py"),
                    "--icon-dir",
                    str(icon_dir),
                    "qc",
                ],
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertTrue((icon_dir / "qc" / icon_export.FAVICON_QC_NAME).exists())
            self.assertTrue((icon_dir / "qc" / icon_export.ANDROID_SAFE_ZONE_QC_NAME).exists())


class CommittedQcRendersUpToDateTest(unittest.TestCase):
    """The committed design/icon/qc/favicon-actual-16.png and
    qc/android-adaptive-safe-zone.png are generated gate evidence, same
    "never hand-edit" rule as the SVGs -- regenerate with
    `python3 scripts/icon_export.py qc` whenever the favicon/Android
    source SVGs change."""

    @unittest.skipIf(MISSING_TOOLS, f"missing harness binaries: {MISSING_TOOLS}")
    def test_committed_favicon_qc_render_matches_a_fresh_build(self):
        committed_path = icon_export.DEFAULT_ICON_DIR / "qc" / icon_export.FAVICON_QC_NAME
        self.assertTrue(committed_path.exists(), f"missing committed {committed_path}")
        with tempfile.TemporaryDirectory() as tmp:
            fresh_path = icon_export.build_favicon_qc_render(Path(tmp) / icon_export.FAVICON_QC_NAME)
            self.assertEqual(
                committed_path.read_bytes(),
                fresh_path.read_bytes(),
                f"{committed_path} is stale -- regenerate with `python3 scripts/icon_export.py qc`",
            )

    @unittest.skipIf(MISSING_TOOLS, f"missing harness binaries: {MISSING_TOOLS}")
    def test_committed_android_safe_zone_qc_render_matches_a_fresh_build(self):
        committed_path = icon_export.DEFAULT_ICON_DIR / "qc" / icon_export.ANDROID_SAFE_ZONE_QC_NAME
        self.assertTrue(committed_path.exists(), f"missing committed {committed_path}")
        with tempfile.TemporaryDirectory() as tmp:
            fresh_path = icon_export.build_android_safe_zone_qc_render(Path(tmp) / icon_export.ANDROID_SAFE_ZONE_QC_NAME)
            self.assertEqual(
                committed_path.read_bytes(),
                fresh_path.read_bytes(),
                f"{committed_path} is stale -- regenerate with `python3 scripts/icon_export.py qc`",
            )


# ---------------------------------------------------------------------------
# One documented command (last acceptance box)
# ---------------------------------------------------------------------------


@unittest.skipIf(MISSING_TOOLS, f"missing harness binaries: {MISSING_TOOLS}")
@unittest.skipIf(MISSING_ICONUTIL, "iconutil not on PATH (macOS only)")
class AllCommandTest(unittest.TestCase):
    def test_cli_all_subcommand_regenerates_every_export(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_dir = Path(tmp)
            icon_dir = tmp_dir / "icon"
            export_dir = tmp_dir / "export"
            icon_dir.mkdir()
            icon_generator.generate_all(icon_dir)

            result = subprocess.run(
                [
                    sys.executable,
                    str(REPO_ROOT / "scripts" / "icon_export.py"),
                    "--icon-dir",
                    str(icon_dir),
                    "all",
                    "--out-dir",
                    str(export_dir),
                ],
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)

            self.assertTrue((icon_dir / icon_export.FAVICON_OUTPUT_NAME).exists())
            self.assertTrue((icon_dir / "android" / "background.svg").exists())
            self.assertTrue((icon_dir / "android" / "foreground.svg").exists())
            for variant in ("light", "dark"):
                self.assertTrue((export_dir / "icns" / f"hummingbird-icon-{variant}.icns").exists())
                self.assertTrue((export_dir / "ico" / f"hummingbird-icon-{variant}.ico").exists())
                for size in icon_export.EXPORT_MATRIX:
                    self.assertTrue(
                        (export_dir / "png" / variant / f"hummingbird-icon-{variant}-{size}.png").exists()
                    )


if __name__ == "__main__":
    unittest.main()
