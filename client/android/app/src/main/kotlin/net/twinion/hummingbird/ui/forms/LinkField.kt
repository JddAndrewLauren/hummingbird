package net.twinion.hummingbird.ui.forms

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import net.twinion.hummingbird.R

/** The item's one Link (#782): a chain-glyph row that discloses the two
 * fields behind it — the URL, and the optional name it is shown by. Shared
 * by both capture surfaces (below their details disclosure) and the item
 * pane's `LINK` section, for the same reason every other file in this
 * directory is shared: one field behind three doors must not be three
 * hand-copies.
 *
 * Kotlin decides nothing about the URL here. What a link is *called*
 * (`linkDisplayLabel`) and what a share seeds it with
 * (`parseSharePayload`) are the core's, crossed on the seam (ADR-0025);
 * this field only holds the two strings. A name without a URL is refused
 * at the ViewModel gate, never silently dropped.
 *
 * The disclosure opens shut unless told otherwise, matching the details
 * chevron's own resting state: a capture that carries no link should not
 * pay two fields for it. [initiallyOpen] is the share target's door — a
 * share carrying a URL arrives with the disclosure already open and filled,
 * so the reader sees what is about to be saved — and it is a
 * `rememberSaveable` *input*, so the seed landing after first composition
 * still opens the field; a URL already in the draft opens it as well. */
@Composable
fun LinkField(
    url: String,
    label: String,
    onUrlChange: (String) -> Unit,
    onLabelChange: (String) -> Unit,
    initiallyOpen: Boolean = false,
    modifier: Modifier = Modifier,
) {
    var open by rememberSaveable(initiallyOpen) { mutableStateOf(initiallyOpen || url.isNotEmpty()) }

    Column(
        modifier = modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        // The glyph is the control, and the words survive as its accessible
        // name — the same shape the details chevron takes on both capture
        // surfaces.
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(onClick = { open = !open }) {
                Icon(
                    painterResource(R.drawable.ic_link),
                    contentDescription = if (open) "Hide link" else "Add link",
                    modifier = Modifier.size(20.dp),
                )
            }
        }
        if (open) {
            OutlinedTextField(
                value = url,
                onValueChange = onUrlChange,
                label = { Text("URL") },
                singleLine = true,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
                modifier = Modifier.fillMaxWidth(),
            )
            OutlinedTextField(
                value = label,
                onValueChange = onLabelChange,
                label = { Text("Link name") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }
}
