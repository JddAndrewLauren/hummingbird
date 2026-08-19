package net.twinion.hummingbird

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Scaffold
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

// The full-screen route around `ItemDetailPanel` — the door a tapped
// `item-threshold/v1` notification and a Recall row open (ADR-0027;
// `NavigationStructuralTest` pins the notification leg's `popUpTo`).
// Everything the item shows and does lives in the panel, which Now's
// inline expansion renders too; this host only owns what a route owns —
// the screen chrome, the scroll (the panel's header says why the host
// must), and the word "Back".
@Composable
fun ItemDetailScreen(
    itemId: String,
    syncTick: Int = 0,
    onBack: () -> Unit,
    onGrill: (String) -> Unit = {},
) {
    Scaffold { padding ->
        ItemDetailPanel(
            itemId = itemId,
            syncTick = syncTick,
            closeLabel = "Back",
            onClose = onBack,
            onGrill = onGrill,
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .padding(24.dp)
                .verticalScroll(rememberScrollState()),
        )
    }
}
