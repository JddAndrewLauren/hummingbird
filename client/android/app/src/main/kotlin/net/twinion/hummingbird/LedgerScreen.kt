package net.twinion.hummingbird

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.launch
import net.twinion.hummingbird.ui.StageBadge
import net.twinion.hummingbird.ui.adaptiveGridCells
import net.twinion.hummingbird.ui.theme.LocalHbDark
import uniffi.hummingbird_ffi_mobile.MobileLedgerRowRecord
import uniffi.hummingbird_ffi_mobile.MobileLedgerRowState

// The Ledger: the complete retained roster — every item the mirror has ever
// known, live, Done and archived alike, last touched first (M3/#532).
//
// This file decides nothing about a row. `LedgerViewModel`'s own doc
// carries the reasoning; ordering, state and the mark-done gate all arrive
// already applied — there is no `when` over `record.state` here beyond
// reading which arm it is, and no re-derivation of `canMarkDone`.
private const val ARCHIVED_ALPHA = 0.72f

@Composable
fun LedgerScreen(
    syncTick: Int = 0,
    onBack: () -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val viewModel: LedgerViewModel = viewModel(factory = LedgerViewModel.factory(context))
    val rows by viewModel.rows.collectAsState()
    val dark = LocalHbDark.current
    val loading by viewModel.loading.collectAsState()
    val statusLine by viewModel.statusLine.collectAsState()

    suspend fun reload() {
        viewModel.refresh(System.currentTimeMillis())
    }

    LaunchedEffect(Unit) { reload() }

    LifecycleResumeEffect(Unit) {
        val resumed = scope.launch { reload() }
        onPauseOrDispose { resumed.cancel() }
    }

    LaunchedEffect(syncTick) {
        if (syncTick > 0) reload()
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
                Text("Every item", style = MaterialTheme.typography.headlineLarge)
                TextButton(onClick = onBack) {
                    Text("Back to Now")
                }
            }

            statusLine?.let {
                Text(
                    it,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.error,
                )
            }

            when {
                loading && rows.isEmpty() -> CircularProgressIndicator()
                rows.isEmpty() -> Text(
                    "Nothing has ever been tracked. Every item the mirror has ever known " +
                        "appears here — archived ones included, labelled.",
                    style = MaterialTheme.typography.bodyLarge,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                // A grid since the unfolded slice: one fixed column on the
                // phone — today's list exactly — adaptive columns on a wide
                // window (`adaptiveGridCells`).
                else -> LazyVerticalGrid(
                    columns = adaptiveGridCells(),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    // The last row scrolls clear of the Capture FAB.
                    contentPadding = PaddingValues(bottom = 64.dp),
                ) {
                    items(rows, key = { it.id }) { row ->
                        LedgerRow(
                            dark = dark,
                            row = row,
                            onComplete = {
                                scope.launch { viewModel.complete(row.id, System.currentTimeMillis()) }
                            },
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun LedgerRow(
    row: MobileLedgerRowRecord,
    dark: Boolean,
    onComplete: () -> Unit,
) {
    val archived = row.state is MobileLedgerRowState.Archived
    Card(
        modifier = Modifier
            .fillMaxWidth()
            .alpha(if (archived) ARCHIVED_ALPHA else 1f),
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
                    // The stage goes through the one treatment (#557,
                    // `ui/StageBadge.kt`); an archived row substitutes the
                    // word "archived" instead — a deliberate divergence
                    // from web's `LedgerScreen.tsx`, which badges archived
                    // rows too and recedes them by opacity alone. Here the
                    // row is already receded and "archived" is the fact the
                    // reader needs; the stage a thing died in is not.
                    if (archived) {
                        Text(
                            "archived",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    } else {
                        StageBadge(stage = row.stage, dark = dark)
                    }
                    if (row.pending) {
                        Text(
                            "Pending",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    if (row.deadLettered) {
                        Text(
                            "edit didn't apply",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.error,
                        )
                    }
                    if (row.hasLiveAlert) {
                        Text(
                            "alert",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.error,
                        )
                    }
                }
            }
            // Live rows only: an archived row is outside the mirror's live
            // set, and an act on it would be `ItemNotFound` — the checkmark
            // never offers what `MobileTaskHost.act` would refuse.
            if (row.canMarkDone) {
                OutlinedButton(onClick = onComplete, enabled = !row.pending) {
                    Text("Done")
                }
            }
        }
    }
}
