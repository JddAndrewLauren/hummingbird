import { describe, expect, it } from "vitest";
import { mintBindingSeed } from "./useBindingsWiring";

// #223: pins the deterministic half of the sync module's seed-minting rule
// (client/core/src/sync/mod.rs) for `Core::set_binding` — a binding write
// touches a `settings` row that either already exists or is created
// idempotently at `expected_version: 0`, so a retry of the identical intent
// must reproduce the identical seed.
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
