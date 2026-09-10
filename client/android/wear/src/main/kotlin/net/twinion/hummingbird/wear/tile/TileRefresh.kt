package net.twinion.hummingbird.wear.tile

import android.content.Context
import androidx.wear.tiles.TileService

/** Asks the system to redraw the capture tile. Called after every completed
 * sync — the foreground leg in `MainActivity`, the background leg through
 * `SyncWorker.onRunFinished` (set by `WearApp`) — so the arc and the count
 * line follow the mirror, and never on a clock of their own: the hourly
 * freshness interval the service declares is the fallback, not a cadence
 * (the no-competing-clocks rule). Cheap and idempotent; the system coalesces. */
object TileRefresh {
    fun request(context: Context) {
        TileService.getUpdater(context.applicationContext).requestUpdate(CaptureTileService::class.java)
    }
}
