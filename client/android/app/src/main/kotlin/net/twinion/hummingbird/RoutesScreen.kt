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

// Routes' live empty state only (#541, M4's acceptance slice). No core call
// backs this screen: `client/ffi-mobile/src/lib.rs` exposes no Route query
// at all yet, so there is nothing to fetch and nothing to be stale — this
// is the honest, permanent shape until a route-minting slice lands, not a
// loading state waiting on one.
//
// The web's `RoutesScreen.tsx` carries a second, populated branch, but that
// branch reads a demo fixture (`demo.route`/`demo.items`) with no live
// counterpart — parity with a fixture is not parity, so it is not ported
// here. This screen renders only what `RoutesScreen.tsx` shows when its own
// `demo` prop is `null`.
@Composable
fun RoutesScreen(onBack: () -> Unit) {
    Scaffold { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .padding(24.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            TextButton(onClick = onBack) { Text("Back") }
            Text("No routes yet", style = MaterialTheme.typography.headlineLarge)
            Text(
                "A Route holds a project's Destination, its Fog, and the actions " +
                    "minted toward it.",
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}
