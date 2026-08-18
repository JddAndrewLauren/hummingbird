package net.twinion.hummingbird.ui.forms

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExposedDropdownMenuBox
import androidx.compose.material3.MenuAnchorType
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier

/** #529's second shared form component: the capture box's open-vocabulary
 * Context field — a text field with suggestions, never a picker
 * constraining what may be typed. CONTEXT.md: "an open vocabulary … because
 * the set of places a person works is theirs" — [suggestions] narrows the
 * dropdown, never the field's own value, and a value absent from
 * [suggestions] is exactly as valid as one present in it.
 *
 * `suggestions` comes from
 * [uniffi.hummingbird_ffi_mobile.captureFormMeta]'s `suggestedContexts` —
 * never a literal Kotlin list of `@home`/`@computer`/etc: Kotlin does not
 * learn what a valid (or even a suggested) context is (#529's own
 * structural-test criterion). The Triage screen (#531) reuses this
 * composable rather than growing a second combobox.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ContextField(
    value: String,
    onValueChange: (String) -> Unit,
    suggestions: List<String>,
    modifier: Modifier = Modifier,
) {
    var expanded by remember { mutableStateOf(false) }
    val filtered = if (value.isEmpty()) suggestions else suggestions.filter {
        it.contains(value, ignoreCase = true)
    }

    ExposedDropdownMenuBox(
        expanded = expanded && filtered.isNotEmpty(),
        onExpandedChange = { expanded = it },
        modifier = modifier,
    ) {
        OutlinedTextField(
            value = value,
            onValueChange = {
                onValueChange(it)
                expanded = true
            },
            label = { Text("Context") },
            modifier = Modifier
                .fillMaxWidth()
                .menuAnchor(MenuAnchorType.PrimaryEditable),
        )
        ExposedDropdownMenu(
            expanded = expanded && filtered.isNotEmpty(),
            onDismissRequest = { expanded = false },
        ) {
            for (suggestion in filtered) {
                DropdownMenuItem(
                    text = { Text(suggestion) },
                    onClick = {
                        onValueChange(suggestion)
                        expanded = false
                    },
                )
            }
        }
    }
}
