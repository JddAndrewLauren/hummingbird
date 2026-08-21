package net.twinion.hummingbird

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Button
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.height
import androidx.compose.ui.unit.width
import net.twinion.hummingbird.ui.ChoiceRow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/** The item pane's submit row must never wrap (operator decision
 * 2026-08-20), and this measures that it doesn't.
 *
 * It is the mirror image of `ChoiceRowWrappingTest`, and the pair is the
 * point: that file proves the shared row *wraps* when a choice cannot fit,
 * which is right for the act row above this one — "Start / Mark blocked /
 * Cancel" fits no phone. This file proves the row **beneath** it never has
 * to, because its two controls fit side by side at the narrowest width any
 * host hands the pane. Same component, two requirements, and the difference
 * is bought with words rather than with layout.
 *
 * **The width is derived, not picked.** `ChoiceRowWrappingTest` measures
 * the bare component at the 320dp qualifier; a row inside this pane never
 * gets the whole display. The narrowest of the four hosts is the
 * notification route (`ItemDetailScreen`), which pays `.padding(24.dp)`
 * around the panel, so on a 320dp phone the row is laid out in **272dp**.
 * That is what [PANE_WIDTH] is, and it is where the old label failed: the
 * numbers below were measured, not estimated.
 *
 * | pair | needs | 272dp |
 * | --- | --- | --- |
 * | `Resume grill` + `Promote to ready` | 131 + 8 + 160 = 299dp | wraps |
 * | `Resume grill` + `Promote` | 131 + 8 + 105 = 244dp | fits |
 *
 * Which is also why the widest *possible* pair is the one asserted rather
 * than the common one. `Grill me` / `Resume grill` is the core's own label
 * (`hummingbird_core::decisions::grill_button_label`, shared verbatim with
 * the web and not this surface's to shorten), so it changes under the
 * submit as a draft appears: `Grill me` + `Promote to ready` fits 272dp
 * perfectly well, and a pane measured only in that state would have called
 * the old label safe.
 *
 * `@GraphicsMode(NATIVE)` is load-bearing for the reason
 * `ChoiceRowWrappingTest`'s header gives at length — in legacy graphics
 * mode Robolectric's text stub returns near-identical widths for every
 * string, and a width-dependent test then measures nothing.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(
    sdk = [35],
    qualifiers = "w320dp-h800dp",
    application = android.app.Application::class,
)
class ItemDetailSubmitRowTest {

    @get:Rule
    val composeTestRule = createComposeRule()

    /** The pane's own submit row, in the shape `DetailBody` renders it: the
     * Grill button, then the mode's submit, in a `ChoiceRow`, at the width
     * the narrowest host gives it. */
    private fun submitRow(grillLabel: String, submitLabel: String) {
        composeTestRule.setContent {
            Column(Modifier.width(PANE_WIDTH)) {
                ChoiceRow {
                    OutlinedButton(onClick = {}) { Text(grillLabel) }
                    Button(onClick = {}) { Text(submitLabel) }
                }
            }
        }
    }

    @Test
    fun `the widest submit row the pane can render stays on one line`() {
        submitRow(grillLabel = "Resume grill", submitLabel = "Promote")
        assertSharesALine("Resume grill", "Promote")
    }

    @Test
    fun `the saving hosts' row stays on one line`() {
        submitRow(grillLabel = "Grill me", submitLabel = "Save")
        assertSharesALine("Grill me", "Save")
    }

    /** The control that gives the two tests above teeth: the label the pane
     * *used* to carry must actually fail at this width.
     *
     * Without it, a widened [PANE_WIDTH] — or a regression in Robolectric's
     * text measurement — would leave this file green while measuring
     * nothing, which is the failure mode a width-dependent test is most
     * exposed to. It is also the record of why the word was shortened at
     * all, in numbers: 299dp asked of a 272dp row. */
    @Test
    fun `the old submit label is what wrapping looked like`() {
        submitRow(grillLabel = "Resume grill", submitLabel = "Promote to ready")
        val grill = composeTestRule.onNodeWithText("Resume grill").getUnclippedBoundsInRoot()
        val submit = composeTestRule.onNodeWithText("Promote to ready").getUnclippedBoundsInRoot()
        assertTrue(
            "\"Promote to ready\" must drop below \"Resume grill\" in a $PANE_WIDTH row — " +
                "otherwise this file's other tests would pass with or without the " +
                "shortened word (widths ${grill.width} and ${submit.width}; tops " +
                "${grill.top} and ${submit.top})",
            submit.top > grill.top,
        )
    }

    /** Two buttons share a line when their tops agree. Compared rather than
     * asserted against a constant, because the number that matters is
     * relative: a `ChoiceRow` that wrapped would put the second button a
     * full button-height lower, and nothing else moves either of them. */
    private fun assertSharesALine(first: String, second: String) {
        val a = composeTestRule.onNodeWithText(first).getUnclippedBoundsInRoot()
        val b = composeTestRule.onNodeWithText(second).getUnclippedBoundsInRoot()
        assertEquals(
            "\"$first\" and \"$second\" must share a line in a $PANE_WIDTH row " +
                "(tops ${a.top} and ${b.top})",
            a.top.value.toDouble(),
            b.top.value.toDouble(),
            0.5,
        )
        assertTrue(
            "and neither may stand up as a letter column (heights ${a.height}, ${b.height})",
            a.height <= ONE_LINE && b.height <= ONE_LINE,
        )
    }

    private companion object {
        /** 320dp of display less the route host's 24dp of padding on each
         * side — see this file's header. */
        val PANE_WIDTH = 272.dp

        /** A button holding one line of its own label; `ChoiceRowWrappingTest`'s
         * own bound, for the same reason (the real render is 40dp, and the
         * margin absorbs font-metric drift without admitting a letter
         * column). */
        val ONE_LINE = 56.dp
    }
}
