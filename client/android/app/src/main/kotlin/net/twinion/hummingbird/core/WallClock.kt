package net.twinion.hummingbird.core

import java.time.Instant
import java.time.ZoneId
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter

/** The one place this app reads a timezone.
 *
 * Every decision in `hummingbird-core` takes "now" as an already-resolved
 * civil string in the deadline grammar's own shape (`YYYY-MM-DDTHH:MM`),
 * because that crate holds no timezone table — deliberately, and at length
 * (`client/core/Cargo.toml`; ADR-0015's "resolves no civil date to an
 * instant, the reader does, in its own zone"). Turning an epoch
 * millisecond count into that shape is therefore the host's job, and this
 * is the host's whole share of it. The web's equivalent is
 * `decisions/seam.ts`'s `localWallClock`/`utcWallClock`; these two must
 * mean the same things.
 *
 * **Two readings, not one.** The rules backtest reads one instant in two
 * frames: `deadline` and `scheduled_date` are device-local civil strings,
 * while `occurred_at` is stamped UTC by the authority
 * (`hummingbird_domain::now_as_deadline`'s own doc: "the Worker runs UTC.
 * Not per-rule, not per-device"). Naming both at the seam is what lets the
 * core compare each field in the frame it belongs to without ever
 * resolving a zone itself.
 *
 * Seconds are truncated, never rounded, in both — the deadline grammar is
 * minute-precision and `now_as_deadline` truncates.
 */
object WallClock {

    private val SHAPE: DateTimeFormatter = DateTimeFormatter.ofPattern("uuuu-MM-dd'T'HH:mm")

    /** This device's own wall-clock reading of `nowMs`. */
    fun local(nowMs: Long, zone: ZoneId = ZoneId.systemDefault()): String =
        Instant.ofEpochMilli(nowMs).atZone(zone).format(SHAPE)

    /** The same instant in UTC — the frame `occurred_at` is stamped in. */
    fun utc(nowMs: Long): String =
        Instant.ofEpochMilli(nowMs).atZone(ZoneOffset.UTC).format(SHAPE)
}
