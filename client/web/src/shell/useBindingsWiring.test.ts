import { describe, expect, it } from "vitest";
import { mintBindingSeed, mintQuestionSwitchSeed } from "./useBindingsWiring";

// #223: pins the deterministic half of the sync module's seed-minting rule
// (client/core/src/sync/mod.rs) for `Core::set_binding` — a binding write
// touches the `settings` row `key` itself names, so the seed's hash becomes
// only the mutation's local queue-entry id and a retry of the identical
// intent must reproduce the identical seed (and therefore the identical
// entry, never a second one).
describe("mintBindingSeed", () => {
  it("retrying the same binding write (same key, nowMs) mints the same seed", () => {
    const first = mintBindingSeed("city-waste-page", 5_000);
    const second = mintBindingSeed("city-waste-page", 5_000);

    expect(first).toEqual(second);
  });

  it("a different key mints a different seed", () => {
    const a = mintBindingSeed("city-waste-page", 5_000);
    const b = mintBindingSeed("race-series", 5_000);

    expect(a).not.toEqual(b);
  });
});

describe("mintQuestionSwitchSeed", () => {
  it("retrying the same toggle (same question, nowMs) mints the same seed", () => {
    // #715, on `mintBindingSeed`'s own rule: the `settings` row a toggle
    // writes is identified by the question, so the seed's hash is only the
    // queue-entry id and a retry of the identical intent must reproduce the
    // identical entry rather than enqueue a second one.
    expect(mintQuestionSwitchSeed("weekend", 5_000)).toEqual(
      mintQuestionSwitchSeed("weekend", 5_000),
    );
  });

  it("a different question mints a different seed", () => {
    expect(mintQuestionSwitchSeed("weekend", 5_000)).not.toEqual(
      mintQuestionSwitchSeed("race", 5_000),
    );
  });

  it("never collides with a binding write's seed for a same-named key", () => {
    // The two vocabularies share one table and one queue. A binding key and
    // a question that happened to be spelled alike would otherwise mint the
    // same queue-entry id at the same instant, and one write would silently
    // stand in for the other.
    expect(mintQuestionSwitchSeed("race", 5_000)).not.toEqual(mintBindingSeed("race", 5_000));
  });
});
