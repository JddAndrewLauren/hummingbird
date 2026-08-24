package net.twinion.hummingbird

import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.NavigationRail
import androidx.compose.material3.NavigationRailItem
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import net.twinion.hummingbird.ui.theme.LocalHbDark
import uniffi.hummingbird_ffi_mobile.MobilePaneBand

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
    statusAlarm: MobilePaneBand? = null,
) {
    val alarmColor = navAlarmColor(statusAlarm, LocalHbDark.current)
    NavigationRail {
        for (destination in NavDestination.entries) {
            // Only Status carries an alarm — the same one-destination rule
            // [BottomNavBar] applies, and the same tint over glyph and
            // label together. Material draws the selected item its own
            // indicator pill, so the colour never has to double as the
            // "you are here" signal.
            val tint = if (destination == NavDestination.STATUS) alarmColor else null
            NavigationRailItem(
                selected = destination.route == currentRoute,
                onClick = { onNavigate(destination.route) },
                icon = {
                    Icon(
                        painterResource(navIcon(destination)),
                        contentDescription = null,
                        modifier = Modifier.size(20.dp),
                        tint = tint ?: LocalContentColor.current,
                    )
                },
                label = { Text(destination.label, color = tint ?: Color.Unspecified) },
                alwaysShowLabel = true,
            )
        }
    }
}
