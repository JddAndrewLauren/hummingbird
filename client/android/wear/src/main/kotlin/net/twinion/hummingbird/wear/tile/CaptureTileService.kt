package net.twinion.hummingbird.wear.tile

import androidx.concurrent.futures.CallbackToFutureAdapter
import androidx.wear.protolayout.ResourceBuilders
import androidx.wear.protolayout.TimelineBuilders
import androidx.wear.tiles.RequestBuilders
import androidx.wear.tiles.TileBuilders
import androidx.wear.tiles.TileService
import com.google.common.util.concurrent.ListenableFuture

// The capture tile (ADR-0039): one static timeline entry, no resources of
// its own (the edge button is text), and a version that never changes
// because nothing in the layout does. The system calls this on a binder
// thread and wants a future; the answer is ready, so the future completes
// inside the adapter's own callback.
class CaptureTileService : TileService() {

    override fun onTileRequest(requestParams: RequestBuilders.TileRequest): ListenableFuture<TileBuilders.Tile> =
        CallbackToFutureAdapter.getFuture { completer ->
            val layout = captureTileLayout(this, requestParams.deviceConfiguration)
            completer.set(
                TileBuilders.Tile.Builder()
                    .setResourcesVersion(RESOURCES_VERSION)
                    .setTileTimeline(TimelineBuilders.Timeline.fromLayoutElement(layout))
                    .build(),
            )
            "capture tile"
        }

    override fun onTileResourcesRequest(requestParams: RequestBuilders.ResourcesRequest): ListenableFuture<ResourceBuilders.Resources> =
        CallbackToFutureAdapter.getFuture { completer ->
            completer.set(ResourceBuilders.Resources.Builder().setVersion(RESOURCES_VERSION).build())
            "capture tile resources"
        }

    private companion object {
        const val RESOURCES_VERSION = "1"
    }
}
