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

/** Resolve every query this runtime knows how to, and **omit** the rest. */
export function resolveZoneFacts(queries: readonly ZoneQuery[]): ZoneFacts {
  const facts: ZoneFacts = {};
  for (const query of queries) {
    const resolved =
      query.kind === "civilDate"
        ? civilDateInZone(query.atMs, query.zone)
        : zonedMidnightMs(query.date, query.zone);
    if (resolved !== null) {
      facts[query.key] = resolved;
    }
  }
  return facts;
}
