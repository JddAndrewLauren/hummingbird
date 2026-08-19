package net.twinion.hummingbird

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Text
import androidx.compose.ui.Alignment
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/** #588 item 3's regression gate — the second measuring test in this
 * module, in `ChoiceRowWrappingTest`'s pattern (whose header carries the
 * reasoning: presence-and-wiring tests cannot catch a layout defect, and
 * `@GraphicsMode(NATIVE)` is what makes the measurements real; do not drop
 * it).
 *
 * The defect: `FacetChipGroup`'s label row used `CenterVertically` against
 * a wrapping `FlowRow`. Context is the one facet whose vocabulary is live,
 * and with enough distinct contexts its chips wrap to a second line — the
 * `Context` label then floats vertically centred *between* the two chip
 * rows, unlike `Size`/`Energy`/`Urgency`, whose fixed three-value rows
 * never wrap. The fix is `Alignment.Top` plus a baseline offset that seats
 * the label beside the first chip line whatever the row's height.
 *
 * The negative control renders the defect's own shape and asserts the
 * label *does* float below the first line there — the same
 * prove-the-teeth discipline `ChoiceRowWrappingTest` established, so a
 * widened qualifier or a chip set too short to wrap cannot leave this file
 * green while measuring nothing.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(
    sdk = [35],
    qualifiers = "w320dp-h800dp",
    application = android.app.Application::class,
)
class FacetLabelAlignmentTest {

    @get:Rule
    val composeTestRule = createComposeRule()

    // Enough distinct contexts that the chip row must wrap at 320dp — the
    // hardware sighting's own shape (five-plus live contexts).
    private val wrappingContexts =
        listOf("@home", "@computer", "@phone", "@errands", "@garden", "@waiting")

    /** The first chip line's vertical extent, recovered from the first
     * chip's own label node: the text sits centred in a 32dp
     * `FilterChip`, so the line spans its centre ± 16dp. */
    private fun firstChipLine(): ClosedRange<Dp> {
        val text = composeTestRule.onNodeWithText(wrappingContexts.first())
            .getUnclippedBoundsInRoot()
        val centre = (text.top + text.bottom) / 2
        return (centre - 16.dp)..(centre + 16.dp)
    }

    private fun labelCentre(): Dp {
        val label = composeTestRule.onNodeWithText("Context").getUnclippedBoundsInRoot()
        return (label.top + label.bottom) / 2
    }

    @Test
    fun `the facet label sits beside the first chip line when the chips wrap`() {
        composeTestRule.setContent {
            FacetChipGroup(
                label = "Context",
                facet = FrontierFacet.CONTEXT,
                values = wrappingContexts,
                selected = emptySet(),
                onToggle = { _, _ -> },
            )
        }

        val line = firstChipLine()
        val centre = labelCentre()
        assertTrue(
            "the Context label must sit beside the first chip line " +
                "(label centre $centre, first line ${line.start}..${line.endInclusive})",
            centre in line,
        )
    }

    @Test
    fun `a centred label against a wrapping FlowRow is what the defect looked like`() {
        composeTestRule.setContent {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Text("Context")
                FlowRow(
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    for (value in wrappingContexts) {
                        FilterChip(
                            selected = false,
                            onClick = {},
                            label = { Text(value) },
                        )
                    }
                }
            }
        }

        val line = firstChipLine()
        val centre = labelCentre()
        assertTrue(
            "a centred label at 320dp must float below the first chip line — otherwise " +
                "the chips did not wrap and this file's other test measures nothing " +
                "(label centre $centre, first line ${line.start}..${line.endInclusive})",
            centre > line.endInclusive,
        )
    }
}
