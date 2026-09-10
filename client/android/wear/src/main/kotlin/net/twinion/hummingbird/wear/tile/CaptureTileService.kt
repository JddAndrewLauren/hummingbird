package net.twinion.hummingbird.wear.tile

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.util.Log
import androidx.concurrent.futures.CallbackToFutureAdapter
import androidx.wear.protolayout.ResourceBuilders
import androidx.wear.protolayout.TimelineBuilders
import androidx.wear.tiles.RequestBuilders
import androidx.wear.tiles.TileBuilders
import androidx.wear.tiles.TileService
import com.google.common.util.concurrent.ListenableFuture
import java.io.ByteArrayOutputStream
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import net.twinion.hummingbird.brand.R
import net.twinion.hummingbird.core.CoreHolder
import net.twinion.hummingbird.core.SyncHistoryStore
import net.twinion.hummingbird.core.TokenStore
import net.twinion.hummingbird.core.WallClock
import uniffi.hummingbird_ffi_mobile.MobileCalmOrder
import uniffi.hummingbird_ffi_mobile.MobileFrontierAxis
import uniffi.hummingbird_ffi_mobile.NowFacetSelectionRecord

// The capture tile (ADR-0039 §5 as amended by the 2026-09-10 design
// handoff): one timeline entry drawn from the mirror — the frontier's
// urgency counts (`nowBoard` on the `Urgency` axis, the same door the
// Items screen reads) and the sync history's last informative cycle — with
// an hourly freshness interval as the fallback behind `TileRefresh`'s
// per-sync asks. Three image resources — `:brand`'s vectors, **rasterised
// here and sent inline as PNG**, because the renderer (sysui's own process
// on Wear OS 7.0) answered `Resources$NotFoundException: File
// res/drawable/ic_feather.xml` to the same drawables by resource id on the
// emulator pass; a bitmap the renderer only has to decode cannot fail that
// way — under a version that changes when the set does. The system calls
// `onTileRequest` on a binder thread and wants a future; the mirror read is
// a suspend call, so it runs on the service's own scope and completes the
// adapter's future when done.
//
// No token yet — or a core that will not open — draws the disc alone: the
// counts are `null`, the count line is absent, the arc is empty. The tile
// never invents a number.
class CaptureTileService : TileService() {

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    override fun onDestroy() {
        scope.cancel()
        super.onDestroy()
    }

    override fun onTileRequest(requestParams: RequestBuilders.TileRequest): ListenableFuture<TileBuilders.Tile> =
        CallbackToFutureAdapter.getFuture { completer ->
            scope.launch {
                val facts = readTileFacts(this@CaptureTileService, System.currentTimeMillis())
                val layout = captureTileLayout(this@CaptureTileService, requestParams.deviceConfiguration, facts)
                completer.set(
                    TileBuilders.Tile.Builder()
                        .setResourcesVersion(RESOURCES_VERSION)
                        .setFreshnessIntervalMillis(FRESHNESS_MS)
                        .setTileTimeline(TimelineBuilders.Timeline.fromLayoutElement(layout))
                        .build(),
                )
            }
            "capture tile"
        }

    override fun onTileResourcesRequest(requestParams: RequestBuilders.ResourcesRequest): ListenableFuture<ResourceBuilders.Resources> =
        CallbackToFutureAdapter.getFuture { completer ->
            completer.set(
                ResourceBuilders.Resources.Builder()
                    .setVersion(RESOURCES_VERSION)
                    .addIdToImageMapping(RES_FEATHER, glyph(R.drawable.ic_feather))
                    .addIdToImageMapping(RES_ZAP, glyph(R.drawable.ic_zap))
                    .addIdToImageMapping(RES_HELP_CIRCLE, glyph(R.drawable.ic_help_circle))
                    .build(),
            )
            "capture tile resources"
        }

    /** One of `:brand`'s vectors drawn into a square bitmap at [GLYPH_PX]
     * (white strokes, as the drawables are authored; the layout's tint is
     * what colours them) and encoded as PNG for the renderer to decode. */
    private fun glyph(resId: Int): ResourceBuilders.ImageResource {
        val drawable = resources.getDrawable(resId, null)
        val bitmap = Bitmap.createBitmap(GLYPH_PX, GLYPH_PX, Bitmap.Config.ARGB_8888)
        drawable.setBounds(0, 0, GLYPH_PX, GLYPH_PX)
        drawable.draw(Canvas(bitmap))
        val png = ByteArrayOutputStream().also { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }.toByteArray()
        bitmap.recycle()
        return ResourceBuilders.ImageResource.Builder()
            .setInlineResource(
                ResourceBuilders.InlineImageResource.Builder()
                    .setData(png)
                    .setWidthPx(GLYPH_PX)
                    .setHeightPx(GLYPH_PX)
                    .setFormat(ResourceBuilders.IMAGE_FORMAT_UNDEFINED)
                    .build(),
            )
            .build()
    }

    private companion object {
        /** "1" was the static tile with no resources; "2" the three glyphs
         * by resource id, which the renderer could not load; "3" the same
         * three inline. */
        const val RESOURCES_VERSION = "3"
        val FRESHNESS_MS: Long = TimeUnit.HOURS.toMillis(1)

        /** Comfortably above the largest glyph the layout draws (the disc's,
         * about 43dp on a 454px face at density 2 — 86px); the renderer
         * scales down, never up. */
        const val GLYPH_PX = 144
    }
}

/** The tile's facts, read once per request: the counts when there is a
 * token and a core to count from (`null` otherwise — never a guessed zero),
 * and the sync history the count line is judged against. A core that
 * throws is logged and read as no counts; a tile must never crash the
 * launcher's carousel. */
internal suspend fun readTileFacts(context: Context, nowMs: Long): TileFacts {
    val counts = if (TokenStore.load(context) == null) {
        null
    } else {
        try {
            val board = CoreHolder.get(context).nowBoard(
                MobileFrontierAxis.URGENCY,
                NowFacetSelectionRecord(emptyList(), emptyList(), emptyList(), emptyList()),
                WallClock.local(nowMs),
                MobileCalmOrder.OLDEST,
            )
            tileCounts(board)
        } catch (failed: Exception) {
            Log.w("hummingbird.wear", "capture tile could not read the mirror", failed)
            null
        }
    }
    return TileFacts(counts, SyncHistoryStore.load(context).latestInformativeAtMs, nowMs)
}
