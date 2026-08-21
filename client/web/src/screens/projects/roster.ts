// #624's Projects grid display shape: which cards there are, in what order,
// and what each one's counts line says. A pure module (no React, no worker),
// the same split `triage-form.ts`/`frontier-facets.ts` use for their own
// screen logic, so the rules below are unit-testable without mounting
// anything.
//
// **The counts are derived, not read.** This slice adds no read beyond the
// `projects` one that has existed since #108: an action's `projectId` is
// already on every Ledger row (`LedgerRowDTO extends TaskItemDTO`), and the
// Ledger is already app-wide state. So a card's "3 actions · 1 done" is a
// tally over rows this device already holds, not a per-project query.
//
// That has one honest consequence, and `ProjectCounts` is `null` rather than
// zeroed to carry it: until the Ledger has answered, this module cannot tell
// a project with no actions from a project whose actions have not been read.
// `0 actions` is a claim, and nothing here is entitled to make it early.

import type { LedgerRowDTO, ProjectDTO } from "../../store/protocol";
import type { TaskProjectResult } from "../../store/store";

/** One project's tallied actions. `done` counts rows at the `done` stage;
 * `live` is everything else this device holds for the project, archived rows
 * included — the Ledger is the retained roster, so an archived action still
 * belongs to its project. */
export interface ProjectCounts {
  live: number;
  done: number;
}

/** One card in the grid. */
export interface ProjectRow {
  project: ProjectDTO;
  archived: boolean;
  /** `null` while the Ledger has not answered — see the module header. */
  counts: ProjectCounts | null;
}

/** The grid's rows, live first and archived after, each half in the order
 * `TaskState.projects` already arrives in (`Core::projects`' id order, which
 * is stable and diffable). Archived rows are always built — the screen's
 * Show-archived toggle filters `rows`, so the count line can name how many
 * are hidden without a second pass. */
export function projectRoster(
  projects: ProjectDTO[],
  ledger: LedgerRowDTO[] | null,
): ProjectRow[] {
  const counts = ledger === null ? null : tallyByProject(ledger);
  const rows = projects.map((project) => ({
    project,
    archived: project.archivedAt !== null,
    counts: counts === null ? null : (counts.get(project.id) ?? { live: 0, done: 0 }),
  }));
  return [...rows.filter((row) => !row.archived), ...rows.filter((row) => row.archived)];
}

function tallyByProject(ledger: LedgerRowDTO[]): Map<string, ProjectCounts> {
  const counts = new Map<string, ProjectCounts>();
  for (const row of ledger) {
    if (row.projectId === null) {
      continue;
    }
    const current = counts.get(row.projectId) ?? { live: 0, done: 0 };
    if (row.stage === "done") {
      current.done += 1;
    } else {
      current.live += 1;
    }
    counts.set(row.projectId, current);
  }
  return counts;
}

/** A card's meta line. Renders the not-read-yet state as a word rather than
 * as `0 actions` — the module header's whole point. Voice per the design
 * brief: bare counts, middle dot for the join. */
export function countsMeta(counts: ProjectCounts | null): string {
  if (counts === null) {
    return "actions not read yet";
  }
  const actions = `${counts.live} ${counts.live === 1 ? "action" : "actions"}`;
  return counts.done === 0 ? actions : `${actions} · ${counts.done} done`;
}

/** The grid's header line: how many projects are live, and how many archived
 * ones the toggle is hiding. */
export function rosterSummary(rows: ProjectRow[]): string {
  const archived = rows.filter((row) => row.archived).length;
  return `${rows.length - archived} live · ${archived} archived`;
}

/** The cards the Show-archived toggle actually leaves on screen. */
export function visibleRows(rows: ProjectRow[], showArchived: boolean): ProjectRow[] {
  return rows.filter((row) => showArchived || !row.archived);
}

/** True while a create is still in flight — its minted id has not reached the
 * grid. `lastProjectWrite` is the only handle on it (there is no optimistic
 * overlay to look in, per `Core::create_project`), and this clears itself the
 * moment the cycle that pulls the project back lands. */
