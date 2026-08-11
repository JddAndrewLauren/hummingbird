import { describe, expect, it, vi } from "vitest";
import type { WorkerResponse } from "../store/protocol";
import { announceReady } from "./announce";

// Regression guard for the PR #79 round-2 blocker: the worker must announce
// readiness itself (push), unconditionally at module evaluation — never wait
// for a request from the main thread, which can be dropped while the async
// module wrapper is still awaiting the wasm import.

const IDENTITY = { coreId: "3f2a1b8c", viewOrdinal: 2 };

describe("announceReady", () => {
  it("posts ready with the core's api version, unprompted", () => {
    const posted: WorkerResponse[] = [];

    announceReady((m) => posted.push(m), () => 1, IDENTITY);

    expect(posted).toEqual([
      { type: "ready", apiVersion: 1, coreId: "3f2a1b8c", viewOrdinal: 2 },
    ]);
  });

  // #172: the handshake is the carrier for ADR-0010's probe precisely
  // because `PortRegistry` posts one PER connecting port — so a PWA window
  // joining an already-running core gets its own, with no new request
  // direction. Both fields are required, so a caller cannot drop them
  // silently.
  it("carries the core instance id and this view's ordinal", () => {
    const post = vi.fn();

    announceReady(post, () => 4, { coreId: "deadbeef", viewOrdinal: 7 });

    expect(post).toHaveBeenCalledWith({
      type: "ready",
      apiVersion: 4,
      coreId: "deadbeef",
      viewOrdinal: 7,
    });
  });

  it("posts error with the message when the core call throws", () => {
    const posted: WorkerResponse[] = [];

    announceReady(
      (m) => posted.push(m),
      () => {
        throw new Error("wasm core exploded");
      },
      IDENTITY,
    );

    expect(posted).toEqual([{ type: "error", message: "wasm core exploded" }]);
  });

  it("stringifies non-Error throws", () => {
    const post = vi.fn();

    announceReady(
      post,
      () => {
        throw "boom";
      },
      IDENTITY,
    );

    expect(post).toHaveBeenCalledWith({ type: "error", message: "boom" });
  });
});
