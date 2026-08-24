package net.twinion.hummingbird.ui.forms

import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.PressInteraction
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TimePicker
import androidx.compose.material3.rememberTimePickerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.DialogProperties
import net.twinion.hummingbird.R
import net.twinion.hummingbird.core.WallClock
import uniffi.hummingbird_ffi_mobile.joinDeadline
import uniffi.hummingbird_ffi_mobile.splitDeadline

/** The one control that edits an item's deadline: a date picker, and a time
 * picker that appears only when the deadline actually names a minute. The
 * Android half of `client/web/src/components/forms/DeadlineField.tsx`,
 * ported rather than reinvented — both clients edit one wire field and a
 * reader moving between them should not meet two different ideas of what a
 * deadline is.
 *
 * **Why the time is behind a button.** Most deadlines are days — "the
 * passport renewal is due on the 9th" — and a permanently visible time
 * picker asks a question the reader usually has no answer to, then shows
 * `00:00` when they decline to answer it. So the resting form is a date,
 * and naming an hour is a deliberate second gesture. Clearing the time is
 * the same gesture reversed (the × on the time field), which puts the
 * deadline back to a whole day rather than to midnight.
 *
 * **The value is one string throughout**, exactly as the wire carries it
 * (`YYYY-MM-DD` or `YYYY-MM-DDTHH:MM`). [splitDeadline]/[joinDeadline] —
 * `hummingbird_core::decisions::urgency`, crossed on this seam for Android
 * the same way `deadline-parts.ts` reaches it on the web — do the whole of
 * the splitting and joining, so this component holds no parsed state that
 * could disagree with the value it was given. Splitting the string here
 * instead would be a second copy of a grammar that already has an owner,
 * which is what ADR-0025 forbids and what
 * `CaptureFieldSetStructuralTest`'s date-regex ban catches.
 *
 * **One deliberate divergence from the web, and the reason for it.** The
 * web reveals an *empty* `<input type="time">` and treats that as a reader
 * mid-decision. A Compose `TimePicker` cannot be empty, and seeding one at
 * `00:00` would be a silent edit rather than a blank: a date-only deadline
 * means *end* of that day (`server/domain/src/deadline.rs`'s
 * `deadline_sort_key` reads `2026-08-15` as `2026-08-15T23:59`), so
 * accepting a default `00:00` would move the deadline nearly a full day
 * earlier the instant somebody tapped "Add time". So Android's second
 * gesture opens the dialog directly, seeded from the current wall clock,
 * and **writes nothing until it is confirmed** — which is the same rule the
 * web's empty input expresses, in the form this toolkit allows. Once a
 * minute *is* named the divergence ends: the time field reopens the picker
 * on a tap, seeded from the value it is showing, so changing 09:30 to 10:00
 * is one gesture rather than a remove and a re-add.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun DeadlineField(
    value: String,
    error: String?,
    onValueChange: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val parts = splitDeadline(value)
    var naming by rememberSaveable { mutableStateOf(false) }

    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(8.dp)) {
        CaptureDateField(
            label = "Deadline",
            value = parts.date,
            error = error,
            // `joinDeadline` clears the time with the date: a time with no
            // day is not a deadline, and that rule is the core's, not this
            // form's.
            onValueChange = { onValueChange(joinDeadline(it, parts.time)) },
        )

        val time = parts.time
        if (time == null) {
            // `heightIn(min = 44.dp)`: a `TextButton` rests at Material's
            // 40dp, under the touch target the rest of this app holds itself
            // to (`PaneShell`, `NowScreen`, `ItemDetailPanel`).
            // `DeadlineFieldWrappingTest` measures it, and its control
            // renders the bare button to keep the 4dp a recorded fact.
            // `enabled` on the date, because `joinDeadline` is what decides
            // and it answers "" for a time with no day — a time with no day
            // is not a deadline. Offered unconditionally the button is a
            // silent dead end on every fresh capture: pick 14:00, confirm,
            // and nothing changes anywhere. The web can leave its own always
            // available because it keeps the revealed input on screen, so
            // the reader at least sees what they chose; opening the dialog
            // directly is what turns that into no feedback at all.
            TextButton(
                onClick = { naming = true },
                enabled = parts.date.isNotEmpty(),
                modifier = Modifier.heightIn(min = 44.dp),
            ) {
                Icon(
                    painterResource(R.drawable.ic_calendar_clock),
                    contentDescription = null,
                )
                Spacer(Modifier.width(8.dp))
                Text("Add time")
            }
        } else {
            // Tapping it reopens the picker, the same gesture and the same
            // press-collection idiom the date field uses (and for the same
            // reason — a `readOnly` field eats a `Modifier.clickable`).
            // Without it a set time could only be removed and re-added,
            // which the web's editable `<input type="time">` never asks of
            // anyone.
            val timeInteractions = remember { MutableInteractionSource() }
            LaunchedEffect(timeInteractions) {
                timeInteractions.interactions.collect { interaction ->
                    if (interaction is PressInteraction.Release) naming = true
                }
            }
            OutlinedTextField(
                value = time,
                onValueChange = {},
                readOnly = true,
                interactionSource = timeInteractions,
                label = { Text("Time") },
                trailingIcon = {
                    IconButton(onClick = { onValueChange(joinDeadline(parts.date, null)) }) {
                        Icon(
                            painterResource(R.drawable.ic_x),
                            contentDescription = "Remove the time",
                        )
                    }
                },
                modifier = Modifier
                    .fillMaxWidth()
                    .semantics {
                        // Performed, not merely labelled — see
                        // `CaptureDateField`'s own modifier for why a `false`
                        // here leaves a screen reader with no way in.
                        onClick(label = "Pick a time") {
                            naming = true
                            true
                        }
                    },
            )
        }
    }

    if (naming) {
        // Seeded from the time already set when there is one — reopening
        // the picker to nudge 09:30 to 10:00 must not start from the
        // current clock. `currentHourMinute` is the no-time case only, and
        // is what keeps "Add time" from defaulting to a `T00:00` that would
        // move the deadline nearly a day earlier.
        val (hour, minute) = parts.time?.let { WallClock.hourMinute(it) }
            ?: WallClock.currentHourMinute()
        val state = rememberTimePickerState(initialHour = hour, initialMinute = minute)
        // `AlertDialog`, not `TimePickerDialog`: the latter arrived in
        // Material 3 1.4, and this module's Compose BOM (2025.06.01) is on
        // the 1.3 line. `DiscardConfirmation` in `ItemDetailPanel.kt` is the
        // same chrome, so this is the module's established dialog rather
        // than a shape invented here.
        AlertDialog(
            onDismissRequest = { naming = false },
            // Material swaps `TimePicker` to its **horizontal** dial below
            // 480dp of screen height — which the Fold's own cover display is
            // in landscape (969x443dp, measured on the emulator) — and that
            // dial is wider than `AlertDialog`'s platform default width,
            // with no scroll container of its own. Letting the dialog size
            // to its content is what stops the clock face being clipped;
            // the scroll is for the short-and-narrow case where even the
            // vertical layout does not fit.
            properties = DialogProperties(usePlatformDefaultWidth = false),
            modifier = Modifier.padding(24.dp),
            text = {
                Box(modifier = Modifier.verticalScroll(rememberScrollState())) {
                    TimePicker(state = state)
                }
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        onValueChange(
                            joinDeadline(parts.date, WallClock.civilTime(state.hour, state.minute)),
                        )
                        naming = false
                    },
                ) { Text("Set") }
            },
            dismissButton = {
                TextButton(onClick = { naming = false }) { Text("Cancel") }
            },
        )
    }
}
