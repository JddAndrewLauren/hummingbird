package net.twinion.hummingbird.core

import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import uniffi.hummingbird_ffi_mobile.MobileZoneFact
import uniffi.hummingbird_ffi_mobile.MobileZoneFactValue
import uniffi.hummingbird_ffi_mobile.MobileZoneQuery
import uniffi.hummingbird_ffi_mobile.mobileZoneQueryKey

/** The **Android half of the pane lane's zone bridge** (#533/#537/M4,
 * ADR-0025) — `client/web/src/screens/questions/zone-bridge.ts`'s twin,
 * over `java.time` instead of `Intl`.
 *
 * `hummingbird-core` owns no tzdb, deliberately (`client/core/Cargo.toml`'s
 * `chrono-tz` note), so a pane's civil-date reasoning crosses two-phase: the
 * core names every `(zone, civil-date)` fact it needs
 * ([uniffi.hummingbird_ffi_mobile.MobileTaskHost.paneZoneQueries]), this
 * object resolves them, and the core decides against the answers
 * ([uniffi.hummingbird_ffi_mobile.MobileTaskHost.rankPanes]). This object
 * contributes one lookup and no judgement — it has no opinion about what an
 * unresolvable zone means, only whether `java.time` can resolve it.
 *
 * **An unresolvable zone is OMITTED, not nulled** — the same protocol
 * `zone-bridge.ts` documents: a query this runtime cannot answer is simply
 * absent from the returned list, and the core turns that absence into its
 * own gap (`WasteGap::UnresolvableZone`, `WeekendGap::UnresolvableZone`, …).
 *
 * `DEVICE_ZONE` (`hummingbird_core::decisions::panes::zone::DEVICE_ZONE`,
 * `"device-local"`) is the one sentinel this object gives meaning to,
 * resolving it to `ZoneId.systemDefault()` before answering — no other
 * caller ever sees that string.
 */
object ZoneBridge {

    /** `hummingbird_core::decisions::panes::zone::DEVICE_ZONE` — pinned by
     * `ZoneBridgeTest`'s own literal rather than crossed on every call,
     * since the sentinel never changes at runtime (`zone-bridge.ts`'s own
     * reasoning for its matching constant). */
    const val DEVICE_ZONE: String = "device-local"

    private fun resolveZoneId(zone: String): ZoneId? {
        val name = if (zone == DEVICE_ZONE) ZoneId.systemDefault().id else zone
        return try {
            ZoneId.of(name)
        } catch (malformed: Exception) {
            null
        }
    }

    /** Resolve every query this runtime knows how to, and omit the rest —
     * [uniffi.hummingbird_ffi_mobile.MobileTaskHost.rankPanes]'s
     * `zoneFacts` argument. */
    fun resolve(queries: List<MobileZoneQuery>): List<MobileZoneFact> = queries.mapNotNull { query ->
        val zoneId = resolveZoneId(zoneOf(query)) ?: return@mapNotNull null
        val value = when (query) {
            is MobileZoneQuery.CivilDate -> {
                val date = Instant.ofEpochMilli(query.atMs).atZone(zoneId).toLocalDate()
                MobileZoneFactValue.Date(date.format(DateTimeFormatter.ISO_LOCAL_DATE))
            }
            is MobileZoneQuery.Midnight -> {
                val localDate = try {
                    LocalDate.parse(query.date)
                } catch (malformed: Exception) {
                    return@mapNotNull null
                }
                MobileZoneFactValue.Instant(localDate.atStartOfDay(zoneId).toInstant().toEpochMilli())
            }
        }
        MobileZoneFact(key = mobileZoneQueryKey(query), value = value)
    }

    private fun zoneOf(query: MobileZoneQuery): String = when (query) {
        is MobileZoneQuery.CivilDate -> query.zone
        is MobileZoneQuery.Midnight -> query.zone
    }
}
