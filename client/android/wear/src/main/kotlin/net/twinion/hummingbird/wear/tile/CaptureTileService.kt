package net.twinion.hummingbird.wear.tile

import android.content.Context
import android.util.Log
import androidx.concurrent.futures.CallbackToFutureAdapter
import androidx.wear.protolayout.ResourceBuilders
import androidx.wear.protolayout.TimelineBuilders
import androidx.wear.tiles.RequestBuilders
import androidx.wear.tiles.TileBuilders
import androidx.wear.tiles.TileService
import com.google.common.util.concurrent.ListenableFuture
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
// per-sync asks. Three image resources, `:brand`'s vectors, under a version
// that changes when the set does. The system calls `onTileRequest` on a
// binder thread and wants a future; the mirror read is a suspend call, so
// it runs on the service's own scope and completes the adapter's future
// when done.
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
                    .addIdToImageMapping(RES_FEATHER, drawable(R.drawable.ic_feather))
                    .addIdToImageMapping(RES_ZAP, drawable(R.drawable.ic_zap))
                    .addIdToImageMapping(RES_HELP_CIRCLE, drawable(R.drawable.ic_help_circle))
                    .build(),
            )
            "capture tile resources"
        }

    private fun drawable(resId: Int): ResourceBuilders.ImageResource =
        ResourceBuilders.ImageResource.Builder()
            .setAndroidResourceByResId(
                ResourceBuilders.AndroidImageResourceByResId.Builder().setResourceId(resId).build(),
            )
            .build()

    private companion object {
        /** Bumped from "1" when the tile gained its three glyphs. */
        const val RESOURCES_VERSION = "2"
        val FRESHNESS_MS: Long = TimeUnit.HOURS.toMillis(1)
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
