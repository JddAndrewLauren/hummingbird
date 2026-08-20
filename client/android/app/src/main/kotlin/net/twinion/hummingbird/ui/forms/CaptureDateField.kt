package net.twinion.hummingbird.ui.forms

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier

/** #529's third shared form component: one of the capture box's two
 * free-text dates (deadline, scheduled date) — a plain text field, deadline
 * and scheduled date reuse it for both the capture form and, later, the
 * Triage screen (#531).
 *
 * **No date regex lives here, or anywhere in this file.** What is wrong
 * with the typed text — a malformed shape, an impossible calendar date — is
 * the core's own answer
 * ([`uniffi.hummingbird_ffi_mobile.captureMetaProblems`], reached through
 * `hummingbird_core::decisions::capture::capture_meta_problems`), read by
 * the caller and passed in as [error]. This component only shows whatever
 * it is handed; it decides nothing about what a valid date looks like
 * (#529's own structural-test criterion: "no date regexes").
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
@Composable
fun CaptureDateField(
    label: String,
    value: String,
    error: String?,
    onValueChange: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    OutlinedTextField(
        value = value,
        onValueChange = onValueChange,
        label = { Text(label) },
        isError = error != null,
        supportingText = error?.let { { Text(it) } },
        modifier = modifier.fillMaxWidth(),
    )
}
