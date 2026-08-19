package net.twinion.hummingbird.ui.forms

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
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
 * **The order (`1,2,3,4,0` — Urgent..Low, then No priority last) is
 * `decisions::frontier::priority_rank`'s own order, pinned from the Rust
 * side** (review finding on #529's own PR, note 5): a plain JVM test
 * cannot call a generated JNI binding to check it here
 * (`CaptureSubmitRefusalTest`'s own doc), so
 * `ffi-mobile/src/lib.rs`'s `the_priority_row_order_matches_priority_rank`
 * asserts this exact sequence against the real rule instead — if that rule
 * ever reorders, that Rust test breaks and names this list as what needs
 * updating to match. ADR-0025's ledger for `priority.ts` records the same
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
 * `""`, not `null`, because a priority draft is a non-null `String`.
 */
@OptIn(ExperimentalLayoutApi::class)
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
        "0" to "No priority",
    )
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(
            "Priority",
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        // A `FlowRow`, not a `Row`: five labelled chips ("Urgent / High /
        // Medium / Low / No priority") are wider than a phone at the
        // default font scale, and a fixed, non-scrolling Row put the
        // trailing priorities out of reach — the same clipping `NowScreen`'s
        // action buttons already answered with a wrapping container.
        FlowRow(
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
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
