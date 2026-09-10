package net.twinion.hummingbird.wear.tile

import android.content.Context
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.wear.protolayout.ActionBuilders
import androidx.wear.protolayout.ColorBuilders.argb
import androidx.wear.protolayout.DeviceParametersBuilders.DeviceParameters
import androidx.wear.protolayout.DimensionBuilders.degrees
import androidx.wear.protolayout.DimensionBuilders.dp
import androidx.wear.protolayout.DimensionBuilders.expand
import androidx.wear.protolayout.LayoutElementBuilders
import androidx.wear.protolayout.LayoutElementBuilders.LayoutElement
import androidx.wear.protolayout.material3.ButtonColors
import androidx.wear.protolayout.material3.ColorScheme
import androidx.wear.protolayout.material3.IconButtonStyle
import androidx.wear.protolayout.material3.MaterialScope
import androidx.wear.protolayout.material3.Typography
import androidx.wear.protolayout.material3.buttonGroup
import androidx.wear.protolayout.material3.icon
import androidx.wear.protolayout.material3.iconButton
import androidx.wear.protolayout.material3.materialScope
import androidx.wear.protolayout.material3.text
import androidx.wear.protolayout.modifiers.clickable
import androidx.wear.protolayout.types.LayoutColor
import androidx.wear.protolayout.types.layoutString
import net.twinion.hummingbird.ui.theme.BorderDefaultDark
import net.twinion.hummingbird.ui.theme.BorderSubtleDark
import net.twinion.hummingbird.ui.theme.CrimsonDark
import net.twinion.hummingbird.ui.theme.Ember200
import net.twinion.hummingbird.ui.theme.Ember500
import net.twinion.hummingbird.ui.theme.Ember600
import net.twinion.hummingbird.ui.theme.Ember900
import net.twinion.hummingbird.ui.theme.Ink300
import net.twinion.hummingbird.ui.theme.Ink400
import net.twinion.hummingbird.ui.theme.Ink700
import net.twinion.hummingbird.ui.theme.Ink800
import net.twinion.hummingbird.ui.theme.Ink900
import net.twinion.hummingbird.ui.theme.Ink950
import net.twinion.hummingbird.ui.theme.Sand100
import net.twinion.hummingbird.ui.theme.StatusDangerBgDark
import net.twinion.hummingbird.ui.theme.StatusInfoFgDark
import net.twinion.hummingbird.ui.theme.UrgencyOverdueDark
import net.twinion.hummingbird.ui.theme.UrgencySoonDark

// The capture tile's one layout (ADR-0039 §5 as amended by the 2026-09-10
// design handoff, option 1d): an urgency arc around the face's edge —
// overdue, then everything due within the core's three-day window — the
// mono count line, the ember disc with the feather (the brand's own verb
// for capture) that opens `CaptureActivity`, and two tonal glyph rounds
// beneath it that open the Items and Questions lists in `MainActivity`.
// No words anywhere else on the tile: the disc is what a thumb hits without
// looking, and the rounds say what they are by glyph.
//
// **Every colour is a `:brand` constant, through the tile's own
// `ColorScheme`** — `materialScope` with no scheme inherits the watch's
// dynamic palette, and the disc would not be ember (the handoff's own code
// note). `WearThemeStructuralTest`'s literal ban cannot see ProtoLayout's
// defaults, so the scheme below is that rule made explicit for the one
// surface Compose does not draw.
//
// The tile is dynamic now, and says so honestly: the count line yields to
// `SYNCED nH AGO` once the mirror is an hour old (`tileCountLine`), the
// service asks for an hourly freshness pass, and every completed sync asks
// for a redraw (`TileRefresh`). Per-question tiles are still a later slice
// (#129 keeps them, and their refresh budget), and this file will not grow
// them.
//
// `captureLaunchAction` and `routeLaunchAction` are pure and pinned by
// `CaptureTileLayoutTest`: the package, class and extra strings are the
// kind a wrong edit breaks silently — a tile that draws and then does
// nothing on tap.

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

/** A glyph round's tap: `MainActivity` with the list it should open over
 * home, as `MainActivity.EXTRA_ROUTE` reads it. */
fun routeLaunchAction(route: String): ActionBuilders.LaunchAction =
    ActionBuilders.LaunchAction.Builder()
        .setAndroidActivity(
            ActionBuilders.AndroidActivity.Builder()
                .setPackageName(CAPTURE_PACKAGE)
                .setClassName(MAIN_ACTIVITY)
                .addKeyToExtraMapping(
                    ROUTE_EXTRA,
                    ActionBuilders.AndroidStringExtra.Builder().setValue(route).build(),
                )
                .build(),
        )
        .build()

