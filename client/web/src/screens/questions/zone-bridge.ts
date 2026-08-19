import type { ZoneFacts, ZoneQuery } from "../../decisions/seam";
import { civilDateInZone, zonedMidnightMs } from "../waste-pane/zoned-day";

// The **web half of the zone bridge** (#533/M4, ADR-0025).
//
// A pane is civil-date reasoning — a bin collection happens on a day at an
// address, and "tonight" flips at the address's midnight, not the reader's
// — and `hummingbird-core` owns no tzdb, deliberately and at a measured
// price (`client/core/Cargo.toml`'s `chrono-tz` note: the table took the
// release wasm from 525 KB to 1.41 MB). So the crossing is two-phase: the
// core names every `(zone, civil-date)` fact it needs, this file resolves
// them, and the core decides.
//
// **This file contributes a lookup and no judgement.** It has no opinion
// about what an unusable zone means, what a resolved date implies, or which
// pane asked. It reads `ZoneQuery.key` — the core's own spelling, sent
// across rather than re-derived here, so the two sides cannot disagree
// about it — and writes one entry per query it could answer.
//
// **A zone this runtime cannot resolve is OMITTED, not nulled.** That is
// the whole protocol for a bad zone: `ZoneFacts::civil_date`/`midnight_ms`
// answer `None` for a missing key and the core turns that into
// `WasteGap::UnresolvableZone`. A `null` here, or an empty-string fallback,
// or a `known: false` flag would each be this side deciding a question that
// ADR-0025 puts in the core — and would be exactly the kind of quiet
// per-client divergence the sink exists to end. `zoned-day.ts` already
// answers `null` for an unknown zone, so the omission is one `if`.
//
// `zoned-day.ts` itself is unchanged by this slice: it stops being the
// waste pane's rule module and becomes the web's *resolver*, which is why
// its own suite still passes as-is.
//
// **`DEVICE_ZONE` (#534).** `weekend.rs`/`vacation.rs` ask about "the
// reader's own zone" rather than a payload-carried IANA name — there is no
// address to look up, only the device the reader is holding. Rather than a
// third `ZoneQuery` variant, the core names that zone with one sentinel
// string (`hummingbird_core::decisions::panes::zone::DEVICE_ZONE`,
// `"device-local"`); this file is the one place that string is given
// meaning, by resolving it to `Intl.DateTimeFormat().resolvedOptions()
// .timeZone` before calling the ordinary `Intl` resolver with it. No other
// module ever sees `"device-local"` as an argument.

/** `hummingbird_core::decisions::panes::zone::DEVICE_ZONE`, pinned against
 * `deviceZoneFromCore()` by `seam.test.ts` (see that file) rather than read
 * through the seam on every call: this constant is compared once, in a
 * loop-free `resolveZone` — a real seam round trip per query would be a
 * live dependency for zero benefit, since the sentinel's whole point is
 * that it never changes at runtime. Exported so the pinning test can
 * import the literal side of the comparison. */
export const DEVICE_ZONE = "device-local";

function resolveZone(zone: string): string {
  return zone === DEVICE_ZONE ? Intl.DateTimeFormat().resolvedOptions().timeZone : zone;
}

/** Resolve every query this runtime knows how to, and **omit** the rest. */
export function resolveZoneFacts(queries: readonly ZoneQuery[]): ZoneFacts {
  const facts: ZoneFacts = {};
  for (const query of queries) {
    const zone = resolveZone(query.zone);
    const resolved =
      query.kind === "civilDate"
        ? civilDateInZone(query.atMs, zone)
        : zonedMidnightMs(query.date, zone);
    if (resolved !== null) {
      facts[query.key] = resolved;
    }
  }
  return facts;
}
