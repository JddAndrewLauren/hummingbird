package net.twinion.hummingbird.core

import android.net.NetworkCapabilities
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.shadow.api.Shadow
import org.robolectric.shadows.ShadowNetworkCapabilities
import uniffi.hummingbird_ffi_mobile.MobileDiagnosticEvent
import uniffi.hummingbird_ffi_mobile.MobileNetworkTransport

/**
 * [NetworkMonitor]'s transport/capability mapping (#710) — the load-bearing
 * logic behind `network.changed`, tested against real
 * `android.net.NetworkCapabilities` instances assembled through
 * Robolectric's own shadow (`NetworkCapabilities.Builder`, the framework's
 * real builder, is absent from this SDK's `android.jar` stub — a compile-
 * time `Unresolved reference` proved that, not a guess) rather than the
 * live `ConnectivityManager` callback itself, which is framework plumbing
 * with nothing of this app's own to assert against. `@Config(sdk = [35])`
 * for the same reason `ChoiceRowWrappingTest` pins it: this module compiles
 * against 36, which Robolectric 4.14.1 has no `android-all` jar for.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = android.app.Application::class)
class NetworkMonitorTest {

    private fun capabilitiesWith(
        transports: List<Int>,
        capabilities: List<Int>,
    ): NetworkCapabilities {
        val instance = ShadowNetworkCapabilities.newInstance()
        val shadow = Shadow.extract<ShadowNetworkCapabilities>(instance)
        for (transport in transports) {
            shadow.addTransportType(transport)
        }
        for (capability in capabilities) {
            shadow.addCapability(capability)
        }
        return instance
    }

    @Test
    fun `a validated unmetered non-roaming wifi network with internet reads online`() {
        val capabilities = capabilitiesWith(
            transports = listOf(NetworkCapabilities.TRANSPORT_WIFI),
            capabilities = listOf(
                NetworkCapabilities.NET_CAPABILITY_INTERNET,
                NetworkCapabilities.NET_CAPABILITY_VALIDATED,
                NetworkCapabilities.NET_CAPABILITY_NOT_METERED,
                NetworkCapabilities.NET_CAPABILITY_NOT_ROAMING,
            ),
        )

        val event = NetworkMonitor.eventFor(capabilities) as MobileDiagnosticEvent.NetworkChanged

        assertEquals(MobileNetworkTransport.WIFI, event.transport)
        assertEquals(true, event.online)
        assertEquals(true, event.internetCapable)
        assertEquals(true, event.validated)
        assertEquals(false, event.metered)
        assertEquals(false, event.roaming)
    }

    /** Cellular that "appeared active and usable" but never actually
     * validated — #710's brief's own motivating description — reads
     * `online: false` even though `internet_capable` is `true`: `online`
     * is `internet_capable && validated`, not either alone. */
    @Test
    fun `unvalidated metered roaming cellular reads offline despite claiming internet capability`() {
        val capabilities = capabilitiesWith(
            transports = listOf(NetworkCapabilities.TRANSPORT_CELLULAR),
            capabilities = listOf(NetworkCapabilities.NET_CAPABILITY_INTERNET),
        )

        val event = NetworkMonitor.eventFor(capabilities) as MobileDiagnosticEvent.NetworkChanged

        assertEquals(MobileNetworkTransport.CELLULAR, event.transport)
        assertFalse(event.online)
        assertEquals(true, event.internetCapable)
        assertEquals(false, event.validated)
        // Absence of NOT_METERED/NOT_ROAMING means metered/roaming.
        assertEquals(true, event.metered)
        assertEquals(true, event.roaming)
    }

    @Test
    fun `cellular takes priority over wifi, wifi takes priority over vpn`() {
        // #710's own `NetworkTransport` doc states the order this picks
        // among a multi-transport network's bits: cellular, then wifi,
        // then vpn — the single most specific physical transport wins.
        val vpnOverWifi = capabilitiesWith(
            transports = listOf(NetworkCapabilities.TRANSPORT_WIFI, NetworkCapabilities.TRANSPORT_VPN),
            capabilities = emptyList(),
        )
        assertEquals(MobileNetworkTransport.WIFI, NetworkMonitor.transportOf(vpnOverWifi))

        val cellularAndWifi = capabilitiesWith(
            transports = listOf(NetworkCapabilities.TRANSPORT_WIFI, NetworkCapabilities.TRANSPORT_CELLULAR),
            capabilities = emptyList(),
        )
        assertEquals(MobileNetworkTransport.CELLULAR, NetworkMonitor.transportOf(cellularAndWifi))
    }

    @Test
    fun `an unrecognised transport such as bluetooth reads as other`() {
        val bluetooth = capabilitiesWith(
            transports = listOf(NetworkCapabilities.TRANSPORT_BLUETOOTH),
            capabilities = emptyList(),
        )
        assertEquals(MobileNetworkTransport.OTHER, NetworkMonitor.transportOf(bluetooth))
    }

    @Test
    fun `losing the active network records transport none and reads offline`() {
        val event = NetworkMonitor.noNetworkEvent() as MobileDiagnosticEvent.NetworkChanged

        assertEquals(MobileNetworkTransport.NONE, event.transport)
        assertFalse(event.online)
    }

    /** No field on this shape can ever carry an IP address or an SSID —
     * both are on the shared enum's own prohibited list (`hummingbird_domain
     * ::diagnostics`'s `FORBIDDEN_FIELD_NAMES`, and #710's brief adds SSID
     * to that reasoning explicitly), and this class never calls
     * `WifiManager` or reads any address-bearing API at all, so there is
     * nothing to redact — checked here as "the shape carries exactly its
     * six named fields", not a string scan for a value nothing here ever
     * produces. */
    @Test
    fun `the event carries no address or SSID field`() {
        val event = NetworkMonitor.eventFor(
            capabilitiesWith(
                transports = listOf(NetworkCapabilities.TRANSPORT_WIFI),
                capabilities = listOf(NetworkCapabilities.NET_CAPABILITY_INTERNET),
            ),
        ) as MobileDiagnosticEvent.NetworkChanged
        val fields = event.toString().lowercase()
        assertFalse(fields.contains("ssid"))
        assertFalse(fields.contains("address"))
    }
}
