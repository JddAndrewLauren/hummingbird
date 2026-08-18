// Backtest (ADR-0011): "re-fetch recent history from the source and show
// which events a draft rule *would have* promoted... needs no
// persistence... the match count is shown before save." No longer
// implemented here: it is `hummingbird_core::decisions::rules::backtest`
// (ADR-0025, #141/M4, #540) — the other of the two drifts ADR-0025's M1
// verdict table recorded as known debt (`rules/backtest.ts:52`), where the
// day-only → `T23:59` resolution and the duration arithmetic were
// re-derived in TS beside `hummingbird_domain::deadline`'s.
//
// **This module's old header claimed `hummingbird-rules-engine` is "a
// native-only crate this build has no wasm path to." That was never
// true** — the crate depends on `hummingbird-domain` and `serde_json` and
// nothing else, and `ffi-web` already compiles `hummingbird-domain` to
// wasm. The core module now compiles it, and gates every operator through
// that crate's own `Operator`. (Stated rather than annotated: a negative
// claim left in place as a footnote is how the next reader inherits it.)
//
// The evaluation is still pure, still client-side, still never a server
// call and never a write, and still restricted to the one kind this client
// holds raw material for: `item_threshold`. A rule targeting any other
// kind reports `"unavailable"` rather than silently answering zero matches
// for a different reason; `eventKind === null` ("any kind") is still
// evaluated, since `item_threshold_event` synthesizes it like every other
// source.
//
// **Two honest gaps against `authority::sweep::item_threshold_event`,
// neither hidden by the count this reports** — both carried into the Rust
// module's header verbatim, since that half of this one was accurate:
//
// 1. **The corpus.** `sweep_tick` evaluates every non-archived item
//    (`load_live_items`); this backtest only ever sees whatever `items`
//    the caller passes it, which today is `task.frontier` —
//    `Ready`/`InProgress`, unarchived, *and* unblocked (`Core::frontier`).
//    Triage-stage and blocked items are outside the count. The UI copy
//    names this explicitly rather than presenting a bare "N matches" a
//    reader could mistake for the sweep's own answer.
// 2. **The synthesized item cannot carry every core field with full
//    fidelity.** `occurred_at` is derived from `updatedAt` the same way
//    the server derives it from `item.updated_at` (`now_as_deadline`), and
//    `calendar_busy` is always `false` because the server's own synthesis
//    always writes `None` for it, so both are exact rather than
//    approximate. Every other field offered is a direct 1:1 copy of what
//    `item_threshold_event` sets.
//
// **One deliberate change the sink made:** string comparison is now
// case-insensitive, ADR-0013's own rule and what the sweep actually does
// at fire time; this module used `===`/`String.includes`. A backtest that
// disagrees with the sweep about `stage eq "Ready"` reports a count the
// sweep would not produce, which is the one thing this panel must not do.
//
// This module is kept as the import site rather than deleted so the sink
// stayed a rewire: `RulesScreen.tsx` and `backtest.test.ts` are untouched.

export {
  backtest,
  type BacktestResult,
  type BacktestUnavailableReason,
} from "../../decisions/seam";
