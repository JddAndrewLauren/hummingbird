package net.twinion.hummingbird.ui.theme

import android.provider.Settings
import androidx.compose.animation.core.AnimationSpec
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.spring
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.platform.LocalContext

// The motion half of the hand-ported design tokens (#801, ADR-0026's rule:
// Android hand-ports the mirror's tokens into Compose under a CI drift
// gate rather than copying the CSS). Colour and type had files of their
// own; motion did not, because until the board's drag nothing here animated
// with a design-system duration -- `StatusQuietStack`'s expand was the only
// spec in the app, and it carried its own literals plus the only
// reduced-motion read. Both now live here, and that function calls this one.
//
// `MotionTokenDriftTest` pins [DurBase] and [EaseFlit] against the mirror's
// `tokens/motion.css`, the same way `ColorTokenDriftTest` pins Color.kt.

/** `--dur-base`, in milliseconds. */
const val DurBase: Int = 200

/** `--ease-flit` (`cubic-bezier(.2,.8,.2,1)`), the system's default easing:
 * a hummingbird beats fast and stops dead. */
val EaseFlit: CubicBezierEasing = CubicBezierEasing(0.2f, 0.8f, 0.2f, 1f)

/** True when the device's animator scale is off — the platform's own
 * reduced-motion switch, which `prefers-reduced-motion` is the web's name
 * for. The design system collapses every duration token to 1ms there; a
 * spring has no token to collapse, so a caller carrying one places its
 * subject instantly instead.
 *
 * `remember`ed against the context: this is a `ContentResolver` read, and
 * unremembered it runs on every recomposition for a value that changes only
 * when the operator changes a system setting. */
@Composable
fun reducedMotion(): Boolean {
    val context = LocalContext.current
    return remember(context) {
        Settings.Global.getFloat(
            context.contentResolver,
            Settings.Global.ANIMATOR_DURATION_SCALE,
            1f,
        ) == 0f
    }
}

/** The spring a **carried** element hangs from — a board card being dragged
 * between columns (ADR-0021 decision 9). It lags behind the finger, tilts
 * into its own travel, overshoots once and settles within about 250ms of
 * release; that overshoot is the exception the design system's Motion
 * paragraph was amended for, and it is the whole character of the gesture.
 *
 * The web's integrator is `k = 520`, `c = 32` at unit mass, which is a
 * damping ratio of `c / (2*sqrt(k))` ~= 0.70 — so this is the same damped
 * model expressed in Compose's own terms, not a second set of numbers
 * chosen to look similar. Deliberately not a token: a spring is not a
 * duration, and `motion.css` gains no property for it. */
fun carrySpring(): AnimationSpec<Offset> =
    spring(dampingRatio = 0.7f, stiffness = 520f, visibilityThreshold = Offset(0.5f, 0.5f))

/** How far a carried card may lean, in degrees, and how much travel it
 * takes to get there. Capped so the lean never reads as spin — the web's
 * own `clamp(vx / 90, ±8)`. */
const val CarryTiltMaxDegrees: Float = 8f
const val CarryTiltPerPixelPerSecond: Float = 1f / 90f
