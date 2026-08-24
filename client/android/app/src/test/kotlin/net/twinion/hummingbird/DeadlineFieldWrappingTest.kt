package net.twinion.hummingbird

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.height
import androidx.compose.ui.unit.width
import net.twinion.hummingbird.ui.forms.CaptureDateField
import net.twinion.hummingbird.ui.forms.DeadlineField
import net.twinion.hummingbird.ui.theme.HummingbirdTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/** The deadline control's one-slot gate — the module's **fifth**
 * layout-measuring test, after `ChoiceRowWrappingTest`,
 * `FacetLabelAlignmentTest`, `AxisRowWrappingTest` and
 * `PriorityRowWrappingTest`, and it borrows their rig unchanged.
 *
 * The dates became pickers on 2026-08-24, which put two new things into a
 * half-width slot that had held plain text: a 24dp trailing icon inside the
 * field, and — when the deadline names a minute — a second control stacked
 * under it. Neither is visible to a structural pin, and both are exactly
 * the kind of thing that measures fine on a wide emulator and clips on the
 * device the app actually ships to.
 *
 * `@GraphicsMode(NATIVE)` is load-bearing for the reason
 * `PriorityRowWrappingTest`'s header states: legacy graphics stubs text
 * measurement and hands back near-identical widths for every string, so
 * every assertion below would pass vacuously without it. So is
 * [HummingbirdTheme] around the content — a bare render resolves
 * `MaterialTheme.typography` to Material's defaults rather than the app's
 * bundled faces, which is how round 4 measured a strip green that clipped
 * on hardware.
 *
 * **The ceiling, stated so nobody mistakes this for coverage of the
 * feature:** the two dialogs are separate windows and are not measured
 * here at all. `captureToImage()` times out under this Robolectric setup
 * (`docs/SURFACES.md`), so bounds are the most this substrate gives. What
 * the pickers actually look like is the operator's hardware run, per that
 * document's 2026-08-20 decision.
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
class DeadlineFieldWrappingTest {

    @get:Rule
    val composeTestRule = createComposeRule()

    // 443dp less the 24dp gutters both capture surfaces pad with.
    private val contentWidth = 395.dp

    // What one half of the paired line is worth, less the 8dp the Row puts
    // between them. Every claim in this file is about fitting inside this.
    private val halfSlot = (395.dp - 8.dp) / 2

    // Wide enough that nothing is squeezed, so a control reports the width
    // it actually wants. A `Row`/`Column` rendered at exactly its budget
    // squeezes its children instead of overflowing, so bounds taken there
    // come back inside the budget however badly it overflows.
    private val unconstrained = 2000.dp

    /** "Add time" is the second gesture's only affordance, and it must be a
     * real touch target. Measured unconstrained, per the rule the other four
     * files keep: rendered at its budget the button would be squeezed into
     * bounds that look like a pass.
     *
     * **Its width is deliberately not asserted.** It measures ~133dp against
     * a $halfSlot slot, and the control below records that even a much longer
     * phrasing still fits — so a width bar here could not fail, and this file
     * does not carry assertions that cannot. What is genuinely tight in this
     * slot is the revealed time control, which the pair test measures. */
    @Test
    fun `the Add time button is a real touch target`() {
        composeTestRule.setContent {
            HummingbirdTheme {
                Box(modifier = Modifier.width(unconstrained)) {
                    DeadlineField(value = "2026-08-24", error = null, onValueChange = {})
                }
            }
        }
        val button = composeTestRule.onNodeWithText("Add time").getUnclippedBoundsInRoot()
        assertTrue(
            "\"Add time\" must stay a 44dp touch target (measured ${button.height})",
            button.height >= 44.dp,
        )
    }

    /** The control that gives the assertion above teeth: a bare
     * `TextButton`, which is what the production one was until this file
     * measured it. Material rests it at 40dp — under the target the rest of
     * this app holds itself to — so `heightIn(min = 44.dp)` in
     * `DeadlineField` is a recorded 4dp rather than a decoration somebody
     * may tidy away. Not a copy of anything shipped: it is kept here so it
     * cannot drift back into production.
     *
     * It doubles as the record that the button's *width* was never the
     * risk — even at this fuller phrasing it comes in under the half slot. */
    @Test
    fun `the bare button is what the touch target buys`() {
        composeTestRule.setContent {
            HummingbirdTheme {
                Box(modifier = Modifier.width(unconstrained)) { BareAddTimeButton() }
            }
        }
        val bare = composeTestRule.onNodeWithText(LONG_LABEL).getUnclippedBoundsInRoot()
        assertTrue(
            "the bare button must fall short of 44dp, or the target above measures " +
                "nothing (measured ${bare.height})",
            bare.height < 44.dp,
        )
        assertTrue(
            "even the long phrasing fits $halfSlot — the width was never the risk " +
                "(measured to ${bare.right})",
            bare.right <= halfSlot,
        )
    }

    /** The pair as it actually ships since the pickers landed: the deadline
     * is [DeadlineField], the scheduled date is [CaptureDateField], and
     * they share one line. `Alignment.Top` stops being tidiness here — a
     * deadline that names a minute grows a second control, and the field
     * beside it must not move when it does. That is a claim no test could
     * make before, because the two columns were always the same height. */
    @Test
    fun `naming a minute grows the deadline column without shifting its neighbour`() {
        composeTestRule.setContent {
            HummingbirdTheme {
                // `spacedBy(8.dp)` and `Alignment.Top`, both as the two
                // capture surfaces render it — the gap is what makes each
                // half $halfSlot rather than a naive 197.5dp, and the first
                // run of this test failed on exactly that 4dp.
                Row(
                    modifier = Modifier.width(contentWidth),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    verticalAlignment = Alignment.Top,
                ) {
                    DeadlineField(
                        value = "2026-08-24T09:30",
                        error = null,
                        onValueChange = {},
                        modifier = Modifier.weight(1f),
                    )
                    CaptureDateField(
                        label = "Scheduled date",
                        value = "2026-08-24",
                        error = null,
                        onValueChange = {},
                        modifier = Modifier.weight(1f),
                    )
                }
            }
        }
        val deadline = composeTestRule.onNodeWithText("Deadline").getUnclippedBoundsInRoot()
        val scheduled = composeTestRule.onNodeWithText("Scheduled date").getUnclippedBoundsInRoot()
        val time = composeTestRule.onNodeWithText("Time").getUnclippedBoundsInRoot()

        assertEquals(
            "both dates must still start on one line",
            deadline.top,
            scheduled.top,
        )
        assertTrue(
            "the revealed time control must sit under the deadline, not beside it " +
                "(deadline top ${deadline.top}, time top ${time.top})",
            time.top > deadline.top,
        )
        assertTrue(
            "the time control must stay inside the deadline's own half " +
                "(measured to ${time.right}, slot $halfSlot)",
            time.right <= halfSlot + 1.dp,
        )
        for ((label, box) in listOf("Deadline" to deadline, "Scheduled date" to scheduled)) {
            assertTrue(
                "$label must get about half of $contentWidth, not ${box.width}",
                box.width > contentWidth / 3 && box.width < contentWidth * 2 / 3,
            )
        }
    }

    private companion object {
        const val LONG_LABEL = "Add a time of day"
    }

    @Composable
    private fun BareAddTimeButton() {
        TextButton(onClick = {}) { Text(LONG_LABEL) }
    }
}
