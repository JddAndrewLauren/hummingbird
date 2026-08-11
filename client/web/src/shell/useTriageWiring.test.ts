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

  it("a different destination mints a different seed", () => {
    const a = mintTriageSeed("item-1", "ready", 5_000);
    const b = mintTriageSeed("item-1", "grilling", 5_000);

    expect(a).not.toEqual(b);
  });
});
