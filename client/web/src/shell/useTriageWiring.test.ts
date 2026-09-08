// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";
import { renderHook } from "../test/component";
import type { WorkerLike } from "../store/worker-client";
import { mintTriageSeed, useTriageWiring } from "./useTriageWiring";

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

/** The seed `triage` returns is what `useCaptureAttachments.ts` keys a
 * capture's note write on. `TaskState.lastTriage` is one broadcast slot
 * shared by every open view, so a returned seed that did not match the one
 * actually posted would leave that attachment waiting on a result it can
 * never recognise — silently, with the write itself landing fine. */
describe("useTriageWiring", () => {
  it("returns the seed it posted", () => {
    const worker: WorkerLike = { onmessage: null, postMessage: vi.fn() };
    const { result } = renderHook(() => useTriageWiring(worker));

    const seed = result.current.triage("item-1", null, { vaultPath: "Reading/Knee.md" });

    const posted = (worker.postMessage as ReturnType<typeof vi.fn>).mock.calls[0][0];
    expect(posted.type).toBe("triage");
    expect(seed).toBe(posted.seed);
    // And it is the deterministic shape #223 requires, not an opaque token.
    expect(seed).toContain("item-1:triage:none:");
  });
});
