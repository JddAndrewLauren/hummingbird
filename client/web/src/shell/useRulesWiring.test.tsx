// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";
import type { CoreStatus } from "../store/store";
import type { RuleDTO } from "../store/protocol";
import { renderHook } from "../test/component";
import type { WorkerLike } from "../store/worker-client";
import { useRulesWiring } from "./useRulesWiring";

// Mounted (via renderHook) rather than called, for
// `useLedgerWiring.test.tsx`'s own reason: a hook that is exported,
// unit-tested and never wired compiles clean and does nothing.

function fakeWorker(): WorkerLike & { postMessage: ReturnType<typeof vi.fn> } {
  return { onmessage: null, postMessage: vi.fn() };
}

function mount(worker: WorkerLike, status: CoreStatus, syncOutcomeSeq: number) {
  return renderHook(
    (props: { status: CoreStatus; syncOutcomeSeq: number }) =>
      useRulesWiring(worker, props.status, props.syncOutcomeSeq),
    { initialProps: { status, syncOutcomeSeq } },
  );
}

function types(worker: ReturnType<typeof fakeWorker>): string[] {
  return worker.postMessage.mock.calls.map(([message]) => (message as { type: string }).type);
}

const rule: RuleDTO = {
  id: "rule-1",
  name: "A rule",
  eventKind: null,
  conditions: [],
  severity: "info",
  tier: "normal",
  enabled: true,
  updatedAt: 1_000,
  version: 1,
  deletedAt: null,
};

describe("useRulesWiring", () => {
  it("asks nothing while the core is still loading", () => {
    const worker = fakeWorker();
    mount(worker, "loading", 0);
    expect(worker.postMessage).not.toHaveBeenCalled();
  });

  it("requests the kind registry and the rules once the core is ready", () => {
    const worker = fakeWorker();
    mount(worker, "ready", 0);
    expect(types(worker)).toEqual(["getKindRegistry", "getRules"]);
  });

  it("re-requests the rules but not the kind registry on a completed cycle", () => {
    const worker = fakeWorker();
    const view = mount(worker, "ready", 0);
    view.rerender({ status: "ready", syncOutcomeSeq: 1 });
    expect(types(worker)).toEqual(["getKindRegistry", "getRules", "getRules"]);
  });

  it("does not re-request on a render that changed nothing", () => {
    const worker = fakeWorker();
    const view = mount(worker, "ready", 2);
    view.rerender({ status: "ready", syncOutcomeSeq: 2 });
    expect(types(worker)).toEqual(["getKindRegistry", "getRules"]);
  });

  it("createRule posts a createRule message with a minted seed", () => {
    const worker = fakeWorker();
    const { result } = mount(worker, "ready", 0);
    worker.postMessage.mockClear();

    result.current.createRule("A new rule", "capture", [], "warn", "normal", true);

    expect(worker.postMessage).toHaveBeenCalledTimes(1);
    const [message] = worker.postMessage.mock.calls[0] as [Record<string, unknown>];
    expect(message.type).toBe("createRule");
    expect(message.name).toBe("A new rule");
    expect(message.eventKind).toBe("capture");
    expect(message.severity).toBe("warn");
    expect(message.tier).toBe("normal");
    expect(message.enabled).toBe(true);
    expect(typeof message.seed).toBe("string");
    expect(typeof message.nowMs).toBe("number");
  });

  it("patchRule posts a patchRule message carrying the current row and a deterministic seed", () => {
    const worker = fakeWorker();
    const { result } = mount(worker, "ready", 0);
    worker.postMessage.mockClear();

    result.current.patchRule(rule, { enabled: false });

    expect(worker.postMessage).toHaveBeenCalledTimes(1);
    const [message] = worker.postMessage.mock.calls[0] as [Record<string, unknown>];
    expect(message.type).toBe("patchRule");
    expect(message.current).toEqual(rule);
    expect(message.enabled).toBe(false);
    // Untouched fields stay `null` rather than carrying a stale value.
    expect(message.name).toBeNull();
    expect(message.deletedAtTouched).toBe(false);
  });

  it("patchRule marks deletedAt as touched when the patch sets it", () => {
    const worker = fakeWorker();
    const { result } = mount(worker, "ready", 0);
    worker.postMessage.mockClear();

    result.current.patchRule(rule, { deletedAt: 5_000 });

    const [message] = worker.postMessage.mock.calls[0] as [Record<string, unknown>];
    expect(message.deletedAtTouched).toBe(true);
    expect(message.deletedAt).toBe(5_000);
  });
});
