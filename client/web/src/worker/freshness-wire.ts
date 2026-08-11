import type { FreshnessDTO } from "../store/protocol";

// One wire mapping for `hummingbird_core::freshness::Freshness`'s serde
// output, shared by both wasm hosts' workers (`task-worker.ts`'s pane reads,
// `calendar-worker.ts`'s calendar-events reads) — they read the exact same
// two-shape enum off two different Rust structs, and a verbatim second copy
// is drift material the moment either serialization changes underneath one
// caller and not the other.

export type RawFreshness =
  | { state: "unknown" }
  | { state: "age"; age_ms: number; declared_cadence_ms: number | null };

export function mapFreshness(raw: RawFreshness): FreshnessDTO {
  return raw.state === "unknown"
    ? { kind: "unknown" }
    : { kind: "age", ageMs: raw.age_ms, declaredCadenceMs: raw.declared_cadence_ms };
}
