package net.twinion.hummingbird.ui.forms

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

/** Priority is a small, fixed 0..4 range, not an open or a domain
 * vocabulary — `hummingbird_core::decisions::vocabulary` owns size, energy
 * and the suggested contexts, but names no priority labels (ADR-0025's own
 * verdict table: the web's `priority.ts` keeps its labels literal TS for
 * the same reason). So this row's five labels stay literal here too,
 * mirroring that precedent rather than inventing a fourth core door for a
 * five-word, never-renamed display list.
 *
 * **The order (`1,2,3,4` — Urgent..Low) is
 * `decisions::frontier::priority_rank`'s own order, pinned from the Rust
 * side** (review finding on #529's own PR, note 5): a plain JVM test
 * cannot call a generated JNI binding to check it here
 * (`CaptureSubmitRefusalTest`'s own doc), so
 * `ffi-mobile/src/lib.rs`'s `the_priority_row_order_matches_priority_rank`
 * asserts this exact sequence against the real rule instead — if that rule
 * ever reorders, that Rust test breaks and names this list as what needs
 * updating to match. That test still sorts the wire value `0` too, and its
 * landing last is what makes this row's omission of it safe. ADR-0025's ledger for `priority.ts` records the same
 * asymmetry on the web side: the rank is pinned (`seam.test.ts`'s
 * `priorityRankFromCore`), the labels are not.
 *
 * Shared rather than private to the capture box since #565's review: the
 * Triage editor seeds a priority it had no control for, so the field was
 * carried into every promote as dead state. One list of five labels, in
 * one order that one Rust test pins, is exactly the thing a second copy
 * must not be made of.
 *
 * Note the sentinel this does not share with [LevelSlider]: cleared here is
 * `""`, not `null`, because a priority draft is a non-null `String`. **That
 * sentinel is the only control over it now** (operator decision
 * 2026-08-20): there is no "No priority" chip, because not picking one
 * already says it, and a chip for the absence of a choice is a fifth target
 * that means what the resting state means. Re-tapping the selected chip is
 * how a priority is taken back off.
 */
@Composable
fun PriorityRow(
    selected: String,
    onSelect: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val options = listOf(
        "1" to "Urgent",
        "2" to "High",
        "3" to "Medium",
        "4" to "Low",
    )
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(
            "Priority",
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        // One line, and a fixed `Row` — [LevelSlider]'s own container, one
        // field above this one, which is half the reason: two adjacent rows
        // of chips that wrap differently read as two different controls.
        //
        // This wrapped in a `FlowRow` until 2026-08-20, and dropping the
        // fifth chip is what let it stop. Four labelled `FilterChip`s want
        // 303dp (`PriorityRowWrappingTest` measures it under the app's own
        // theme, not Material's defaults — the round-4 trap), against
        // 395dp of content on the Fold's cover display, the narrowest
        // surface this app ships to. That is the operator's stated width
        // budget for a one-line strip (`AxisRow`'s header states it and
        // why), and the five-chip row could not have met it.
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            for ((value, label) in options) {
                FilterChip(
                    selected = value == selected,
                    onClick = { onSelect(if (value == selected) "" else value) },
                    label = { Text(label) },
                )
            }
        }
    }
}