export function awaitingCreate(
  rows: ProjectRow[],
  lastProjectWrite: TaskProjectResult | null,
): boolean {
  if (lastProjectWrite === null || lastProjectWrite.kind !== "ok") {
    return false;
  }
  const { projectId } = lastProjectWrite;
  return projectId !== null && !rows.some((row) => row.project.id === projectId);
}

/** The minimal shape every project-lane write result shares —
 * `TaskProjectResult`, `TaskProjectLinkResult` and `TaskRouteResult` alike
 * (`store.ts`'s own docs) — everything this pure function needs. */
export interface ProjectLaneWriteResult {
  seed: string;
  kind: "ok" | "failed" | "busy";
  error: string | null;
}

/** What a non-`ok` write says, or `null` when there is nothing to say. Both
 * non-`ok` kinds land here on purpose: a `busy` result is a write the worker
 * dropped rather than one it delivered, so it is as much a "this did not
 * happen" as a `failed` is, and silence would be the one answer this screen
 * must never give. `busy` carries no `error` of its own, which is what
 * `fallback` is for.
 *
 * **`issuedSeed` is what makes this seed-keyed** (batch review,
 * projects-dossier #668, generalised for #669). `lastWrite` is one broadcast
 * slot shared by every reader of the same project-lane write —
 * `PropertiesCard` and `ArchiveCard` both patch the same `project.id`, and
 * `LinksCard`/`RouteCard` share their own slot across every open dossier
 * (`TaskProjectResult`'s own doc) — so a reader gating on the target id
 * alone can resolve on a write a SIBLING reader issued rather than its own.
 * Passing the reader's own `issuedSeed` (the seed its own write minted,
 * `null` whenever it holds no write outstanding) is what tells them apart:
 * the message renders only once `lastWrite.seed` matches it.
 *
 * `issuedSeed` is optional for the one caller with no per-write seed to
 * scope to: the grid's create banner has no single in-flight write of its
 * own (a create's minted seed is dropped — `useProjectsWiring`'s own doc),
 * so omitting the argument keeps its pre-existing, ungated read — the grid
 * still names ANY project write's failure, same as it always has. Every
 * other caller passes its own `issuedSeed` and gets the scoped read. */
export function writeFailureMessage(
  lastWrite: ProjectLaneWriteResult | null,
  issuedSeed?: string | null,
  fallback = "That project write did not go through.",
): string | null {
  if (lastWrite === null || lastWrite.kind === "ok") {
    return null;
  }
  if (issuedSeed !== undefined && lastWrite.seed !== issuedSeed) {
    return null;
  }
  return lastWrite.error ?? fallback;
}

/** How many of a project's items are currently live (#630, ADR-0030
 * decision 5) — the number the archive dialog names before the human
 * commits. `null` while the Ledger has not answered, same "cannot claim a
 * count early" doctrine [`countsMeta`]'s own doc states; the dialog must
 * render that as prose, not `0`.
 *
 * Deliberately **not** `ProjectCounts.live`: that tally excludes `done`
 * rows but keeps archived ones (its own doc says so — "archived rows
 * included"), because it counts *actions the project has ever had*. The
 * archive cascade's scope is different: every item this project still
 * holds with no `archivedAt` of its own, `done` included, since a done-but-
 * live item still archives with the rest
 * (`server/authority/src/handlers/items.rs`'s `cascade_archive_for_project`
 * is the source of truth this mirrors — `project_id = ? AND archived_at IS
 * NULL`, no stage filter). */
export function liveItemCount(ledger: LedgerRowDTO[] | null, projectId: string): number | null {
  if (ledger === null) {
    return null;
  }
  return ledger.filter((row) => row.projectId === projectId && row.archivedAt === null).length;
}

/** The derived display link for a project's `githubRepo` (#625, ADR-0030
 * decision 2) — computed here, never stored: `github_repo` holds only the
 * `owner/repo` slug, so there is one spelling to compare and no half-typed
 * link to normalize. `null` when the project names no repo, so a caller
 * never has to re-check for an empty string. */
export function githubRepoUrl(githubRepo: string | null): string | null {
  return githubRepo === null || githubRepo === "" ? null : `https://github.com/${githubRepo}`;
}
