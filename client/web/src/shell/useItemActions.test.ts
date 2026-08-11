import { describe, expect, it } from "vitest";
import { mintActSeed } from "./useItemActions";

// #223: pins the deterministic half of the sync module's seed-minting rule
// (client/core/src/sync/mod.rs) for `Core::act` — acting touches an item
// that already exists, so a retry of the identical intent must reproduce
// the identical seed (and therefore, per `deterministic_id`'s own frozen
// "same seed always derives the same id" test in
// `client/core/src/sync/write/id.rs`, the identical id).
describe("mintActSeed", () => {
  it("retrying the same act intent (same item, action, nowMs) mints the same seed", () => {
    const first = mintActSeed("item-1", "complete", 5_000);
    const second = mintActSeed("item-1", "complete", 5_000);

    expect(first).toEqual(second);
  });

  it("a different item mints a different seed", () => {
    const a = mintActSeed("item-1", "complete", 5_000);
    const b = mintActSeed("item-2", "complete", 5_000);

    expect(a).not.toEqual(b);
  });

  it("a different action mints a different seed", () => {
    const a = mintActSeed("item-1", "complete", 5_000);
    const b = mintActSeed("item-1", "start", 5_000);

    expect(a).not.toEqual(b);
  });
});
