package net.twinion.hummingbird

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
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
import net.twinion.hummingbird.ui.forms.CaptureDateField
import net.twinion.hummingbird.ui.forms.PriorityRow
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

/** The priority row's one-line gate (operator decision 2026-08-20: "get all
 * the priority options on a single line").
 *
 * `AxisRowWrappingTest`'s rig and both of its halves: it **measures** the
 * real [PriorityRow] at the narrowest width the app ships to, and it
 * **proves the measurement has teeth** by measuring the five-chip row this
 * replaced — the same chips plus "No priority" — and asserting that one
 * does *not* fit. Without the control, shortening a label or widening the
 * budget would leave this file green while measuring nothing, and the
 * control is also the record of why the fifth chip had to go rather than
 * merely being unwanted: four fit the budget, five do not.
 *
 * [PriorityRow] is a fixed `Row` now, so anything over budget is *clipped*,
 * silently, and the chip at the trailing edge is Low. A structural pin sees
 * four present, wired chips either way.
 *
 * `@GraphicsMode(NATIVE)` is load-bearing exactly as it is there: legacy
 * graphics stubs text measurement and returns near-identical widths for
 * every string. So is [HummingbirdTheme] around the content — a bare render
 * resolves `MaterialTheme.typography` to Material's defaults rather than
 * the app's bundled faces, which is how round 4 measured a strip green that
 * clipped on the device.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(
    sdk = [35],
    // The Fold's cover display — the narrowest surface this app ships to,
    // measured on hardware 2026-08-20 as 1080px at density 390.
    qualifiers = "w443dp-h800dp",
    application = android.app.Application::class,
)
class PriorityRowWrappingTest {

    @get:Rule
    val composeTestRule = createComposeRule()

    // 443dp less the 24dp gutters both capture surfaces pad with — the
    // width the row gets, not the screen's. The Triage editor renders the
    // same component inside a card and has less; that surface is the reason
    // the budget is stated here rather than assumed to be the whole screen.
    private val contentWidth = 395.dp

    // Wide enough that nothing is squeezed, so the chips report the width
    // they actually want. A `Row` rendered at exactly its budget squeezes
    // the trailing child instead of overflowing, so bounds taken there come
    // back inside the budget however badly the row overflows — measuring
    // unconstrained is the only form of this assertion that can fail.
    private val unconstrained = 2000.dp

    private val labels = listOf("Urgent", "High", "Medium", "Low")

    @Test
    fun `every priority chip fits one line inside the narrowest content width`() {
        composeTestRule.setContent {
            HummingbirdTheme {
                Box(modifier = Modifier.width(unconstrained)) {
                    PriorityRow(selected = "", onSelect = {})
                }
            }
        }

        val bounds = labels.associateWith {
            composeTestRule.onNodeWithText(it).getUnclippedBoundsInRoot()
        }
        val trailing = bounds.values.maxOf { it.right }
        assertTrue(
            "the priority row wants $trailing and only has $contentWidth — a fixed Row " +
                "clips whatever does not fit " +
                "(${bounds.entries.joinToString { "${it.key}=${it.value.width}" }})",
            trailing <= contentWidth,
        )
        val tops = bounds.values.map { it.top }.distinct()
        assertEquals("every chip must share one line (tops: $tops)", 1, tops.size)
    }

    @Test
    fun `the fifth chip is what did not fit`() {
        composeTestRule.setContent {
            HummingbirdTheme {
                Box(modifier = Modifier.width(unconstrained)) {
                    FiveChipPriorityRow()
                }
            }
        }

        val trailing = (labels + "No priority").maxOf {
            composeTestRule.onNodeWithText(it).getUnclippedBoundsInRoot().right
        }
        assertTrue(
            "the row this replaced must overflow $contentWidth — otherwise the test above " +
                "would pass with the fifth chip still in it, and dropping it would be a " +
                "taste decision rather than the one-line decision it was (measured $trailing)",
            trailing > contentWidth,
        )
    }

    @Test
    fun `PriorityRow neither wraps nor offers a no-priority chip`() {
        // The source half. The measurement above is taken at one width, and
        // a `FlowRow` would satisfy it while restoring the two-line row the
        // operator asked to be rid of.
        val src = repoFile(
            "client/android/app/src/main/kotlin/net/twinion/hummingbird/ui/forms/PriorityRow.kt",
        ).replace(Regex("""/\*[\s\S]*?\*/"""), "")
            .replace(Regex("""(?m)^\s*//.*$"""), "")
        assertFalse(
            "PriorityRow must not wrap — one line is the whole shape",
            src.contains("FlowRow("),
        )
        assertFalse(
            "there is no chip for the absence of a priority — the resting state says it",
            src.contains("No priority"),
        )
    }

    /** The other one-line change of the same batch: deadline and scheduled
     * date share a row, `weight(1f)` each.
     *
     * **What this measures, and what it deliberately does not.** A weight
     * *bounds* each field at half the row, so unlike the chips these cannot
     * overflow — the claim worth pinning is that they are side by side and
     * evenly split, which is exactly what a stray `fillMaxWidth()` or a
     * dropped weight would break. It does **not** pin that
     * "Scheduled date" renders whole inside its half: `onNodeWithText` on an
     * `OutlinedTextField` returns the *field's* bounds, not the label's
     * (measured here as 197x64dp — the field, at its own height), so a
     * truncated label is invisible to this substrate. The first draft of
     * this test asserted it anyway and failed against its own field
     * heights. On the Fold's cover display the labels do render whole
     * (accessibility tree, 2026-08-20: Deadline `[98,1241]`, Scheduled date
     * `[589,1241]`) — that is hardware evidence, and it is the only kind
     * this particular claim has.
     *
     * Note also that this measures **at** the budget, the reverse of the
     * rule the chip tests follow, and the reason the two differ is worth
     * keeping straight: measure unconstrained when the container squeezes
     * (the overflow would be hidden inside the bounds), measure at the
     * budget when the container bounds (the split *is* the question).
     */
    @Test
    fun `the two dates split the narrowest row evenly, side by side`() {
        composeTestRule.setContent {
            HummingbirdTheme {
                Row(modifier = Modifier.width(contentWidth)) {
                    CaptureDateField(
                        label = "Deadline",
                        value = "",
                        error = null,
                        onValueChange = {},
                        modifier = Modifier.weight(1f),
                    )
                    CaptureDateField(
                        label = "Scheduled date",
                        value = "",
                        error = null,
                        onValueChange = {},
                        modifier = Modifier.weight(1f),
                    )
                }
            }
        }

        val bounds = listOf("Deadline", "Scheduled date").associateWith {
            composeTestRule.onNodeWithText(it).getUnclippedBoundsInRoot()
        }
        val tops = bounds.values.map { it.top }.distinct()
        assertEquals("both dates must share one line (tops: $tops)", 1, tops.size)
        // Evenly, and neither crushed: a field that lost its weight takes
        // the whole row and leaves its sibling nothing.
        for ((label, box) in bounds) {
            assertTrue(
                "$label must get about half of $contentWidth, not ${box.width} " +
                    "(${bounds.entries.joinToString { "${it.key}=${it.value.width}" }})",
                box.width > contentWidth / 3 && box.width < contentWidth * 2 / 3,
            )
        }
    }

    /** The control for the pair above: the same two fields at full width
     * each, which is what stacking them meant. Their combined height is
     * what one line saves, and measuring it is what keeps the assertion
     * above from being a claim about nothing. */
    @Test
    fun `stacking the dates is what costs the second row`() {
        composeTestRule.setContent {
            HummingbirdTheme {
                Box(modifier = Modifier.width(contentWidth)) {
                    Row(modifier = Modifier.fillMaxWidth()) {
                        CaptureDateField(
                            label = "Deadline",
                            value = "",
                            error = null,
                            onValueChange = {},
                        )
                    }
                }
            }
        }
        val single = composeTestRule.onNodeWithText("Deadline")
            .getUnclippedBoundsInRoot()
        assertTrue(
            "a full-width date field must span more than half the row — otherwise " +
                "seating two of them side by side saved nothing (measured ${single.width})",
            single.width > contentWidth / 2,
        )
    }

    /** The row as it stood before 2026-08-20: the same four chips plus "No
     * priority", in the `FlowRow` that let them take two lines. Not a copy
     * of anything shipped — the control the test above needs, kept in this
     * file so it cannot drift back into production. */
    @Composable
    private fun FiveChipPriorityRow() {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            for (label in labels + "No priority") {
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
