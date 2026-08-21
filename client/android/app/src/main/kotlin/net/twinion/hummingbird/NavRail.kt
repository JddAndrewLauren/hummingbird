package net.twinion.hummingbird

import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationRail
import androidx.compose.material3.NavigationRailItem
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp

/** The wide window's nav form — the web `NavRail.tsx`'s contract, ported:
 * every destination gets its own rail item, so there is no "More" here at
 * all and the More sheet stays phone-only by construction (only the bottom
 * bar opens it). Same glyphs, same labels, same `contentDescription = null`
 * reasoning as [BottomNavBar]: the visible label is the accessible name,
 * and a described icon would announce twice.
 *
 * No collapse toggle and no rail header/footer: the top bar — composed in
 * both modes — already owns the mark, the wordmark and the search trigger,
 * so a second copy on the rail would be the duplication #588 removed from
 * the screens. The rail never hides with the chrome either: unlike the
 * bars it costs the content no height, and the web rail is always visible
 * too. */
@Composable
internal fun HbNavRail(
    currentRoute: String?,
    onNavigate: (String) -> Unit,
) {
    NavigationRail {
        for (destination in NavDestination.entries) {
            NavigationRailItem(
                selected = destination.route == currentRoute,
                onClick = { onNavigate(destination.route) },
                icon = {
                    Icon(
                        painterResource(navIcon(destination)),
                        contentDescription = null,
                        modifier = Modifier.size(20.dp),
                    )
                },
                label = { Text(destination.label) },
                alwaysShowLabel = true,
            )
        }
    }
}
