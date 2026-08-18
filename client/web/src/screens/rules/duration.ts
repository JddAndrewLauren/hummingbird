// The `within_next`/`within_last` value grammar (ADR-0013) — a bare
// positive integer plus a `m`/`h`/`d` suffix. No longer implemented here:
// it is `hummingbird_core::decisions::rules::duration` (ADR-0025, #141/M4,
// #540), which parses through `hummingbird_domain::parse_duration` rather
// than the second regex and second unit → milliseconds table this module
// used to carry beside it.
//
// The alarm-interval measurement (#138) came with it, unchanged in
// meaning: **warn, never reject** — a malformed duration is #133's
// save-time rejection to catch, server-side, not this one's.
//
// This module is kept as the import site rather than deleted so the sink
// stayed a rewire: every caller and `duration.test.ts` are untouched.

export {
  durationUnitsFor,
  formatDuration,
  isBelowAlarmInterval,
  parseDurationMs,
  type DurationUnit,
} from "../../decisions/seam";
