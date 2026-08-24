package net.twinion.hummingbird.ui.forms

import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.PressInteraction
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.DatePicker
import androidx.compose.material3.DatePickerDefaults
import androidx.compose.material3.DatePickerDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberDatePickerState
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
import net.twinion.hummingbird.R
import net.twinion.hummingbird.core.WallClock

/** #529's third shared form component: one of the capture box's two
 * dates — a whole civil day, picked rather than typed. The scheduled date
 * uses it directly; the deadline reaches it through [DeadlineField], which
 * adds the optional minute the deadline grammar also accepts.
 *
 * **No date regex lives here, or anywhere in this file.** What is wrong
 * with the value — a malformed shape, an impossible calendar date — is the
 * core's own answer
 * ([`uniffi.hummingbird_ffi_mobile.captureMetaProblems`], reached through
 * `hummingbird_core::decisions::capture::capture_meta_problems`), read by
 * the caller and passed in as [error]. This component only shows whatever
 * it is handed; it decides nothing about what a valid date looks like
 * (#529's own structural-test criterion: "no date regexes"). That the field
 * is now a picker does not retire [error]: a value can still arrive
 * malformed from an item a skill or an older capture wrote.
 *
 * **The field is read-only and never raises the keyboard** (operator
 * decision 2026-08-24). Tapping it anywhere opens the picker, which is why
 * the press is collected off an [interactionSource][MutableInteractionSource]
 * rather than through `Modifier.clickable`: a `readOnly` `OutlinedTextField`
 * consumes the click itself, so a `clickable` on it never fires — a trap
 * worth knowing before the next read-only field. `ProjectField`'s
 * `menuAnchor` is the other shape of the same problem, and does not
 * transfer: it anchors a dropdown, not a dialog.
 *
 * **The trailing icon is the whole clearing story**, and is load-bearing
 * rather than decoration: with no keyboard there is otherwise no gesture
 * that empties a date somebody set by mistake. One slot, two states — a
 * calendar when the field is empty, an × when it is not — because the two
 * dates share a line and a ~193dp slot on the Fold's cover display has room
 * for one 24dp target beside a ten-character value, not two.
 *
 * A value the picker cannot represent still shows, and can still be
 * replaced or cleared: `WallClock.civilDateMillis` answers `null` for it and
 * the picker simply opens on today. That covers a *valid* date too, when its
 * year falls outside Material's `DatePickerDefaults.YearRange` — the core
 * bounds no year, and handing an out-of-range one to `rememberDatePickerState`
 * crashes rather than clamps. That is the visible half of
 * `split_deadline`'s deliberate pass-through of legacy free text — it stays
 * readable rather than being emptied on load, which is the whole reason the
 * core keeps it. **The absence of `singleLine = true` is load-bearing**, not
 * an omission: a legacy value longer than the ~193dp half-slot wraps instead
 * of truncating, and with no keyboard there is nothing wrapping costs. Adding
 * it for tidiness would hide exactly the text the core went to trouble to
 * keep.
 *
 * [modifier] exists so a caller can seat two of these side by side — the
 * capture surfaces pair deadline and scheduled date on one line (operator
 * decision 2026-08-20) with a `weight(1f)` each. `fillMaxWidth()` is
 * applied *after* it rather than being the parameter's default, so a
 * caller that passes a modifier still gets the full width of whatever it
 * was given: inside a weighted `Row` slot that is the slot's width, and
 * unmodified it is the Triage editor's stacked full-width field. A
 * `Modifier.fillMaxWidth()` default would have silently dropped for every
 * caller that passed anything.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CaptureDateField(
    label: String,
    value: String,
    error: String?,
    onValueChange: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    var picking by rememberSaveable { mutableStateOf(false) }
    val interactionSource = remember { MutableInteractionSource() }

    LaunchedEffect(interactionSource) {
        interactionSource.interactions.collect { interaction ->
            if (interaction is PressInteraction.Release) picking = true
        }
    }

    OutlinedTextField(
        value = value,
        onValueChange = {},
        readOnly = true,
        interactionSource = interactionSource,
        label = { Text(label) },
        isError = error != null,
        supportingText = error?.let { { Text(it) } },
        trailingIcon = {
            if (value.isEmpty()) {
                IconButton(onClick = { picking = true }) {
                    Icon(
                        painterResource(R.drawable.ic_calendar),
                        contentDescription = "Pick $label",
                    )
                }
            } else {
                IconButton(onClick = { onValueChange("") }) {
                    Icon(
                        painterResource(R.drawable.ic_x),
                        contentDescription = "Clear $label",
                    )
                }
            }
        },
        // The field announces itself as an edit box, which it no longer is,
        // so the gesture is named here — and, crucially, **performed** here.
        // A screen reader activates a node with `ACTION_CLICK`, which Compose
        // routes to this lambda; it never synthesises the pointer input the
        // press collector above listens for. An earlier version returned
        // `false` (label only, "the collector handles it"), which meant
        // TalkBack announced "Pick Deadline", took the double-tap, and did
        // nothing — and with `readOnly` there is no keyboard to fall back
        // on, so a date could be cleared but never set. Returning `true`
        // reports the action handled.
        modifier = modifier
            .fillMaxWidth()
            .semantics {
                onClick(label = "Pick $label") {
                    picking = true
                    true
                }
            },
    )

    if (picking) {
        // Seeded fresh on every open, so the picker always starts from the
        // value the field is showing rather than from wherever it was left.
        val state = rememberDatePickerState(
            // `DatePickerDefaults.YearRange` is the same bound this state
            // `require`s, handed to the conversion so an out-of-range year
            // resolves to "open on today" instead of throwing. See
            // `WallClock.civilDateMillis`.
            initialSelectedDateMillis =
                WallClock.civilDateMillis(value, DatePickerDefaults.YearRange),
        )
        DatePickerDialog(
            onDismissRequest = { picking = false },
            confirmButton = {
                TextButton(
                    // Disabled rather than a no-op: a picker opened on a
                    // value it cannot represent starts with nothing
                    // selected, and confirming that must not read as
                    // "accepted" when it changed nothing.
                    enabled = state.selectedDateMillis != null,
                    onClick = {
                        state.selectedDateMillis?.let { onValueChange(WallClock.civilDate(it)) }
                        picking = false
                    },
                ) { Text("Set") }
            },
            dismissButton = {
                TextButton(onClick = { picking = false }) { Text("Cancel") }
            },
        ) {
            DatePicker(state = state)
        }
    }
}
