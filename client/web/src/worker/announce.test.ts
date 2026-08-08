import { describe, expect, it, vi } from "vitest";
import type { WorkerResponse } from "../store/protocol";
import { announceReady } from "./announce";

// Regression guard for the PR #79 round-2 blocker: the worker must announce
// readiness itself (push), unconditionally at module evaluation — never wait
// for a request from the main thread, which can be dropped while the async
// module wrapper is still awaiting the wasm import.

describe("announceReady", () => {
  it("posts ready with the core's api version, unprompted", () => {
    const posted: WorkerResponse[] = [];

    announceReady((m) => posted.push(m), () => 1);

    expect(posted).toEqual([{ type: "ready", apiVersion: 1 }]);
  });

  it("posts error with the message when the core call throws", () => {
    const posted: WorkerResponse[] = [];

    announceReady(
      (m) => posted.push(m),
      () => {
        throw new Error("wasm core exploded");
      },
    );

    expect(posted).toEqual([{ type: "error", message: "wasm core exploded" }]);
  });

  it("stringifies non-Error throws", () => {
    const post = vi.fn();

    announceReady(post, () => {
      throw "boom";
    });

    expect(post).toHaveBeenCalledWith({ type: "error", message: "boom" });
  });
});
