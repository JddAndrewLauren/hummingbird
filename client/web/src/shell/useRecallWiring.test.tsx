// @vitest-environment jsdom

// Recall's read-side wiring, pinned at hook level: which renders ask
// `Core::search` again, and which renders throw the previous answer away.
//
// Mounted rather than called, for `useLedgerWiring.test.tsx`'s own reason: a
// hook that is exported, unit-tested and never wired compiles clean and does
// nothing.
//
// The hook writes the store singleton (its own doc says why the clear lives
// there rather than in `App.tsx`), so the slot is reset before each case and
// read back through `coreStore.getSnapshot()` — there is no return value to
// assert against.

import { beforeEach, describe, expect, it, vi } from "vitest";
import { coreStore, type CoreStatus, type TaskState } from "../store/store";
import type { WorkerLike } from "../store/worker-client";
import { render } from "../test/component";
import { useRecallWiring } from "./useRecallWiring";

function fakeWorker(): WorkerLike & { postMessage: ReturnType<typeof vi.fn> } {
  return { onmessage: null, postMessage: vi.fn() };
}

function Probe({
  worker,
  status,
  query,
  lastTriage = null,
}: {
  worker: WorkerLike;
  status: CoreStatus;
  query: string;
  lastTriage?: TaskState["lastTriage"];
}) {
  useRecallWiring(worker, status, query, lastTriage);
  return null;
}

function searches(worker: ReturnType<typeof fakeWorker>): unknown[] {
  return worker.postMessage.mock.calls.map(([message]) => message);
}

function seedAnswer(): void {
  coreStore.setTaskState({ search: { rows: [], total: 3 } });
}

describe("useRecallWiring", () => {
  beforeEach(() => coreStore.setTaskState({ search: null }));

  it("asks nothing while the core is still loading", () => {
    const worker = fakeWorker();
    render(<Probe worker={worker} status="loading" query="backup" />);
    expect(worker.postMessage).not.toHaveBeenCalled();
  });

  it("asks nothing for an empty or whitespace-only query — the rule this side owns", () => {
    const worker = fakeWorker();
    const view = render(<Probe worker={worker} status="ready" query="" />);
    view.rerender(<Probe worker={worker} status="ready" query="   " />);
    expect(worker.postMessage).not.toHaveBeenCalled();
  });

  it("requests the search (with a nowMs) once the core is ready", () => {
    const worker = fakeWorker();
    render(<Probe worker={worker} status="ready" query="backup" />);
    expect(searches(worker)).toHaveLength(1);
    const [message] = worker.postMessage.mock.calls[0] as [
      { type: string; query: string; nowMs: number },
    ];
    expect(message.type).toBe("search");
    expect(message.query).toBe("backup");
    expect(typeof message.nowMs).toBe("number");
  });

  it("re-asks when the query changes", () => {
    const worker = fakeWorker();
    const view = render(<Probe worker={worker} status="ready" query="backup" />);
    view.rerender(<Probe worker={worker} status="ready" query="backups" />);
    expect(searches(worker)).toHaveLength(2);
    expect((searches(worker)[1] as { query: string }).query).toBe("backups");
  });

  it("re-asks when a triage result lands, and does NOT clear the slot on that path", () => {
    // #498's refresh of a row that is still on screen and being edited. The
    // regression guard for the clear effect's keying: clearing here would
    // flash "Searching…" over the open panel.
    const worker = fakeWorker();
    const view = render(<Probe worker={worker} status="ready" query="backup" />);
    seedAnswer();
    view.rerender(
      <Probe
        worker={worker}
        status="ready"
        query="backup"
        lastTriage={{ seed: "s-1", itemId: "a-1", kind: "ok", error: null }}
      />,
    );
    expect(searches(worker)).toHaveLength(2);
    expect(coreStore.getSnapshot().task.search).not.toBeNull();
  });

  it("does not re-ask on a render that changed nothing", () => {
    const worker = fakeWorker();
    const view = render(<Probe worker={worker} status="ready" query="backup" />);
    view.rerender(<Probe worker={worker} status="ready" query="backup" />);
    expect(searches(worker)).toHaveLength(1);
  });

  it("throws the previous answer away the instant the query changes", () => {
    const worker = fakeWorker();
    const view = render(<Probe worker={worker} status="ready" query="backup" />);
    seedAnswer();
    view.rerender(<Probe worker={worker} status="ready" query="warranty" />);
    expect(coreStore.getSnapshot().task.search).toBeNull();
  });
});
