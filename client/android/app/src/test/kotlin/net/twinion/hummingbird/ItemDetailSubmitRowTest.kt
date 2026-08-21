package net.twinion.hummingbird

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.unit.Dp
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

/** The item pane's action row is one line — the grill, the microtask
 * affordance, the submit and the mark-done check together (operator
 * decision 2026-08-20) — and this measures that it stays one.
 *
 * It is the mirror image of `ChoiceRowWrappingTest`, and the pair is the
 * point: that file proves the shared row *wraps* when a choice cannot fit,
 * which is right for the act row above this one — "Start / Mark blocked /
 * Cancel" fits no phone. This file proves the row **beneath** it never has
 * to, at the narrowest width any host hands the pane. The difference is
 * bought with labels rather than with layout, and the arithmetic is why
 * only the submit keeps a printed one.
 *
 * **The width is derived, not picked.** `ChoiceRowWrappingTest` measures
 * the bare component at the 320dp qualifier; a row inside this pane never
 * gets the whole display. The narrowest of the four hosts is the
 * notification route (`ItemDetailScreen`), which pays `.padding(24.dp)`
 * around the panel, so on a 320dp phone the row is laid out in **272dp**.
 * That is what [PANE_WIDTH] is, and every number below was measured here
 * rather than estimated:
 *
 * | row | needs | 272dp |
 * | --- | --- | --- |
 * | `Resume grill` + `Rewrite 3 steps` + `Promote` + check | 131 + 149 + 114 + 48 + gaps = 466dp | wraps twice |
 * | `Grill` + `Steps` + `Save` + check, shortened | 77 + 86 + 90 + 48 + gaps = 325dp | still wraps — and 269dp without the check, which is how close it is |
 * | two icons + `Promote` + check | 48 + 48 + 114 + 48 = 258dp | fits |
 *
 * Which is also why the widest *possible* labels are the ones the control
 * test asserts against rather than the common ones: `Grill me` /
 * `Resume grill` is the core's own label
 * (`hummingbird_core::decisions::grill_button_label`, shared verbatim with
 * the web and not this surface's to shorten) and it changes under the row
 * as a draft appears, as does `Rewrite N steps` as steps are ticked off.
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

    /** The pane's own action row, in the shape `DetailBody` renders it: the
     * two agent affordances as icons, the mode's submit, and the mark-done
     * check, at the width the narrowest host gives it. */
    private fun actionRow(grillLabel: String, microtaskLabel: String, submitLabel: String) {
        composeTestRule.setContent {
            Column(Modifier.width(PANE_WIDTH)) {
                Row(
                    modifier = Modifier.width(PANE_WIDTH),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    IconButton(onClick = {}) {
                        Icon(
                            painterResource(R.drawable.ic_messages_square),
                            contentDescription = grillLabel,
                            modifier = Modifier.size(18.dp),
                        )
                    }
                    IconButton(onClick = {}) {
                        Icon(
                            painterResource(R.drawable.ic_list_checks),
                            contentDescription = microtaskLabel,
                            modifier = Modifier.size(18.dp),
                        )
                    }
                    Spacer(Modifier.weight(1f))
                    Button(onClick = {}) { Text(submitLabel) }
                    IconButton(onClick = {}) {
                        Icon(
                            painterResource(R.drawable.ic_check),
                            contentDescription = MARK_DONE,
                            modifier = Modifier.size(18.dp),
                        )
                    }
                }
            }
        }
    }

    @Test
    fun `the widest action row the pane can render stays on one line`() {
        actionRow(
            grillLabel = "Resume grill",
            microtaskLabel = "Rewrite 3 steps",
            submitLabel = "Promote",
        )
        val tops = listOf(
            byDescription("Resume grill"),
            byDescription("Rewrite 3 steps"),
            byText("Promote"),
            byDescription(MARK_DONE),
        )
        assertSharesALine(tops)
    }

    @Test
    fun `the saving hosts' row stays on one line`() {
        actionRow(
            grillLabel = "Grill me",
            microtaskLabel = "Break into steps",
            submitLabel = "Save",
        )
        assertSharesALine(
            listOf(
                byDescription("Grill me"),
                byDescription("Break into steps"),
                byText("Save"),
                byDescription(MARK_DONE),
            ),
        )
    }

    /** The submit and the check stay anchored to the row's right edge —
     * the `weight(1f)` between the two groups, not the count of buttons, is
     * what decides where they sit. A pane whose submit slid left when the
     * grill was ineligible would move its most important control out from
     * under the operator's thumb, so the edge is asserted absolutely rather
     * than against a sibling render. */
    @Test
    fun `the submit and the check are anchored to the right edge`() {
        actionRow("Grill me", "Break into steps", "Save")
        val check = byDescription(MARK_DONE)
        val submit = byText("Save")
        assertTrue(
            "the check must end at the row's right edge, less the touch target's own " +
                "inset around its 18dp glyph (${check.right} of $PANE_WIDTH)",
            check.right > PANE_WIDTH - EDGE_INSET,
        )
        assertTrue(
            "and the submit must sit immediately left of it, not adrift in the middle " +
                "(submit ends ${submit.right}, check starts ${check.left})",
            submit.right <= check.left && submit.right > check.left - 8.dp,
        )
    }

    /** The same, with neither leading affordance rendered — an archived
     * item, or one the grill is ineligible for. Same absolute edge. */
    @Test
    fun `the submit stays at that edge when the leading affordances are absent`() {
        composeTestRule.setContent {
            Column(Modifier.width(PANE_WIDTH)) {
                Row(
                    modifier = Modifier.width(PANE_WIDTH),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Spacer(Modifier.weight(1f))
                    Button(onClick = {}) { Text("Save") }
                    IconButton(onClick = {}) {
                        Icon(
                            painterResource(R.drawable.ic_check),
                            contentDescription = MARK_DONE,
                            modifier = Modifier.size(18.dp),
                        )
                    }
                }
            }
        }
        assertTrue(
            "the check must still end at the row's right edge",
            byDescription(MARK_DONE).right > PANE_WIDTH - EDGE_INSET,
        )
    }

    /** The control that gives the tests above teeth: the row the pane
     * *used* to draw — every control labelled — must actually fail at this
     * width, and so must the shortened words.
     *
     * Without it, a widened [PANE_WIDTH] — or a regression in Robolectric's
     * text measurement — would leave this file green while measuring
     * nothing, which is the failure mode a width-dependent test is most
     * exposed to. It is also the record of why the labels came off at all,
     * in numbers: 466dp, and 325dp even cut to one word each, asked of a
     * 272dp row. Both variants render at once, and their labels differ, so
     * one `setContent` measures both. */
    @Test
    fun `labelling every control is what wrapping looked like`() {
        val variants = listOf(
            Triple("Resume grill", "Rewrite 3 steps", "Promote"),
            Triple("Grill", "Steps", "Save"),
        )
        composeTestRule.setContent {
            Column(Modifier.width(PANE_WIDTH)) {
                for (labels in variants) {
                    ChoiceRow {
                        OutlinedButton(onClick = {}) { Text(labels.first) }
                        OutlinedButton(onClick = {}) { Text(labels.second) }
                        Button(onClick = {}) { Text(labels.third) }
                        IconButton(onClick = {}) {
                            Icon(
                                painterResource(R.drawable.ic_check),
                                // Distinct per variant, so both rows can be
                                // measured from one `setContent`.
                                contentDescription = "Mark \"${labels.first}\" done",
                                modifier = Modifier.size(18.dp),
                            )
                        }
                    }
                }
            }
        }
        for (labels in variants) {
            val first = byText(labels.first)
            val check = byDescription("Mark \"${labels.first}\" done")
            assertTrue(
                "labelled \"${labels.first} / ${labels.second} / ${labels.third}\" plus the " +
                    "check must not fit a $PANE_WIDTH row — otherwise this file's other " +
                    "tests would pass with or without the labels coming off " +
                    "(tops ${first.top} and ${check.top})",
                check.top > first.top,
            )
        }
    }

    private fun byText(text: String) =
        composeTestRule.onNodeWithText(text).getUnclippedBoundsInRoot()

    private fun byDescription(description: String) =
        composeTestRule.onNodeWithContentDescription(description).getUnclippedBoundsInRoot()

    /** Controls share a line when their tops agree. Compared rather than
     * asserted against a constant, because the number that matters is
     * relative: a row that wrapped would put the overflowing control a full
     * button-height lower, and nothing else moves any of them. */
    private fun assertSharesALine(bounds: List<androidx.compose.ui.unit.DpRect>) {
        val top = bounds.first().top
        for ((index, b) in bounds.withIndex()) {
            assertEquals(
                "control $index must share the action row's line " +
                    "(tops ${bounds.map { it.top }})",
                top.value.toDouble(),
                b.top.value.toDouble(),
                0.5,
            )
            assertTrue(
                "and none may stand up as a letter column (heights ${bounds.map { it.height }})",
                b.height <= ONE_LINE,
            )
        }
        val widest: Dp = bounds.maxOf { it.width }
        assertTrue("nor be squeezed to nothing (widths ${bounds.map { it.width }})", widest > 0.dp)
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

        /** The glyph inside a 48dp `IconButton` is inset from the button's
         * own edge, so an anchored control's measured right edge lands a
         * few dp short of the row's. Measured at 4dp; the bound is loose
         * enough to absorb a Material padding change and tight enough that
         * a control adrift in the middle of a 272dp row still fails. */
        val EDGE_INSET = 8.dp

        /** The check's accessible name is the item's, so the pane's own
         * string is not what this file measures — only that the glyph is
         * on the line. */
        const val MARK_DONE = "Mark \"Ring the vet\" done"
    }
}
