package net.twinion.hummingbird.core

import android.content.Context
import android.net.ConnectivityManager
import android.net.NetworkCapabilities

// A raw platform fact — "does this device currently have a usable
// network path" — read fresh at call time, never cached and never a
// decision: what an online/offline reading *means* for the sync card is
// `hummingbird_core::decisions::settings::sync_status_tone`/`_label`'s
// answer (#535), reached only through the seam. This is the one Kotlin-side
// input that answer takes, the same role `SkillRunner.kt`'s transport
// reports occupy for the skills lane (ADR-0025: the transport/platform
// read stays per-client, what it *means* sinks).
object NetworkStatus {
    fun isOnline(context: Context): Boolean {
        val manager = context.getSystemService(Context.CONNECTIVITY_SERVICE) as? ConnectivityManager
            ?: return false
        val network = manager.activeNetwork ?: return false
        val capabilities = manager.getNetworkCapabilities(network) ?: return false
        return capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
    }
}
