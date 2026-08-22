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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.launch
import net.twinion.hummingbird.ui.adaptiveGridCells
import uniffi.hummingbird_ffi_mobile.MobileDoneRecord

// The Done screen (M3's one real sink, #532): every live Done item,
// most-recently-touched first, read-only. This file decides nothing —
// `DoneViewModel`'s own doc carries the reasoning; the ordering and the
// membership rule both arrive already applied.
@Composable
fun DoneScreen(
    syncTick: Int = 0,
    onBack: () -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val viewModel: DoneViewModel = viewModel(factory = DoneViewModel.factory(context))
    val items by viewModel.items.collectAsState()
    val loading by viewModel.loading.collectAsState()
    val statusLine by viewModel.statusLine.collectAsState()

    LaunchedEffect(Unit) { viewModel.refresh() }

    LifecycleResumeEffect(Unit) {
        val resumed = scope.launch { viewModel.refresh() }
        onPauseOrDispose { resumed.cancel() }
    }

    LaunchedEffect(syncTick) {
        if (syncTick > 0) viewModel.refresh()
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
                Text("Done", style = MaterialTheme.typography.headlineLarge)
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
                loading && items.isEmpty() -> CircularProgressIndicator()
                items.isEmpty() -> Text(
                    "Nothing completed yet. Items you complete land here and stay until cancelled.",
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
                    items(items, key = { it.id }) { record ->
                        DoneRow(record)
                    }
                }
            }
        }
    }
}

@Composable
private fun DoneRow(record: MobileDoneRecord) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(12.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(record.title, style = MaterialTheme.typography.bodyLarge)
            if (record.pending) {
                Text(
                    "Pending",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}
