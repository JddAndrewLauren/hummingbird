package net.twinion.hummingbird

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import uniffi.hummingbird_ffi_mobile.MobileRecallGroup
import uniffi.hummingbird_ffi_mobile.MobileRecallRowRecord

// Recall (#542, #478/CONTEXT.md): re-find one known item across everything
// the mirror has ever known, live or archived — the phone's counterpart to
// `RecallOverlay.tsx`, reached from the More sheet's "Search everything"
// row (#541). Search-as-you-type, no debounce (`RecallViewModel`'s own
// doc); tapping a live row opens that item's own detail screen through
// [onOpenItem] — the same door `NowScreen`'s own rows already use. A Done
// or archived row is shown, labelled, dimmed, and not tappable (#597):
// this slice ships no inline edit the way web's #479 does, and the alpha
// is what says so — an inert row at full opacity carries every affordance
// cue of the live row above it and answers none of them.
//
// This file decides nothing about a row or the result set.
// [MobileRecallRowRecord]s arrive already matched, grouped and ordered;
// `total` is the seam's own un-capped count, never re-derived from
// `rows.size`. There is no sort, filter or group-by anywhere below —
// `RecallScreenStructuralTest` gates that.

/** The alpha of a row that answers no tap — Done and archived alike
 * (#597): the dimming tracks inertness, not the archive flag, so the two
 * inert groups read the same and only the tappable one reads solid.
 * `LedgerScreen` declares the same 0.72 for its own archived rows. */
private const val INERT_ALPHA = 0.72f

@Composable
fun RecallScreen(
    onBack: () -> Unit,
    onOpenItem: (String) -> Unit,
) {
    val context = LocalContext.current
    val viewModel: RecallViewModel = viewModel(factory = RecallViewModel.factory(context))
    val query by viewModel.query.collectAsState()
    val rows by viewModel.rows.collectAsState()
    val total by viewModel.total.collectAsState()
    val loading by viewModel.loading.collectAsState()
    val statusLine by viewModel.statusLine.collectAsState()

    LaunchedEffect(query) {
        viewModel.search(System.currentTimeMillis())
    }

    Scaffold { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .padding(24.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text("Search", style = MaterialTheme.typography.headlineLarge)
                TextButton(onClick = onBack) { Text("Back") }
            }

            OutlinedTextField(
                value = query,
                onValueChange = { viewModel.setQueryText(it) },
                label = { Text("Title, notes, project, or hb-42") },
                modifier = Modifier.fillMaxWidth(),
            )

            statusLine?.let {
                Text(
                    it,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.error,
                )
            }

            val shown = rows.size.toUInt()
            val more = if (total > shown) total - shown else 0u

            when {
                query.isBlank() -> Text(
                    "Live, Done and archived items — everything the mirror has ever known.",
                    style = MaterialTheme.typography.bodyLarge,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                loading && rows.isEmpty() -> CircularProgressIndicator()
                rows.isEmpty() -> Text(
                    "Nothing matched. Every word has to appear, or type a handle like hb-42.",
                    style = MaterialTheme.typography.bodyLarge,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                else -> LazyColumn(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    items(rows, key = { it.id }) { row ->
                        RecallRow(
                            row = row,
                            onClick = if (row.group == MobileRecallGroup.LIVE) {
                                { onOpenItem(row.id) }
                            } else {
                                null
                            },
                        )
                    }
                    if (more > 0u) {
                        item {
                            Text(
                                "$more more matched — narrow the words to see them",
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                }
            }
        }
    }
}

private fun groupLabel(group: MobileRecallGroup): String = when (group) {
    MobileRecallGroup.LIVE -> "live"
    MobileRecallGroup.DONE -> "done"
    MobileRecallGroup.ARCHIVED -> "archived"
}

@Composable
private fun RecallRow(
    row: MobileRecallRowRecord,
    onClick: (() -> Unit)?,
) {
    // Exhaustive over the groups, no `else` arm (the structural test's
    // rule for a `uniffi::Enum` crossing): a fourth group added core-side
    // must be answered for here, not silently full-opacity.
    val alpha = when (row.group) {
        MobileRecallGroup.LIVE -> 1f
        MobileRecallGroup.DONE -> INERT_ALPHA
        MobileRecallGroup.ARCHIVED -> INERT_ALPHA
    }
    Card(
        modifier = Modifier
            .fillMaxWidth()
            .alpha(alpha)
            .let { if (onClick != null) it.clickable(onClick = onClick) else it },
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(12.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Text(row.title, style = MaterialTheme.typography.bodyLarge)
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(
                        groupLabel(row.group),
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    if (row.pending) {
                        Text(
                            "Pending",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }
        }
    }
}
