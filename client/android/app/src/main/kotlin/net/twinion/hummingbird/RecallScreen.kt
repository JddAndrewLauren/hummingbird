package net.twinion.hummingbird

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

// The Recall entry point (#541): the More sheet's "Search" row lands here.
// This is the reachability slot only — #542 replaces this body with the
// real search-as-you-type surface over the mobile seam's pre-grouped
// result rows. Nothing here groups or ranks anything; there is nothing to
// group or rank yet.
@Composable
fun RecallScreen(onBack: () -> Unit) {
    Scaffold { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .padding(24.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            TextButton(onClick = onBack) { Text("Back") }
            Text("Search", style = MaterialTheme.typography.headlineLarge)
            Text(
                "Search is coming soon.",
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}
