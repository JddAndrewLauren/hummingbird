// The one invariant #577's proactive rotation stands on, and the one nothing
// in either language can check on its own: **the authority must consider its
// cached token stale before this client wakes up to rotate it.**
//
// Two constants, two languages, one relationship. `ROTATION_MARGIN_MS` here
// says when the browser arms its timer; `CACHE_REMINT_MARGIN_MS` in
// `server/authority/src/google_calendar.rs` says when the Durable Object
// stops answering from cache. If the server's is not the larger, the timer
// fires, the route hands back the *same* token and the *same* `expires_at_ms`,
// the rotation effect in `shell/useCalendarWiring.ts` sees unchanged deps and
// never arms another timer — proactive rotation dies after its first cache
// hit, silently, and every session rediscovers expiry through a live 401.
// That is the exact defect this pin exists to stop coming back; it shipped
// once, found in review rather than by any test.
//
// Nothing types this. The Rust side has its own test that a token is stale at
// the moment the client wakes, but it hard-codes the client's margin — so the
// two tests together only agree if the constants do, which is what this file
// checks. Read as source text with `?raw`, the same technique
// `shell/responsive-breakpoint.test.ts` uses for the CSS breakpoint and
// `worker/sync-timer-ownership.test.ts` for its import graph.
import calendarSource from "../../../../server/authority/src/google_calendar.rs?raw";
import { describe, expect, it } from "vitest";
import { ROTATION_MARGIN_MS } from "./connection";

/** `pub const CACHE_REMINT_MARGIN_MS: i64 = 6 * 60 * 1000;` — a product of
 * integer literals, which is how both sides spell a duration. */
const RUST_CONSTANT = /pub const CACHE_REMINT_MARGIN_MS: i64 = ([\d_ *]+);/;

function serverMarginMs(): number {
  const match = calendarSource.match(RUST_CONSTANT);
  // Not an assertion about drift — an assertion that this file is still
  // reading something. A renamed constant would make every check below pass
  // vacuously, which is the failure mode a source-text pin is most prone to.
  expect(match, "CACHE_REMINT_MARGIN_MS not found — has it been renamed?").not.toBeNull();
  return match![1]
    .split("*")
    .map((factor) => Number(factor.replace(/_/g, "").trim()))
    .reduce((product, factor) => product * factor, 1);
}

describe("the calendar token's two re-mint margins", () => {
  it("is read from the Rust source, not guessed", () => {
    expect(serverMarginMs()).toBeGreaterThan(0);
    expect(Number.isFinite(serverMarginMs())).toBe(true);
  });

  it("has the authority giving up on its cache before the client wakes to rotate", () => {
    expect(serverMarginMs()).toBeGreaterThan(ROTATION_MARGIN_MS);
  });
});
