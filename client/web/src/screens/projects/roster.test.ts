import { describe, expect, it } from "vitest";
import type { LedgerRowDTO, ProjectDTO, TaskItemDTO } from "../../store/protocol";
import type { TaskProjectResult } from "../../store/store";
import {
  awaitingCreate,
  countsMeta,
  githubRepoUrl,
  projectRoster,
  rosterSummary,
  visibleRows,
  writeFailureMessage,
} from "./roster";

function project(id: string, name: string, archived = false): ProjectDTO {
  return {
    id,
    name,
    githubRepo: null,
    defaultContext: null,
    archivedAt: archived ? 1_000 : null,
    createdAt: 1,
    updatedAt: 1,
    version: 1,
  };
}

function row(id: string, projectId: string | null, stage: TaskItemDTO["stage"]): LedgerRowDTO {
  return {
    id,
    seq: 1,
    title: id,
    description: null,
    stage,
    size: null,
    energy: null,
    context: null,
    priority: 0,
    projectId,
    projectPos: null,
    deadline: null,
    scheduledDate: null,
    source: null,
    sourceKey: null,
    sourceUrl: null,
    archivedAt: null,
    createdAt: 1,
    updatedAt: 1,
    version: 1,
    pending: false,
    absentSinceMs: null,
    deadLettered: false,
    hasLiveAlert: false,
  };
}

describe("projectRoster", () => {
  const projects = [project("p-1", "House repairs"), project("p-9", "Old bike", true), project("p-2", "Garden")];

  it("puts live projects before archived ones, keeping each half's given order", () => {
    const rows = projectRoster(projects, []);
    expect(rows.map((r) => r.project.id)).toEqual(["p-1", "p-2", "p-9"]);
    expect(rows.map((r) => r.archived)).toEqual([false, false, true]);
  });

  it("tallies actions from the ledger, splitting done out and ignoring unassigned rows", () => {
    const ledger = [
      row("i-1", "p-1", "ready"),
      row("i-2", "p-1", "triage"),
      row("i-3", "p-1", "done"),
      row("i-4", null, "ready"),
      row("i-5", "p-unknown", "ready"),
    ];
    const rows = projectRoster(projects, ledger);
    expect(rows.find((r) => r.project.id === "p-1")?.counts).toEqual({ live: 2, done: 1 });
    expect(rows.find((r) => r.project.id === "p-2")?.counts).toEqual({ live: 0, done: 0 });
  });

  // The module's whole reason for a nullable count: `0 actions` is a claim,
  // and a device whose Ledger has not answered has no standing to make it.
  it("leaves counts null — not zero — while the ledger has not answered", () => {
    const rows = projectRoster(projects, null);
    expect(rows.every((r) => r.counts === null)).toBe(true);
    expect(countsMeta(null)).toBe("actions not read yet");
  });
});

describe("countsMeta", () => {
  it("singularises one action and drops a zero done count", () => {
    expect(countsMeta({ live: 1, done: 0 })).toBe("1 action");
    expect(countsMeta({ live: 0, done: 0 })).toBe("0 actions");
    expect(countsMeta({ live: 3, done: 2 })).toBe("3 actions · 2 done");
  });
});

describe("rosterSummary", () => {
  it("counts the live and archived halves", () => {
    const rows = projectRoster([project("p-1", "A"), project("p-2", "B", true)], []);
    expect(rosterSummary(rows)).toBe("1 live · 1 archived");
  });
});

describe("visibleRows", () => {
  const rows = projectRoster([project("p-1", "A"), project("p-2", "B", true)], []);

  it("hides archived rows until the toggle, and never drops a live one", () => {
    expect(visibleRows(rows, false).map((r) => r.project.id)).toEqual(["p-1"]);
    expect(visibleRows(rows, true).map((r) => r.project.id)).toEqual(["p-1", "p-2"]);
  });
});

function write(kind: TaskProjectResult["kind"], overrides: Partial<TaskProjectResult> = {}) {
  return { seed: "s-1", projectId: null, kind, error: null, ...overrides };
}

describe("awaitingCreate", () => {
  const empty = projectRoster([], []);

  it("waits on a minted id the grid has not seen, and stops the moment it lands", () => {
    const ok = write("ok", { projectId: "p-new" });
    expect(awaitingCreate(empty, ok)).toBe(true);
    expect(awaitingCreate(projectRoster([project("p-new", "Deck")], []), ok)).toBe(false);
  });

  it("waits on nothing before the first write, or after one that did not go through", () => {
    expect(awaitingCreate(empty, null)).toBe(false);
    expect(awaitingCreate(empty, write("failed", { error: "boom" }))).toBe(false);
    expect(awaitingCreate(empty, write("busy"))).toBe(false);
  });
});

describe("writeFailureMessage", () => {
  it("says nothing about a write that went through", () => {
    expect(writeFailureMessage(null)).toBeNull();
    expect(writeFailureMessage(write("ok", { projectId: "p-new" }))).toBeNull();
  });

  it("passes a failure's own message through", () => {
    expect(writeFailureMessage(write("failed", { error: "name must be non-empty" }))).toBe(
      "name must be non-empty",
    );
  });

  // A dropped write is still a write that did not happen, and `busy` carries
  // no message of its own — so the fallback copy is the only thing standing
  // between the operator and silence.
  it("states a busy drop in the fallback copy rather than staying silent", () => {
    expect(writeFailureMessage(write("busy"))).toBe("That project write did not go through.");
    expect(writeFailureMessage(write("failed"))).toBe("That project write did not go through.");
  });
});

describe("githubRepoUrl", () => {
  it("derives the display link from owner/repo, never storing it", () => {
    expect(githubRepoUrl("JddAndrewLauren/hummingbird")).toBe(
      "https://github.com/JddAndrewLauren/hummingbird",
    );
  });

  it("is null when the project names no repo", () => {
    expect(githubRepoUrl(null)).toBeNull();
    expect(githubRepoUrl("")).toBeNull();
  });
});
