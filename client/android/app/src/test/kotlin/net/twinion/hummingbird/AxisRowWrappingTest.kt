package net.twinion.hummingbird

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.width
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.height
import androidx.compose.ui.unit.width
import java.io.File
import net.twinion.hummingbird.ui.theme.HummingbirdTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.hummingbird_ffi_mobile.MobileFrontierAxis

/** The axis strip's shrink-to-fit gate (operator decision 2026-08-20).
 *
 * `ChoiceRowWrappingTest`'s rig, for the same reason and with the same two
 * halves: it **measures** the real `AxisRow` at the narrowest width the app
 * ships to, and it **proves the measurement has teeth** by rendering the
 * same five chips at `FilterChip`'s default size and asserting they do
 * *not* fit — without that control, a widened qualifier or a shortened
 * label set would leave this file green while measuring nothing.
 *
 * The failure this exists for is not the vertical letter column
 * `ChoiceRow` guards; it is the opposite. `AxisRow` is a fixed single-line
 * `Row` now — no scroll, no wrap — so anything that does not fit is
 * *clipped*, silently, and the clipped chip is the Filter disclosure at the
 * trailing edge: the only door to an active filter, hidden with no sign
 * that it is there. Nothing else in the repo can catch that: a structural
 * pin sees five present, wired chips.
 *
 * `@GraphicsMode(NATIVE)` is load-bearing exactly as it is there — legacy
 * graphics stubs text measurement and returns near-identical widths for
 * every string, so the numbers below would be fiction. `captureToImage()`
 * is never used: it times out under Robolectric even in NATIVE mode, which
 * is why this file measures bounds instead of pixels.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(
    sdk = [35],
    qualifiers = "w320dp-h800dp",
    application = android.app.Application::class,
)
class AxisRowWrappingTest {

    @get:Rule
    val composeTestRule = createComposeRule()

    // 320dp — the narrowest width this repo tests at
    // (`ChoiceRowWrappingTest`'s own qualifier) — less `NowScreen`'s 24dp
    // gutters. The width the strip gets, not the screen's.
    private val contentWidth = 272.dp

    // Wide enough that nothing is squeezed, so the chips report the width
    // they actually want. See the first test for why that matters.
    private val unconstrained = 2000.dp

    // Every label the strip renders: `AXIS_LABEL`'s four, plus the Filter
    // chip with a count in it (the wider of its two states, so the test
    // measures the worse case rather than the resting one).
    private val labels = listOf("Context", "Project", "Size", "Energy", "Filter · 2")

    @Test
    fun `the whole axis strip fits one line inside the narrowest content width`() {
        // Measured **unconstrained**, and that is the load-bearing part of
        // this rig. Rendered at exactly 272dp the `Row` squeezes whatever
        // runs out of width, so the trailing chip's bounds come back inside
        // the budget no matter how badly it overflows — the first draft of
        // this test passed with the Filter chip measuring 272dp..272dp,
        // i.e. crushed to nothing. Measuring what the strip *wants* is the
        // only form of this assertion that can fail.
        composeTestRule.setContent {
            HummingbirdTheme {
            Box(modifier = Modifier.width(unconstrained)) {
                AxisRow(
                    axis = MobileFrontierAxis.CONTEXT,
                    onPick = {},
                    filtersOpen = false,
                    facetCount = 2,
                    onToggleFilters = {},
                )
            }
            }
        }

        val bounds = labels.associateWith {
            composeTestRule.onNodeWithText(it).getUnclippedBoundsInRoot()
        }
        val trailing = bounds.values.maxOf { it.right }
        println("MEASURED compact trailing=$trailing widths=" + bounds.entries.joinToString { "${it.key}=${it.value.width}" })
        assertTrue(
            "the strip wants ${trailing} and only has $contentWidth — a fixed Row clips " +
                "whatever does not fit, and the chip at the trailing edge is the Filter " +
                "disclosure, the only door to an active filter " +
                "(${bounds.entries.joinToString { "${it.key}=${it.value.width}" }})",
            trailing <= contentWidth,
        )
        // One line: every chip shares a top. A wrap would be a different
        // defect from a clip, and this is the assertion that tells them
        // apart.
        val tops = bounds.values.map { it.top }.distinct()
        assertEquals("every chip must share one line (tops: $tops)", 1, tops.size)
    }

    @Test
    fun `the default chip size is what does not fit`() {
        // The control, measured the same unconstrained way. `FilterChip`'s
        // own size — 32dp of horizontal chrome per chip, inflated to a 48dp
        // interactive box — is what `AxisChip` and the waived minimum
        // target exist to escape. If this ever starts fitting, the test
        // above has stopped proving anything and the compact treatment can
        // go back to being a plain `FilterChip`.
        composeTestRule.setContent {
            HummingbirdTheme {
                Box(modifier = Modifier.width(unconstrained)) {
                    DefaultSizedAxisRow()
                }
            }
        }

        val trailing = labels.maxOf {
            composeTestRule.onNodeWithText(it).getUnclippedBoundsInRoot().right
        }
        println("MEASURED default trailing=$trailing")
        assertTrue(
            "five default-sized FilterChips must overflow $contentWidth — otherwise " +
                "`the whole axis strip fits` would pass with or without the compact " +
                "treatment (measured $trailing)",
            trailing > contentWidth,
        )
    }

    @Test
    fun `AxisRow neither scrolls nor wraps`() {
        // The source half. The measurement above is taken at one width, and
        // a `horizontalScroll` would satisfy it while restoring the very
        // gesture the operator removed — the chips would fit because the
        // strip would be as wide as it liked.
        val src = repoFile(
            "client/android/app/src/main/kotlin/net/twinion/hummingbird/NowScreen.kt",
        ).replace(Regex("""/\*[\s\S]*?\*/"""), "")
            .replace(Regex("""(?m)^\s*//.*$"""), "")
        val body = Regex("""internal fun AxisRow\([\s\S]*?\n}""")
            .find(src)
            ?.value
            ?: error("could not locate AxisRow in NowScreen.kt")
        assertFalse(
            "AxisRow must not scroll — the chips shrink to fit instead",
            body.contains("horizontalScroll("),
        )
        assertFalse(
            "AxisRow must not wrap — one line is the whole shape",
            body.contains("FlowRow("),
        )
    }

    /** The strip as it was before the compact treatment: default-sized
     * `FilterChip`s at 8dp spacing, the same five labels. Not a copy of
     * anything shipped — the control the test above needs, kept in this
     * file so nothing can drift into production. */
    @Composable
    private fun DefaultSizedAxisRow() {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            for (label in labels) {
                FilterChip(selected = false, onClick = {}, label = { Text(label) })
            }
        }
    }

    private fun repoFile(relative: String): String {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val file = File(root, relative)
        check(file.isFile) { "$relative not found under $root" }
        return file.readText()
    }
}
