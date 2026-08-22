package net.twinion.hummingbird.ui

import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.runtime.Composable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.unit.dp

/** The shell's one breakpoint — the web's `PHONE_MAX_WIDTH_PX`
 * (`client/web/src/shell/breakpoints.ts`), dp-for-px: 640 sits above the
 * largest phone in portrait and above the Fold's 443dp cover display, and
 * below the Fold's unfolded inner display, so the folded app is the phone
 * form and the unfolded app is the wide one with no third state between.
 * `WindowWidthStructuralTest` pins the two equal, the same way
 * `responsive-breakpoint.test.ts` pins the web pair. Deliberately not
 * material3-adaptive's 600/840: those would contradict the parity source. */
internal const val PHONE_MAX_WIDTH_DP = 640

/** Whether this window is past the breakpoint. A `Configuration` read is
 * exact on the install target — fold/unfold recreates the Activity, so the
 * value cannot go stale mid-composition. */
@Composable
fun isWideWindow(): Boolean = LocalConfiguration.current.screenWidthDp > PHONE_MAX_WIDTH_DP

/** The resolved wide/phone answer, provided once in `AppRoot` (the
 * `LocalHbDark` precedent) so every screen reads the same fact rather than
 * each re-deriving it from the configuration. Defaults false: an unprovided
 * read renders the phone form, which is the layout every screen already
 * had. */
val LocalWideWindow = staticCompositionLocalOf { false }

/** The list screens' grid shape: one fixed column on the phone — which
 * makes the phone rendering identical to the single-column list it
 * replaces — and adaptive 320dp-minimum columns on a wide window.
 * `Adaptive`, not `Fixed(2)`, so the count follows the window rather than
 * hardcoding the Fold's. */
@Composable
fun adaptiveGridCells(): GridCells =
    if (LocalWideWindow.current) GridCells.Adaptive(320.dp) else GridCells.Fixed(1)
