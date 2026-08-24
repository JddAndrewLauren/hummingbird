package net.twinion.hummingbird.core

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import net.twinion.hummingbird.diagnostics.DiagnosticsRecorder
import uniffi.hummingbird_ffi_mobile.MobileDiagnosticEvent
import uniffi.hummingbird_ffi_mobile.MobileNetworkTransport

/**
 * #710's `network.changed`: registers one process-wide
 * `ConnectivityManager.NetworkCallback` (mirroring [DiagnosticsRecorder]'s
 * "exactly one, process-wide" discipline — see [start]) and records a
 * reading every time the active network's capabilities change, so "cellular
 * that appeared active and usable" becomes a checkable fact rather than an
 * unverified description of the incident (#710's own brief).
 *
 * **What is recorded, and what never is.** Transport is [transportOf]'s
 * closed collapse of [NetworkCapabilities.hasTransport]'s bits — cellular,
 * wifi and vpn checked in that order (a real network can carry more than
 * one; this picks the single most specific), `OTHER` for every other
 * transport bit Android defines (Bluetooth, Ethernet, USB, …), `NONE` when
 * there is no active network at all. Alongside it: the internet,
 * validated, metered and roaming capability bits, read straight off
 * [NetworkCapabilities]. Never an IP address, and never an SSID — an SSID
 * is as much a location fingerprint as an address (#710's brief calls this
 * out explicitly, since the acceptance list's own prose only names the
 * address), and this class has no code path that could read one in the
 * first place: it never touches `WifiManager`.
 *
 * **Not [NetworkStatus].** That object answers "is there a usable network
 * path right now" as a plain synchronous read, for
 * `sync_status_tone`/`_label`'s decision seam. This class answers a
 * different question — "what changed, and to what" — as a diagnostic event
 * stream, and the two are read at different times for different reasons;
 * neither should be rebuilt in terms of the other.
 */
object NetworkMonitor {
    @Volatile
    private var callback: ConnectivityManager.NetworkCallback? = null

    /** Registers the callback once per process — a second call while one
     * is already registered is a no-op, the same "exactly one" discipline
     * [DiagnosticsRecorder.get] documents for itself. Safe to call from
     * `HummingbirdApp.onCreate` on every launch. */
    fun start(context: Context) {
        if (callback != null) return
        synchronized(this) {
            if (callback != null) return
            val manager = context.getSystemService(Context.CONNECTIVITY_SERVICE) as? ConnectivityManager
                ?: return
            val recorder = DiagnosticsRecorder.get(context)
            val registered = object : ConnectivityManager.NetworkCallback() {
                override fun onCapabilitiesChanged(network: Network, capabilities: NetworkCapabilities) {
                    recorder.record(eventFor(capabilities))
                }

                override fun onLost(network: Network) {
                    recorder.record(noNetworkEvent())
                }
            }
            manager.registerDefaultNetworkCallback(registered)
            callback = registered
        }
    }

    /** Test-only: the mapping [start]'s callback would have recorded,
     * without needing a real `ConnectivityManager` — the mapping is the
     * part with real logic; wiring a live callback is Android framework
     * plumbing this app cannot unit-test meaningfully. */
    internal fun eventFor(capabilities: NetworkCapabilities): MobileDiagnosticEvent {
        val internetCapable = capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
        val validated = capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)
        return MobileDiagnosticEvent.NetworkChanged(
            online = internetCapable && validated,
            transport = transportOf(capabilities),
            internetCapable = internetCapable,
            validated = validated,
            metered = !capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED),
            roaming = !capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_ROAMING),
        )
    }

    internal fun noNetworkEvent(): MobileDiagnosticEvent =
        MobileDiagnosticEvent.NetworkChanged(
            online = false,
            transport = MobileNetworkTransport.NONE,
            internetCapable = false,
            validated = false,
            metered = false,
            roaming = false,
        )

    /** Cellular, then wifi, then vpn — the order #710's brief lists them
     * in, and the order this picks the single transport a multi-transport
     * network (a VPN over wifi, say) is reported as. `OTHER` for every
     * transport bit Android defines beyond those three (Bluetooth,
     * Ethernet, USB, Wi-Fi Aware, LoWPAN, …) — a closed catch-all rather
     * than growing this list, the same "closed value" rule the shared
     * `NetworkTransport` enum's own doc states. */
    internal fun transportOf(capabilities: NetworkCapabilities): MobileNetworkTransport =
        when {
            capabilities.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) -> MobileNetworkTransport.CELLULAR
            capabilities.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) -> MobileNetworkTransport.WIFI
            capabilities.hasTransport(NetworkCapabilities.TRANSPORT_VPN) -> MobileNetworkTransport.VPN
            else -> MobileNetworkTransport.OTHER
        }
}