const val CAPTURE_PACKAGE = "net.twinion.hummingbird"
const val CAPTURE_ACTIVITY = "net.twinion.hummingbird.wear.capture.CaptureActivity"
const val MAIN_ACTIVITY = "net.twinion.hummingbird.wear.MainActivity"

/** `MainActivity.EXTRA_ROUTE`, restated: the extra key crosses as a string
 * and this file must not depend on the activity to name it. */
const val ROUTE_EXTRA = "net.twinion.hummingbird.wear.extra.ROUTE"
const val ROUTE_ITEMS = "items"
const val ROUTE_QUESTIONS = "questions"

/** The resource ids the layout names and `CaptureTileService` maps to
 * `:brand`'s drawables — the two halves must agree, so both read these. */
const val RES_FEATHER = "feather"
const val RES_ZAP = "zap"
const val RES_HELP_CIRCLE = "help-circle"

/** What the layout draws from: the counts (or none), and the sync age the
 * count line is judged against. */
data class TileFacts(val counts: TileCounts?, val lastInformativeAtMs: Long?, val nowMs: Long)

fun captureTileLayout(context: Context, deviceParameters: DeviceParameters, facts: TileFacts): LayoutElement =
    materialScope(context, deviceParameters, allowDynamicTheme = false, defaultColorScheme = tileColorScheme()) {
        val face = deviceParameters.screenWidthDp.toFloat()
        LayoutElementBuilders.Box.Builder()
            .setWidth(expand())
            .setHeight(expand())
            .setHorizontalAlignment(LayoutElementBuilders.HORIZONTAL_ALIGN_CENTER)
            .setVerticalAlignment(LayoutElementBuilders.VERTICAL_ALIGN_CENTER)
            .addContent(arcTrack())
            .addContent(urgencyArc(arcDegrees(facts.counts)))
            .addContent(centreColumn(face, facts))
            .build()
    }

/** The tile's palette — `WearTheme.kt`'s scheme, slot for slot, in
 * ProtoLayout's type. */
internal fun tileColorScheme(): ColorScheme = ColorScheme(
    primary = Ember500.layout(),
    primaryDim = Ember600.layout(),
    primaryContainer = Ember900.layout(),
    onPrimary = Color.White.layout(),
    onPrimaryContainer = Ember200.layout(),
    secondary = Ink300.layout(),
    secondaryDim = Ink400.layout(),
    secondaryContainer = Ink700.layout(),
    onSecondary = Ink950.layout(),
    onSecondaryContainer = Sand100.layout(),
    tertiary = StatusInfoFgDark.layout(),
    tertiaryDim = StatusInfoFgDark.layout(),
    tertiaryContainer = Ink700.layout(),
    onTertiary = Ink950.layout(),
    onTertiaryContainer = Sand100.layout(),
    surfaceContainerLow = Ink900.layout(),
    surfaceContainer = Ink800.layout(),
    surfaceContainerHigh = Ink700.layout(),
    onSurface = Sand100.layout(),
    onSurfaceVariant = Ink300.layout(),
    outline = BorderDefaultDark.layout(),
    outlineVariant = BorderSubtleDark.layout(),
    background = Ink950.layout(),
    onBackground = Sand100.layout(),
    error = CrimsonDark.layout(),
    errorDim = CrimsonDark.layout(),
    errorContainer = StatusDangerBgDark.layout(),
    onError = Ink950.layout(),
    onErrorContainer = CrimsonDark.layout(),
)

private fun Color.layout(): LayoutColor = LayoutColor(toArgb())

/** The faint full ring the segments sit on — `--border-subtle` dark, white
 * at seven percent, the design's track. */
private fun arcTrack(): LayoutElement =
    LayoutElementBuilders.Arc.Builder()
        .setAnchorAngle(degrees(0f))
        .setAnchorType(LayoutElementBuilders.ARC_ANCHOR_START)
        .addContent(
            LayoutElementBuilders.ArcLine.Builder()
                .setLength(degrees(360f))
                .setThickness(dp(ARC_THICKNESS_DP))
                .setColor(argb(BorderSubtleDark.toArgb()))
                .build(),
        )
        .build()

