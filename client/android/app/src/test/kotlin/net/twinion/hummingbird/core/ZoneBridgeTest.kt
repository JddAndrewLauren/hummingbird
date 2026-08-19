package net.twinion.hummingbird.core

import java.time.Instant
import java.time.ZoneId
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobileZoneFactValue
import uniffi.hummingbird_ffi_mobile.MobileZoneQuery
import uniffi.hummingbird_ffi_mobile.mobileZoneQueryKey

// The Android half of the pane lane's zone bridge (#533/#537), exercised
// directly against `java.time` — the same "no generated JNI binding
// involved" reasoning every other pure-Kotlin unit test in this app states.
class ZoneBridgeTest {

    @Test
    fun `a civil-date query resolves to the calendar date at the named zone`() {
        // 2026-08-15T23:30 UTC is still 2026-08-15 local at UTC-1, but
        // already 2026-08-16 at UTC+2 -- the same instant, two different
        // civil dates, pinning that the resolver reads the query's own
        // zone rather than the JVM's default.
        val atMs = Instant.parse("2026-08-15T23:30:00Z").toEpochMilli()
        val facts = ZoneBridge.resolve(
            listOf(
                MobileZoneQuery.CivilDate(zone = "Atlantic/Cape_Verde", atMs = atMs),
                MobileZoneQuery.CivilDate(zone = "Europe/Istanbul", atMs = atMs),
            ),
        )
        val capeVerde = facts.single { it.key == mobileZoneQueryKey(MobileZoneQuery.CivilDate("Atlantic/Cape_Verde", atMs)) }
        val istanbul = facts.single { it.key == mobileZoneQueryKey(MobileZoneQuery.CivilDate("Europe/Istanbul", atMs)) }
        assertEquals(MobileZoneFactValue.Date("2026-08-15"), capeVerde.value)
        assertEquals(MobileZoneFactValue.Date("2026-08-16"), istanbul.value)
    }

    @Test
    fun `a midnight query resolves to that civil dates own local-midnight instant`() {
        val query = MobileZoneQuery.Midnight(zone = "Europe/London", date = "2026-08-17")
        val facts = ZoneBridge.resolve(listOf(query))
        val fact = facts.single()
        assertEquals(mobileZoneQueryKey(query), fact.key)
        val expected =
            java.time.LocalDate.parse("2026-08-17").atStartOfDay(ZoneId.of("Europe/London")).toInstant().toEpochMilli()
        assertEquals(MobileZoneFactValue.Instant(expected), fact.value)
    }

    @Test
    fun `an unresolvable zone is omitted, never a null or fallback entry`() {
        val facts = ZoneBridge.resolve(
            listOf(MobileZoneQuery.CivilDate(zone = "Not/A_Real_Zone", atMs = 0L)),
        )
        assertTrue(facts.isEmpty())
    }

    @Test
    fun `a malformed midnight date is omitted rather than crashing the resolve`() {
        val facts = ZoneBridge.resolve(
            listOf(MobileZoneQuery.Midnight(zone = "Europe/London", date = "not-a-date")),
        )
        assertTrue(facts.isEmpty())
    }

    @Test
    fun `DEVICE_ZONE resolves to the runtimes own default zone, never a literal lookup`() {
        val atMs = Instant.parse("2026-08-15T12:00:00Z").toEpochMilli()
        val facts = ZoneBridge.resolve(
            listOf(MobileZoneQuery.CivilDate(zone = ZoneBridge.DEVICE_ZONE, atMs = atMs)),
        )
        val expected = Instant.ofEpochMilli(atMs).atZone(ZoneId.systemDefault()).toLocalDate().toString()
        assertEquals(MobileZoneFactValue.Date(expected), facts.single().value)
    }

    @Test
    fun `the device-local sentinel matches the core constant literal`() {
        // Pinned against `hummingbird_core::decisions::panes::zone::DEVICE_ZONE`
        // by review rather than a cross-language test — `zone-bridge.ts`'s
        // own `DEVICE_ZONE` constant carries the identical note.
        assertEquals("device-local", ZoneBridge.DEVICE_ZONE)
    }
}
