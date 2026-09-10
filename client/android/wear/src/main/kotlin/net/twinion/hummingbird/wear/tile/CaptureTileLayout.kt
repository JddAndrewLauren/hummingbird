package net.twinion.hummingbird.wear.tile

import android.content.Context
import androidx.wear.protolayout.ActionBuilders
import androidx.wear.protolayout.DeviceParametersBuilders.DeviceParameters
import androidx.wear.protolayout.LayoutElementBuilders.LayoutElement
import androidx.wear.protolayout.material3.materialScope
import androidx.wear.protolayout.material3.primaryLayout
import androidx.wear.protolayout.material3.text
import androidx.wear.protolayout.material3.textEdgeButton
import androidx.wear.protolayout.modifiers.clickable
import androidx.wear.protolayout.types.layoutString

// The capture tile's one layout (ADR-0039): a line that says what the tile
// does and an edge button that does it — a `LaunchAction` straight into
// `CaptureActivity`, which opens the system input chooser. The tile itself
// is static: no data, no refresh, nothing to be stale. Per-question tiles
// are a later slice (#129 keeps them), and their opt-in is decided to be a
// `settings` vocabulary rather than a stored tile record; this file will not
// grow them.
//
// `captureLaunchAction` is pure and pinned by `CaptureTileLayoutTest`: the
// package and class names are the two strings a wrong edit would break
// silently — a tile that draws and then does nothing on tap.

/** The tile's tap: launch capture, by the phone-shared package name and the
 * watch activity's class — both literal, because a `LaunchAction` crosses
 * process boundaries as strings, not as a `Class`. */
fun captureLaunchAction(): ActionBuilders.LaunchAction =
    ActionBuilders.LaunchAction.Builder()
        .setAndroidActivity(
            ActionBuilders.AndroidActivity.Builder()
                .setPackageName(CAPTURE_PACKAGE)
                .setClassName(CAPTURE_ACTIVITY)
                .build(),
        )
        .build()

const val CAPTURE_PACKAGE = "net.twinion.hummingbird"
const val CAPTURE_ACTIVITY = "net.twinion.hummingbird.wear.capture.CaptureActivity"

fun captureTileLayout(context: Context, deviceParameters: DeviceParameters): LayoutElement =
    materialScope(context, deviceParameters) {
        primaryLayout(
            mainSlot = { text("Say it — it lands in Triage.".layoutString) },
            bottomSlot = {
                textEdgeButton(onClick = clickable(captureLaunchAction(), id = "capture")) {
                    text("Capture".layoutString)
                }
            },
        )
    }