/** The two segments, from ten o'clock clockwise: overdue, a gap, soon. A
 * zero-length segment is not drawn at all — a round cap on nothing would
 * still leave a dot. */
private fun urgencyArc(lengths: ArcDegrees): LayoutElement {
    val arc = LayoutElementBuilders.Arc.Builder()
        .setAnchorAngle(degrees(ARC_START_DEGREES))
        .setAnchorType(LayoutElementBuilders.ARC_ANCHOR_START)
        .setArcDirection(LayoutElementBuilders.ARC_DIRECTION_CLOCKWISE)
    if (lengths.overdue > 0f) arc.addContent(segment(lengths.overdue, UrgencyOverdueDark))
    if (lengths.overdue > 0f && lengths.soon > 0f) {
        arc.addContent(LayoutElementBuilders.ArcSpacer.Builder().setLength(degrees(ARC_GAP_DEGREES)).build())
    }
    if (lengths.soon > 0f) arc.addContent(segment(lengths.soon, UrgencySoonDark))
    return arc.build()
}

private fun segment(length: Float, color: Color): LayoutElementBuilders.ArcLine =
    LayoutElementBuilders.ArcLine.Builder()
        .setLength(degrees(length))
        .setThickness(dp(ARC_THICKNESS_DP))
        .setColor(argb(color.toArgb()))
        .setStrokeCap(LayoutElementBuilders.STROKE_CAP_ROUND)
        .build()

/** The count line, the disc, the two rounds — vertically centred. Sizes are
 * the design's 284px face scaled to this one (`face` dp): the disc is
 * 116/284 of it, the rounds 44/284 with a floor at Wear's touch minimum,
 * the gap between them 40/284. */
private fun MaterialScope.centreColumn(face: Float, facts: TileFacts): LayoutElement {
    val column = LayoutElementBuilders.Column.Builder()
        .setHorizontalAlignment(LayoutElementBuilders.HORIZONTAL_ALIGN_CENTER)
    val line = tileCountLine(facts.counts, facts.lastInformativeAtMs, facts.nowMs)
    if (line != null) {
        column.addContent(
            text(line.layoutString, typography = Typography.LABEL_SMALL, color = Ink300.layout()),
        )
        column.addContent(gap(GAP_DP))
    }
    val disc = face * DISC_FRACTION
    column.addContent(
        iconButton(
            onClick = clickable(captureLaunchAction(), id = "capture"),
            iconContent = { icon(RES_FEATHER, width = dp(disc * DISC_ICON_FRACTION), height = dp(disc * DISC_ICON_FRACTION)) },
            width = dp(disc),
            height = dp(disc),
            colors = ButtonColors(containerColor = Ember500.layout(), iconColor = Color.White.layout()),
            style = IconButtonStyle.largeIconButtonStyle(),
        ),
    )
    column.addContent(gap(GAP_DP))
    val round = maxOf(face * ROUND_FRACTION, MIN_TOUCH_DP)
    column.addContent(
        buttonGroup(spacing = face * ROUND_GAP_FRACTION) {
            buttonGroupItem { glyphRound(RES_ZAP, ROUTE_ITEMS, round) }
            buttonGroupItem { glyphRound(RES_HELP_CIRCLE, ROUTE_QUESTIONS, round) }
        },
    )
    return column.build()
}

private fun MaterialScope.glyphRound(resource: String, route: String, size: Float): LayoutElement =
    iconButton(
        onClick = clickable(routeLaunchAction(route), id = route),
        iconContent = { icon(resource, width = dp(size * ROUND_ICON_FRACTION), height = dp(size * ROUND_ICON_FRACTION)) },
        width = dp(size),
        height = dp(size),
        colors = ButtonColors(containerColor = Ink700.layout(), iconColor = Sand100.layout()),
        style = IconButtonStyle.defaultIconButtonStyle(),
    )

private fun gap(heightDp: Float): LayoutElement =
    LayoutElementBuilders.Spacer.Builder().setHeight(dp(heightDp)).build()

private const val ARC_THICKNESS_DP = 4f
private const val GAP_DP = 8f
private const val DISC_FRACTION = 116f / 284f
private const val DISC_ICON_FRACTION = 54f / 116f
private const val ROUND_FRACTION = 44f / 284f
private const val ROUND_ICON_FRACTION = 20f / 44f
private const val ROUND_GAP_FRACTION = 40f / 284f
private const val MIN_TOUCH_DP = 48f

