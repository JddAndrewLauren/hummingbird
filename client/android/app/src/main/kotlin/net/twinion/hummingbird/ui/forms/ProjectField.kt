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
import uniffi.hummingbird_ffi_mobile.MobileProject

/** The details disclosure's Project picker (review finding on #529's own
 * PR): a read-only `ExposedDropdownMenuBox` over the real live
 * [projects][MobileProject] list plus a leading "No project" entry — the
 * same shape `CaptureBox.tsx:830-839`'s `Select` offers on the web side.
 * An opaque, hand-typed project id would mint locally and be refused at
 * the authority the first time it did not match a real row
 * (`items.project_id` is `TEXT REFERENCES projects(id)`,
 * `server/authority/src/schema.rs:135`) — exactly the dead-letter failure
 * `CaptureViewModel.canSubmitDraft`'s own doc says the two-predicate gate
 * exists to prevent, so this field must offer only ids that already exist.
 * `readOnly = true` on the anchor field is what makes that true: nothing
 * here lets a reader type a project id at all.
 *
 * Shared, like every other component in this directory, because both
 * capture surfaces now render the details disclosure — the sheet gained it
 * on operator decision 2026-08-20 — and a second hand-copy of a picker
 * whose whole job is refusing free text is a second place for that
 * refusal to lapse.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ProjectField(
    projects: List<MobileProject>,
    selectedId: String,
    onSelect: (String) -> Unit,
) {
    var expanded by remember { mutableStateOf(false) }
    val selectedName = projects.find { it.id == selectedId }?.name ?: "No project"

    ExposedDropdownMenuBox(
        expanded = expanded,
        onExpandedChange = { expanded = it },
    ) {
        OutlinedTextField(
            value = selectedName,
            onValueChange = {},
            readOnly = true,
            label = { Text("Project") },
            modifier = Modifier
                .fillMaxWidth()
                .menuAnchor(MenuAnchorType.PrimaryNotEditable),
        )
        ExposedDropdownMenu(
            expanded = expanded,
            onDismissRequest = { expanded = false },
        ) {
            DropdownMenuItem(
                text = { Text("No project") },
                onClick = {
                    onSelect("")
                    expanded = false
                },
            )
            for (project in projects) {
                DropdownMenuItem(
                    text = { Text(project.name) },
                    onClick = {
                        onSelect(project.id)
                        expanded = false
                    },
                )
            }
        }
    }
}
