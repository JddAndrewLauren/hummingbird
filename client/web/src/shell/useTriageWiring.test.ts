import { describe, expect, it } from "vitest";
import { mintTriageSeed } from "./useTriageWiring";

// #223: pins the deterministic half of the sync module's seed-minting rule
// (client/core/src/sync/mod.rs) for `Core::triage` — triaging touches an
// item that already exists, so a retry of the identical intent must
// reproduce the identical seed (and therefore, per `deterministic_id`'s own
// frozen "same seed always derives the same id" test in
// `client/core/src/sync/write/id.rs`, the identical id).
describe("mintTriageSeed", () => {
  it("retrying the same triage intent (same item, destination, nowMs) mints the same seed", () => {
    const first = mintTriageSeed("item-1", "ready", 5_000);
    const second = mintTriageSeed("item-1", "ready", 5_000);

    expect(first).toEqual(second);
  });

  it("a null destination (#122's pure field edit) mints its own stable seed, distinct from every real destination", () => {
    const first = mintTriageSeed("item-1", null, 5_000);
    const second = mintTriageSeed("item-1", null, 5_000);
    const ready = mintTriageSeed("item-1", "ready", 5_000);

    expect(first).toEqual(second);
    expect(first).not.toEqual(ready);
  });
});
