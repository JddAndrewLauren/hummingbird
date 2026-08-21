package net.twinion.hummingbird

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.height
import androidx.compose.ui.unit.width
import net.twinion.hummingbird.ui.ChoiceRow
import uniffi.hummingbird_ffi_mobile.MobileMicrotaskAffordance
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
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
 * | two icons + `Promote` + check | 48 + 48 + 105 + 48 = 249dp | fits, and only the submit can grow |
 *
 * Which is also why the widest *possible* labels are the ones the control
 * test asserts against rather than the common ones: `Grill me` /
 * `Resume grill` is the core's own label
 * (`hummingbird_core::decisions::grill_button_label`, shared verbatim with
 * the web and not this surface's to shorten) and it changes under the row
 * as a draft appears, as does `Rewrite N steps` as steps are ticked off.
 *
 * **The fit is not the whole story, which is why this file also measures a
 * raised font scale.** A row that fits at default scale can still lose a
 * control at a bigger one: a plain `Row` measures its non-weighted children
 * in composition order and the check is composed last, so the check was the
 * row's shock absorber and went to zero width at 2.5x. The panel caps the
 * submit for that reason, and the pair of tests below — the survival and
 * its uncapped control — is where the cap earns its keep.
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

    /** Renders `content` at a chosen font scale, leaving the density alone.
     *
     * `LocalDensity` is what Compose measures text against, and replacing
     * only its `fontScale` is the one lever that moves the labels without
     * moving the 48dp touch targets — which is exactly the asymmetry this
     * row lives or dies on. The Robolectric route (a `fontScale` on the
     * test `Configuration`) is not used: 4.14.1 has no font-scale resource
     * qualifier, so it would have to be poked onto the configuration
     * imperatively, per-test, before the compose rule's host view exists —
     * and a scale that failed to take would leave a green test measuring
     * default scale, which is the failure mode this file is most exposed to
     * (see the control test below). Provided in-composition instead, where
     * it cannot fail to apply. */
    @Composable
    private fun AtFontScale(scale: Float, content: @Composable () -> Unit) {
        val base = LocalDensity.current
        CompositionLocalProvider(
            LocalDensity provides Density(base.density, scale),
            content = content,
        )
    }

    /** The pane's own action row, in the shape `DetailBody` renders it: the
     * two agent affordances as icons, the mode's submit, and the mark-done
     * check, at the width the narrowest host gives it.
     *
     * [submitCapped] mirrors the panel's own `submitMaxWidth` plus its
     * single-line label; `false` is the row as it shipped, and only the
     * control test asks for that.
     */
    private fun actionRow(
        grillLabel: String,
        microtaskLabel: String,
        submitLabel: String,
        fontScale: Float = 1f,
        submitCapped: Boolean = true,
    ) {
        composeTestRule.setContent {
            AtFontScale(fontScale) {
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
                        Button(
                            onClick = {},
                            modifier = if (submitCapped) {
                                Modifier.widthIn(max = SUBMIT_MAX)
                            } else {
                                Modifier
                            },
                        ) {
                            if (submitCapped) {
                                Text(submitLabel, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            } else {
                                Text(submitLabel)
                            }
                        }
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

    /** The defect this row shipped with, and the cap that ends it.
     *
     * A plain `Row` measures its non-weighted children in composition order
     * against the width left over, and the check is composed **last** — so
     * the check is what the row spends when the submit's label grows, and
     * it spends it silently. Measured uncapped, the check goes 40dp → 37 →
     * 33 → 30 → 24 → 12 → **0** across 1.6x…2.5x ([RAISED_FONT_SCALE]
     * carries the table): a WRITE control gone, with nothing on screen
     * saying it was there.
     *
     * The submit is therefore capped at [SUBMIT_MAX] and ellipsises on one
     * line, so the three touch targets are what the row cannot spend and
     * the submit is what gives way. This asserts the consequence at the
     * scale that used to take the check: all four still on one line, and
     * the check a whole touch target rather than a surviving sliver. */
    @Test
    fun `the mark-done check survives a raised font scale`() {
        actionRow(
            grillLabel = "Resume grill",
            microtaskLabel = "Rewrite 3 steps",
            submitLabel = "Promote",
            fontScale = RAISED_FONT_SCALE,
        )
        val check = byDescription(MARK_DONE)
        assertSharesALine(
            listOf(
                byDescription("Resume grill"),
                byDescription("Rewrite 3 steps"),
                byText("Promote"),
                check,
            ),
            // One line of 2.5x text measures 66dp; the same row uncapped,
            // whose label wraps, measures 114dp. 80dp is between them, so
            // this still refuses a wrap without calling a big single line
            // one.
            maxHeight = 80.dp,
        )
        assertTrue(
            "the check must still be a whole touch target at ${RAISED_FONT_SCALE}x, " +
                "not a sliver of one (${check.width})",
            check.width >= TOUCH_TARGET,
        )
        assertTrue(
            "and still be reachable at the row's right edge (${check.right} of $PANE_WIDTH)",
            check.right > PANE_WIDTH - EDGE_INSET,
        )
    }

    /** The control that gives the test above teeth: the row **without** the
     * cap, at the same scale, must actually lose the check.
     *
     * Without it, a Robolectric text-measurement change — or a font scale
     * that silently failed to apply — would leave the test above green
     * while the cap did nothing, and the cap is the only thing standing
     * between a raised-scale phone and a missing write control.
     *
     * The check is asserted at **zero** width rather than merely at less
     * than a touch target, because that is the whole shape of the defect:
     * a control the operator cannot see is missing. Note what does *not*
     * catch it — the uncapped row's four centres still agree, so
     * [assertSharesALine] passes on it. Only the width knows. */
    @Test
    fun `without the cap that scale is what took the check`() {
        actionRow(
            grillLabel = "Resume grill",
            microtaskLabel = "Rewrite 3 steps",
            submitLabel = "Promote",
            fontScale = RAISED_FONT_SCALE,
            submitCapped = false,
        )
        val check = byDescription(MARK_DONE)
        assertEquals(
            "an uncapped submit at ${RAISED_FONT_SCALE}x must squeeze the check to nothing — " +
                "otherwise the cap is not what keeps it (width ${check.width})",
            0.0,
            check.width.value.toDouble(),
            0.5,
        )
    }

    /** Default scale is unchanged, by measurement rather than by eye — the
     * cap is a ceiling nothing reaches there.
     *
     * `Promote` is the wider of the two submits the pane renders (`Save` is
     * narrower), and it measures 105dp against the 128dp cap, so the label
     * is un-ellipsised and the button is the width it always was. Asserted
     * as a strict inequality: a submit sitting *at* the cap is one whose
     * label has been clipped, which would be the cap changing the pane's
     * look rather than guarding it. */
    @Test
    fun `at default scale the cap changes nothing`() {
        actionRow("Resume grill", "Rewrite 3 steps", "Promote")
        val submit = byText("Promote")
        assertTrue(
            "the submit must measure clear of the cap at default scale " +
                "(${submit.width} inside $SUBMIT_MAX)",
            submit.width < SUBMIT_MAX,
        )
        val check = byDescription(MARK_DONE)
        assertTrue(
            "and the check must be a whole touch target (${check.width})",
            check.width >= TOUCH_TARGET,
        )
    }

    /** [microtaskLabel] is the affordance's whole voice — the control prints
     * no words, so this string *is* the button to a screen reader, and
     * "Rewrite 1 steps" is a defect nothing else in this module can see
     * (Robolectric cannot read a rendered icon's accessible name back out of
     * a pixel, and the render tests above only pass labels in).
     *
     * The count is the affordance's own applied number, so the singular is
     * the case that has to be right: one undone step is the state a
     * half-worked item sits in longest. */
    @Test
    fun `the microtask affordance pluralises its own step count`() {
        assertEquals(
            "Break into steps",
            microtaskLabel(MobileMicrotaskAffordance.Break),
        )
        assertEquals(
            "one step is singular",
            "Rewrite 1 step",
            microtaskLabel(MobileMicrotaskAffordance.Rewrite(1u)),
        )
        assertEquals(
            "Rewrite 3 steps",
            microtaskLabel(MobileMicrotaskAffordance.Rewrite(3u)),
        )
        // Not "Rewrite 0 step": the affordance is only `Rewrite` when there
        // is something to rewrite, but the rule is the count, not the
        // affordance, so it answers for the boundary either way.
        assertEquals(
            "Rewrite 0 steps",
            microtaskLabel(MobileMicrotaskAffordance.Rewrite(0u)),
        )
    }

    private fun byText(text: String) =
        composeTestRule.onNodeWithText(text).getUnclippedBoundsInRoot()

    private fun byDescription(description: String) =
        composeTestRule.onNodeWithContentDescription(description).getUnclippedBoundsInRoot()

    /** Controls share a line when their vertical centres agree. Compared
     * rather than asserted against a constant, because the number that
     * matters is relative: a row that wrapped would put the overflowing
     * control a full button-height lower, and nothing else moves any of
     * them.
     *
     * Centres rather than tops, which is what this was: at default scale
     * every control is 40dp tall and the two agree, but a raised font scale
     * makes the submit taller than the icon buttons beside it, and
     * `Alignment.CenterVertically` then gives them different tops on the
     * same line. Tops would have failed that row for being centred, which
     * is the one thing it is doing right. */
    private fun assertSharesALine(
        bounds: List<androidx.compose.ui.unit.DpRect>,
        // A letter column is many lines tall; one line of text at a raised
        // font scale is legitimately taller than [ONE_LINE], so the bound
        // is the caller's to state at the scale it renders at.
        maxHeight: Dp = ONE_LINE,
    ) {
        fun centre(b: androidx.compose.ui.unit.DpRect) = (b.top + b.height / 2).value.toDouble()
        val first = centre(bounds.first())
        for ((index, b) in bounds.withIndex()) {
            assertEquals(
                "control $index must share the action row's line " +
                    "(centres ${bounds.map { centre(it) }})",
                first,
                centre(b),
                0.5,
            )
            assertTrue(
                "and none may stand up as a letter column (heights ${bounds.map { it.height }})",
                b.height <= maxHeight,
            )
        }
        // Per control, not `maxOf`, which is what this used to be: one
        // control measured to zero width while its neighbours are fine is
        // the whole defect (see `the mark-done check survives a raised font
        // scale`), and a max over the row cannot see it.
        for ((index, b) in bounds.withIndex()) {
            assertNotEquals(
                "control $index must not be squeezed to nothing " +
                    "(widths ${bounds.map { it.width }})",
                0.0,
                b.width.value.toDouble(),
                0.01,
            )
        }
        val widest: Dp = bounds.maxOf { it.width }
        assertTrue("and the row must have measured at all ($widest)", widest > 0.dp)
    }

    private companion object {
        /** 320dp of display less the route host's 24dp of padding on each
         * side — see this file's header. */
        val PANE_WIDTH = 272.dp

        /** The panel's own `submitMaxWidth`, derived the same way it is
         * there: [PANE_WIDTH] less the three 48dp touch targets that must
         * never be squeezed, 272 − 3 × 48 = 128dp. Restated here rather
         * than read off the source, so a change to one and not the other
         * fails a measurement rather than passing a grep. */
        val SUBMIT_MAX = 128.dp

        /** The scale the squeeze was reproduced at, measured here rather
         * than assumed: uncapped, the check holds its full width to 1.6x,
         * then loses it steadily — 37dp at 1.7x, 33 at 1.8, 30 at 1.9, 24
         * at 2.0, 12 at 2.2 — and is **zero** at 2.5x, which is where the
         * control test below plants its flag. 2.5x is the honest scale to
         * pin at for two reasons: it is unambiguous (a partly-clipped
         * target is a judgement call, a missing one is not), and it is the
         * scale at which the uncapped row also wraps its label, so a
         * single number covers both halves of the failure.
         *
         * Two things this deliberately does not claim. It is past Android's
         * own font-size slider (1.3x, where nothing is lost); reaching it
         * takes Android 14's non-linear scaling on top of a display-size
         * bump. And the loss is gradual, so no single scale is "the
         * threshold" — the defect is that the check is the row's shock
         * absorber at all. */
        const val RAISED_FONT_SCALE = 2.5f

        /** A Material3 `IconButton`'s merged node measures 40dp here (its
         * touch box around an 18dp glyph — the 48dp in the pane's own
         * arithmetic is Material's nominal minimum, which is why the cap is
         * derived from the nominal and lands conservative). Bounded a
         * little under the measured value so a Material padding change does
         * not fail it, and far enough above zero that a squeezed-out
         * control cannot pass. */
        val TOUCH_TARGET = 36.dp

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
