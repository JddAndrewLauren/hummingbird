import { useEffect, useState } from "react";
import { Badge } from "../components/core/Badge";
import { Button } from "../components/core/Button";
import { Card } from "../components/core/Card";
import { IconButton } from "../components/core/IconButton";
import { EmptyState } from "../components/feedback/EmptyState";
import { Checkbox } from "../components/forms/Checkbox";
import { Input } from "../components/forms/Input";
import { Switch } from "../components/forms/Switch";
import { Textarea } from "../components/forms/Textarea";
import type {
  FogDTO,
  LedgerRowDTO,
  ProjectDTO,
  ProjectLinkDTO,
  RouteDTO,
  StepDTO,
  TaskItemDTO,
} from "../store/protocol";
import type {
  TaskActionReorderResult,
  TaskFogResult,
  TaskProjectLinkResult,
  TaskProjectResult,
  TaskRouteResult,
  TaskState,
  TaskStepResult,
} from "../store/store";
import { Aside, Column, TwoColumn } from "./layout";
import {
  awaitingCreate,
  countsMeta,
  githubRepoUrl,
  liveItemCount,
  projectRoster,
  rosterSummary,
  visibleRows,
  writeFailureMessage,
  type ProjectRow,
} from "./projects/roster";

// #624: the Projects screen — the project lane's first real client surface,
// replacing the `RoutesScreen` mock that took `DemoData` and never touched
// state. Two levels on one piece of local state: a card grid of every project
// this device holds, and a full-page dossier for one of them. That shape is
// the operator's prototyped verdict (#449's UX comment, variant B).
//
// **Everything here renders from `TaskState`.** There is no `demo` prop and
// no fixture path: under `?demo` (the kit world) the store is at rest, so the
// grid honestly photographs its holding state, exactly as Done and the Ledger
// do; under `?demo=board` the seeded fixture drives the real render path.
//
// **The create round-trips, deliberately.** `Core::create_project` mints no
// optimistic overlay (its own doc carries the argument, inherited verbatim
// from `Core::rules`), so a created project appears only when the next
// completed cycle pulls it back. This screen's job is therefore to *say it is
// waiting* rather than look like it dropped the input — `WAITING_COPY` below,
// shown from the moment the enqueue succeeds until the id turns up in
// `projects`.
//
// **The dossier has no placeholders left.** Properties (#625), links
// (#626), the Route's destination/notes (#627), the reading column's fog
// (#628), the ordered action list with each action's inline steps (#629)
// and now the archive affordance (#630, `ArchiveCard`) are all real cards
// — #630 was the last one `ComingRegion` stood in for, and that component
// is gone with it.
//
// **The action list's reorder overlays; its Steps don't.** Unlike every
// other project-lane write on this screen, reordering an Action patches
// the ordinary item table, which already carries `Core::act`'s overlay
// (`Core::patch_action_position`'s own doc) — `ActionsCard` shows the new
// order the instant a reorder is clicked, with no "waiting" state to paint.
// Steps stay in the no-overlay convention every other card here uses: a
// tick, a reword, an add or a delete becomes visible once the next
// completed cycle pulls it back, same as Fog and Links.
//
// **The Route card has no optimistic overlay, so it says so.** Same
// no-overlay contract as every other project-lane write here
// (`Core::patch_route`'s own doc) — a saved destination or notes edit is
// only confirmed once the next completed cycle pulls the row's bumped
// `version` back. `RouteCard` below tracks its own in-flight save
// explicitly (`saving`/`justSaved` local state) rather than staying silent
// about it the way the properties card does, because ADR-0030's brief for
// this slice asks for a visible saving/saved state where the properties
// card's own slice did not. A conflict is not surfaced here at all — it
// lands in the ordinary dead-letter journal like every other CAS write
// (ADR-0030 decision 1), and this card adds no bespoke conflict UI for it.

const WAITING_COPY = "creating — appears when the round trip lands";

export interface ProjectsScreenProps {
  task: TaskState;
  onCreateProject: (name: string) => void;
  /** #625: the dossier's properties card write, widened by #630's archive
   * card — `patch` carries only the fields the caller actually changed.
   * `archivedAt` drives ADR-0030 decision 5's server-side cascade onto the
   * project's live items; this prop itself enqueues the project's own
   * field change only. */
  onPatchProject: (
    current: ProjectDTO,
    patch: { githubRepo?: string | null; defaultContext?: string | null; archivedAt?: number | null },
  ) => void;
  /** #626: the links card's read — fetches one project's links. The caller
   * (this component's own effect) decides when: on open, and again on
   * every completed cycle it is still open for. */
  onRequestProjectLinks: (projectId: string) => void;
  /** #626: the links card's add gesture. */
  onCreateProjectLink: (projectId: string, url: string, label: string | null, position: number) => void;
  /** #626: the links card's edit/reorder/remove gesture — `patch` carries
   * only the fields the card actually changed. */
  onPatchProjectLink: (
    current: ProjectLinkDTO,
    patch: { url?: string; label?: string | null; position?: number; removedAt?: number | null },
  ) => void;
  /** #627: the Route card's read — fetches one project's Route. Same
   * "the caller's own effect decides when" shape as `onRequestProjectLinks`. */
  onRequestRoute: (projectId: string) => void;
  /** #627: the Route card's edit gesture — `patch` carries only the fields
   * the card actually changed. */
  onPatchRoute: (current: RouteDTO, patch: { destination?: string | null; notes?: string | null }) => void;
  /** #628: the fog card's read — fetches one project's open fog. Same
   * "the caller's own effect decides when" shape as `onRequestProjectLinks`. */
  onRequestFog: (projectId: string) => void;
  /** #628: the fog card's add-a-question gesture. */
  onCreateFog: (projectId: string, question: string, position: number) => void;
  /** #628: the fog card's reword/reposition/resolve gesture — `patch`
   * carries only the fields the card actually changed. */
  onPatchFog: (
    current: FogDTO,
    patch: { question?: string; position?: number; resolvedAt?: number | null },
  ) => void;
  /** #629: the action list's read — fetches one project's actions, route
   * order. Same "the caller's own effect decides when" shape as
   * `onRequestProjectLinks`. */
  onRequestActions: (projectId: string) => void;
  /** #629: the action list's reorder gesture — moves one Action's
   * `projectPos`. Overlaid immediately (`Core::patch_action_position`'s
   * own doc), unlike every other project-lane write on this screen. */
  onReorderAction: (projectId: string, current: TaskItemDTO, position: number) => void;
  /** #629: an expanded action's checklist read — the same `getSteps` door
   * item detail's own checklist already uses (issue #96), reused here. */
  onRequestActionSteps: (itemId: string) => void;
  /** #629: an expanded action's add-a-step gesture. */
  onCreateStep: (itemId: string, body: string, position: number) => void;
  /** #629: an expanded action's tick/reword/reorder/delete gesture —
   * `patch` carries only the fields the row actually changed. */
  onPatchStep: (
    current: StepDTO,
    patch: { body?: string; done?: boolean; position?: number; deletedAt?: number | null },
  ) => void;
}

export function ProjectsScreen({
  task,
  onCreateProject,
  onPatchProject,
  onRequestProjectLinks,
  onCreateProjectLink,
  onPatchProjectLink,
  onRequestRoute,
  onPatchRoute,
  onRequestFog,
  onCreateFog,
  onPatchFog,
  onRequestActions,
  onReorderAction,
  onRequestActionSteps,
  onCreateStep,
  onPatchStep,
}: ProjectsScreenProps) {
  const [openId, setOpenId] = useState<string | null>(null);

  // `null` is "not read yet", not "no projects" (`TaskState.projects`' own
  // doc). An empty grid here would be a claim this device has no standing to
  // make before the core has answered.
  if (task.projects === null) {
    return (
      <Card padding="var(--space-6)">
        <span style={{ font: "var(--type-body)", color: "var(--text-secondary)" }}>
          Reading projects…
        </span>
      </Card>
    );
  }

  // Both halves, because an archived project is *absent* in the mirror and
  // the live read cannot see one at all (`Core::archived_projects`' own doc).
  // Reading `projects` alone here would leave the Show-archived toggle
  // permanently empty against real data while passing on any hand-authored
  // fixture — the toggle would work everywhere except the app.
  const rows = projectRoster([...task.projects, ...(task.archivedProjects ?? [])], task.ledger);
  const open = rows.find((row) => row.project.id === openId);

  return open === undefined ? (
    <Grid
      rows={rows}
      lastProjectWrite={task.lastProjectWrite}
      onOpen={setOpenId}
      onCreateProject={onCreateProject}
    />
  ) : (
    <Dossier
      row={open}
      lastProjectWrite={task.lastProjectWrite}
      onBack={() => setOpenId(null)}
      onPatchProject={onPatchProject}
      ledger={task.ledger}
      links={task.linksByProject[open.project.id]}
      lastProjectLinkWrite={task.lastProjectLinkWrite}
      onRequestProjectLinks={onRequestProjectLinks}
      onCreateProjectLink={onCreateProjectLink}
      onPatchProjectLink={onPatchProjectLink}
      route={task.routeByProject[open.project.id]}
      lastRouteWrite={task.lastRouteWrite}
      onRequestRoute={onRequestRoute}
      onPatchRoute={onPatchRoute}
      fog={task.fogByProject[open.project.id]}
      lastFogWrite={task.lastFogWrite}
      onRequestFog={onRequestFog}
      onCreateFog={onCreateFog}
      onPatchFog={onPatchFog}
      actions={task.actionsByProject[open.project.id]}
      lastActionReorder={task.lastActionReorder}
      onRequestActions={onRequestActions}
      onReorderAction={onReorderAction}
      stepsByItem={task.stepsByItem}
      lastStepWrite={task.lastStepWrite}
      onRequestActionSteps={onRequestActionSteps}
      onCreateStep={onCreateStep}
      onPatchStep={onPatchStep}
      syncOutcomeSeq={task.syncOutcomeSeq}
    />
  );
}

function Grid({
  rows,
  lastProjectWrite,
  onOpen,
  onCreateProject,
}: {
  rows: ProjectRow[];
  lastProjectWrite: TaskProjectResult | null;
  onOpen: (id: string) => void;
  onCreateProject: (name: string) => void;
}) {
  const [showArchived, setShowArchived] = useState(false);
  const visible = visibleRows(rows, showArchived);
  const waiting = awaitingCreate(rows, lastProjectWrite);
  const failure = writeFailureMessage(lastProjectWrite);

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-6)" }}>
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: "var(--space-6)" }}>
        <span className="hb-meta">{rosterSummary(rows)}</span>
        <Switch
          checked={showArchived}
          onChange={() => setShowArchived((current) => !current)}
          label="Show archived"
        />
      </div>

      {failure !== null ? <Badge tone="danger">{failure}</Badge> : null}

      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(auto-fill, minmax(240px, 1fr))",
          gap: "var(--space-6)",
          // Cards size to their own content rather than stretching to the
          // tallest cell — the New-project card is the tall one, and without
          // this every project card wears its height as empty space. The
          // regions that will fill a card out are later slices' (#627's
          // destination excerpt, #625's repo badge).
          alignItems: "start",
        }}
      >
        {visible.map((row) => (
          <Card
            key={row.project.id}
            as="button"
            interactive
            onClick={() => onOpen(row.project.id)}
            style={{
              display: "flex",
              flexDirection: "column",
              alignItems: "flex-start",
              gap: "var(--space-4)",
              textAlign: "left",
              opacity: row.archived ? 0.6 : 1,
            }}
          >
            <div style={{ display: "flex", alignItems: "baseline", gap: "var(--space-4)", width: "100%" }}>
              <h3 style={{ font: "var(--type-h3)", color: "var(--text-primary)", flex: 1, minWidth: 0 }}>
                {row.project.name}
              </h3>
              {row.archived ? <Badge mono>archived</Badge> : null}
            </div>
            <span className="hb-meta">{countsMeta(row.counts)}</span>
          </Card>
        ))}

        <NewProjectCard waiting={waiting} onCreateProject={onCreateProject} />
      </div>

      {rows.length === 0 && !waiting ? (
        <Card padding="0">
          <EmptyState
            icon="folder-kanban"
            headingLevel={3}
            title="No projects yet"
            body="A project is a Route plus the actions on it. Name one above to start."
          />
        </Card>
      ) : null}
    </div>
  );
}

/** The grid's inline create — a dashed card in the grid rather than a
 * separate form, so minting a project is one gesture from the list. */
function NewProjectCard({
  waiting,
  onCreateProject,
}: {
  waiting: boolean;
  onCreateProject: (name: string) => void;
}) {
  const [name, setName] = useState("");
  const trimmed = name.trim();

  function submit() {
    if (trimmed === "") {
      return;
    }
    onCreateProject(trimmed);
    setName("");
  }

  return (
    <Card
      as="form"
      onSubmit={(event) => {
        event.preventDefault();
        submit();
      }}
      style={{
        borderStyle: "dashed",
        boxShadow: "none",
        display: "flex",
        flexDirection: "column",
        gap: "var(--space-4)",
        justifyContent: "center",
      }}
    >
      <span style={{ font: "var(--type-body-strong)", color: "var(--text-primary)" }}>New project</span>
      <Input
        label="Name"
        value={name}
        placeholder="Name"
        onChange={(event) => setName(event.target.value)}
      />
      <Button type="submit" size="sm" disabled={trimmed === ""}>
        Create
      </Button>
      {waiting ? <span className="hb-meta">{WAITING_COPY}</span> : null}
    </Card>
  );
}

function Dossier({
  row,
  lastProjectWrite,
  onBack,
  onPatchProject,
  ledger,
  links,
  lastProjectLinkWrite,
  onRequestProjectLinks,
  onCreateProjectLink,
  onPatchProjectLink,
  route,
  lastRouteWrite,
  onRequestRoute,
  onPatchRoute,
  fog,
  lastFogWrite,
  onRequestFog,
  onCreateFog,
  onPatchFog,
  actions,
  lastActionReorder,
  onRequestActions,
  onReorderAction,
  stepsByItem,
  lastStepWrite,
  onRequestActionSteps,
  onCreateStep,
  onPatchStep,
  syncOutcomeSeq,
}: {
  row: ProjectRow;
  lastProjectWrite: TaskProjectResult | null;
  onBack: () => void;
  onPatchProject: (
    current: ProjectDTO,
    patch: { githubRepo?: string | null; defaultContext?: string | null; archivedAt?: number | null },
  ) => void;
  /** The Ledger (#630's `ArchiveCard`'s own read) — already app-wide state,
   * the same "counts are derived, not read" doctrine `roster.ts` states for
   * the grid's own counts. `null` while it has not answered yet. */
  ledger: LedgerRowDTO[] | null;
  /** `undefined` = not read yet (`TaskState.linksByProject`'s own doc),
   * distinct from `[]` (this project genuinely has none). */
  links: ProjectLinkDTO[] | undefined;
  lastProjectLinkWrite: TaskProjectLinkResult | null;
  onRequestProjectLinks: (projectId: string) => void;
  onCreateProjectLink: (projectId: string, url: string, label: string | null, position: number) => void;
  onPatchProjectLink: (
    current: ProjectLinkDTO,
    patch: { url?: string; label?: string | null; position?: number; removedAt?: number | null },
  ) => void;
  /** `undefined` = not read yet (`TaskState.routeByProject`'s own doc) —
   * every project has exactly one Route, so there is no "this project has
   * none" to distinguish it from, unlike `links`. */
  route: RouteDTO | undefined;
  lastRouteWrite: TaskRouteResult | null;
  onRequestRoute: (projectId: string) => void;
  onPatchRoute: (current: RouteDTO, patch: { destination?: string | null; notes?: string | null }) => void;
  /** `undefined` = not read yet (`TaskState.fogByProject`'s own doc),
   * distinct from `[]` (this project genuinely has no open fog). */
  fog: FogDTO[] | undefined;
  lastFogWrite: TaskFogResult | null;
  onRequestFog: (projectId: string) => void;
  onCreateFog: (projectId: string, question: string, position: number) => void;
  onPatchFog: (
    current: FogDTO,
    patch: { question?: string; position?: number; resolvedAt?: number | null },
  ) => void;
  /** `undefined` = not read yet (`TaskState.actionsByProject`'s own doc),
   * distinct from `[]` (this project genuinely has no actions). */
  actions: TaskItemDTO[] | undefined;
  lastActionReorder: TaskActionReorderResult | null;
  onRequestActions: (projectId: string) => void;
  onReorderAction: (projectId: string, current: TaskItemDTO, position: number) => void;
  /** Item detail's checklist store (issue #96, S10) — the expanded action's
   * steps read the same map, keyed by item id. */
  stepsByItem: Record<string, StepDTO[]>;
  lastStepWrite: TaskStepResult | null;
  onRequestActionSteps: (itemId: string) => void;
  onCreateStep: (itemId: string, body: string, position: number) => void;
  onPatchStep: (
    current: StepDTO,
    patch: { body?: string; done?: boolean; position?: number; deletedAt?: number | null },
  ) => void;
  syncOutcomeSeq: number;
}) {
  const projectId = row.project.id;

  // #626: fetches this dossier's links the moment it opens, and again on
  // every completed cycle it stays open for — same "the caller's own
  // effect decides when" shape `useItemDetailWiring.ts`'s `getSteps` effect
  // uses for item detail's checklist.
  useEffect(() => {
    onRequestProjectLinks(projectId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId, syncOutcomeSeq]);

  // #627: same shape, for the Route.
  useEffect(() => {
    onRequestRoute(projectId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId, syncOutcomeSeq]);

  // #628: same shape, for the open fog.
  useEffect(() => {
    onRequestFog(projectId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId, syncOutcomeSeq]);

  // #629: same shape, for the ordered action list.
  useEffect(() => {
    onRequestActions(projectId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId, syncOutcomeSeq]);

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-6)" }}>
      <div style={{ display: "flex", alignItems: "center", gap: "var(--space-5)", flexWrap: "wrap" }}>
        <Button variant="ghost" size="sm" iconLeft="arrow-left" onClick={onBack}>
          All projects
        </Button>
        <span className="hb-meta">{countsMeta(row.counts)}</span>
        {row.archived ? <Badge mono>archived</Badge> : null}
      </div>

      {/* h2 under the shell's h1 — `--type-h2` is the size token, not the
          level, and heading levels must not skip. */}
      <h2 style={{ font: "var(--type-h2)", color: "var(--text-primary)" }}>{row.project.name}</h2>

      <TwoColumn>
        <Column>
          <RouteCard
            projectId={projectId}
            route={route}
            lastRouteWrite={lastRouteWrite}
            onPatchRoute={onPatchRoute}
          />
          <ActionsCard
            projectId={projectId}
            actions={actions}
            lastActionReorder={lastActionReorder}
            onReorderAction={onReorderAction}
            stepsByItem={stepsByItem}
            lastStepWrite={lastStepWrite}
            onRequestActionSteps={onRequestActionSteps}
            onCreateStep={onCreateStep}
            onPatchStep={onPatchStep}
            syncOutcomeSeq={syncOutcomeSeq}
          />
          <FogCard
            projectId={projectId}
            fog={fog}
            lastFogWrite={lastFogWrite}
            onCreateFog={onCreateFog}
            onPatchFog={onPatchFog}
          />
        </Column>
        <Aside label="Project properties">
          <PropertiesCard
            project={row.project}
            lastProjectWrite={lastProjectWrite}
            onPatchProject={onPatchProject}
          />
          <LinksCard
            projectId={projectId}
            links={links}
            lastProjectLinkWrite={lastProjectLinkWrite}
            onCreateProjectLink={onCreateProjectLink}
            onPatchProjectLink={onPatchProjectLink}
          />
          <ArchiveCard
            project={row.project}
            liveCount={liveItemCount(ledger, projectId)}
            lastProjectWrite={lastProjectWrite}
            onPatchProject={onPatchProject}
          />
        </Aside>
      </TwoColumn>
    </div>
  );
}

/** The dossier's properties card (#625, ADR-0030 decisions 2–3): shows and
 * edits `githubRepo`/`defaultContext`, the two columns #625 adds. The repo
 * renders as a derived link (`githubRepoUrl`) — the stored value is always
 * the bare `owner/repo` slug, never the URL. Local edit state re-syncs from
 * `project` whenever its `version` moves, which is what lets a patch this
 * card just sent (no optimistic overlay) settle into the fields once the
 * next completed cycle pulls it back, rather than being clobbered by the
 * stale value already on screen — but **per field**, and only where the
 * field still holds what the last seed put there. The two fields share one
 * Save and one version, so a re-seed of both would let the repo's write
 * landing discard a default context typed while it was in flight. */
function PropertiesCard({
  project,
  lastProjectWrite,
  onPatchProject,
}: {
  project: ProjectDTO;
  lastProjectWrite: TaskProjectResult | null;
  onPatchProject: (
    current: ProjectDTO,
    patch: { githubRepo?: string | null; defaultContext?: string | null },
  ) => void;
}) {
  const [repoInput, setRepoInput] = useState(project.githubRepo ?? "");
  const [contextInput, setContextInput] = useState(project.defaultContext ?? "");
  const [synced, setSynced] = useState({
    version: project.version,
    repo: project.githubRepo ?? "",
    context: project.defaultContext ?? "",
  });

  if (project.version !== synced.version) {
    // Only a field nobody has typed in since the last seed is re-seeded. A
    // version can move for a reason that has nothing to do with the field
    // being edited — saving the repo bumps it while a default context is
    // half-typed — and re-seeding both would throw that typing away, the
    // one thing an editor must not do (`triage-form.ts`'s `effectiveDraft`
    // makes the same guarantee structurally, by keeping only touched fields
    // in state). "Untouched" is the input still holding exactly what the
    // last seed put there.
    if (repoInput === synced.repo) {
      setRepoInput(project.githubRepo ?? "");
    }
    if (contextInput === synced.context) {
      setContextInput(project.defaultContext ?? "");
    }
    setSynced({
      version: project.version,
      repo: project.githubRepo ?? "",
      context: project.defaultContext ?? "",
    });
  }

  const trimmedRepo = repoInput.trim();
  const trimmedContext = contextInput.trim();
  const repoChanged = trimmedRepo !== (project.githubRepo ?? "");
  const contextChanged = trimmedContext !== (project.defaultContext ?? "");
  const dirty = repoChanged || contextChanged;
  const link = githubRepoUrl(project.githubRepo);
  // Gated on `projectId` — `lastProjectWrite` is one global slot shared with
  // `createProject` and every other dossier's own patch, so an unguarded
  // read would paint a stranger's failure into this card.
  const failure =
    lastProjectWrite !== null && lastProjectWrite.projectId === project.id
      ? writeFailureMessage(lastProjectWrite)
      : null;

  function save() {
    const patch: { githubRepo?: string | null; defaultContext?: string | null } = {};
    if (repoChanged) {
      patch.githubRepo = trimmedRepo === "" ? null : trimmedRepo;
    }
    if (contextChanged) {
      patch.defaultContext = trimmedContext === "" ? null : trimmedContext;
    }
    onPatchProject(project, patch);
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
      <span className="hb-meta">properties</span>
      <Card
        as="form"
        padding="var(--space-5)"
        onSubmit={(event) => {
          event.preventDefault();
          save();
        }}
        style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}
      >
        <Input
          label="GitHub repo"
          placeholder="owner/repo"
          value={repoInput}
          onChange={(event) => setRepoInput(event.target.value)}
        />
        {link !== null ? (
          <a
            href={link}
            target="_blank"
            rel="noreferrer"
            style={{ font: "var(--type-body-sm)", color: "var(--accent)" }}
          >
            {link}
          </a>
        ) : null}
        <Input
          label="Default context"
          placeholder="@computer"
          value={contextInput}
          onChange={(event) => setContextInput(event.target.value)}
          hint="Copied onto an action minted with no context of its own."
        />
        {failure !== null ? <Badge tone="danger">{failure}</Badge> : null}
        <Button type="submit" size="sm" disabled={!dirty}>
          Save
        </Button>
      </Card>
    </div>
  );
}

/** The dossier reading column's Route card (#627, ADR-0030 decision 1):
 * edits `destination`/`notes`, the two fields `CONTEXT.md`'s Route entry
 * names. `route` is `undefined` while the read is in flight
 * (`TaskState.routeByProject`'s own doc) — every project has exactly one
 * Route, so there is no "none yet" state to distinguish, unlike the links
 * card. The re-seed-on-version-bump logic is `PropertiesCard`'s own,
 * ported verbatim: only a field nobody has typed in since the last seed is
 * re-seeded, so a save of one field never throws away in-flight typing in
 * the other.
 *
 * **The saving/saved state is explicit here**, unlike `PropertiesCard`'s own
 * silent re-sync — this slice's brief asks for it, since there is no
 * optimistic overlay to imply it visually. `saving` flips true the moment
 * Save is clicked, and resolves one of two ways: it flips false (to
 * `justSaved`) once the row's `version` actually moves — which is also how
 * a 409 that this write lost would show up: `version` still moves (to
 * whatever won), so this card cannot tell its own write apart from a
 * concurrent one and does not try to — or, if the write never reached the
 * queue at all (`lastRouteWrite.kind` `"failed"`/`"busy"`, e.g. a sync cycle
 * holding the host at click time), it flips false with no `justSaved`, so
 * `Saving…` cannot outlive the failure badge painted from that same
 * `lastRouteWrite`. A genuine 409 conflict is never surfaced here; it lands
 * in the ordinary dead-letter journal like every other CAS write (ADR-0030
 * decision 1) — this card adds no bespoke conflict UI. */
function RouteCard({
  projectId,
  route,
  lastRouteWrite,
  onPatchRoute,
}: {
  projectId: string;
  route: RouteDTO | undefined;
  lastRouteWrite: TaskRouteResult | null;
  onPatchRoute: (current: RouteDTO, patch: { destination?: string | null; notes?: string | null }) => void;
}) {
  const [destinationInput, setDestinationInput] = useState(route?.destination ?? "");
  const [notesInput, setNotesInput] = useState(route?.notes ?? "");
  const [synced, setSynced] = useState({
    version: route?.version ?? null,
    destination: route?.destination ?? "",
    notes: route?.notes ?? "",
  });
  const [saving, setSaving] = useState(false);
  const [justSaved, setJustSaved] = useState(false);

  if (route !== undefined && route.version !== synced.version) {
    if (destinationInput === synced.destination) {
      setDestinationInput(route.destination ?? "");
    }
    if (notesInput === synced.notes) {
      setNotesInput(route.notes ?? "");
    }
    setSynced({ version: route.version, destination: route.destination ?? "", notes: route.notes ?? "" });
    if (saving) {
      setSaving(false);
      setJustSaved(true);
    }
  }

  // Gated on `projectId`, same reasoning `PropertiesCard`'s own failure read
  // carries: `lastRouteWrite` is one broadcast slot shared by every open
  // dossier.
  const failedHere =
    lastRouteWrite !== null && lastRouteWrite.projectId === projectId && lastRouteWrite.kind !== "ok";

  // The write never reached the queue at all — no `version` bump is ever
  // coming to clear `saving` for us (the branch above only fires on a real
  // version move), so a `"failed"`/`"busy"` result clears it directly. No
  // `justSaved` here: the failure badge below is the visible resolution,
  // not a silent "Saved".
  if (saving && failedHere) {
    setSaving(false);
  }

  const trimmedDestination = destinationInput.trim();
  const trimmedNotes = notesInput.trim();
  const destinationChanged = trimmedDestination !== (route?.destination ?? "");
  const notesChanged = trimmedNotes !== (route?.notes ?? "");
  const dirty = destinationChanged || notesChanged;
  const failure =
    failedHere && lastRouteWrite !== null
      ? lastRouteWrite.error ?? "That route write did not go through."
      : null;

  function save() {
    if (route === undefined) {
      return;
    }
    const patch: { destination?: string | null; notes?: string | null } = {};
    if (destinationChanged) {
      patch.destination = trimmedDestination === "" ? null : trimmedDestination;
    }
    if (notesChanged) {
      patch.notes = trimmedNotes === "" ? null : trimmedNotes;
    }
    onPatchRoute(route, patch);
    setSaving(true);
    setJustSaved(false);
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
      <span className="hb-meta">route · destination</span>
      <Card
        as="form"
        padding="var(--space-5)"
        onSubmit={(event) => {
          event.preventDefault();
          save();
        }}
        style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}
      >
        {route === undefined ? (
          <span style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
            Reading Route…
          </span>
        ) : (
          <>
            <Textarea
              label="Destination"
              placeholder="What does done look like, in your own terms?"
              value={destinationInput}
              onChange={(event) => {
                setDestinationInput(event.target.value);
                setJustSaved(false);
              }}
            />
            <Textarea
              label="Notes"
              value={notesInput}
              onChange={(event) => {
                setNotesInput(event.target.value);
                setJustSaved(false);
              }}
            />
            {failure !== null ? <Badge tone="danger">{failure}</Badge> : null}
            <div style={{ display: "flex", alignItems: "center", gap: "var(--space-4)" }}>
              <Button type="submit" size="sm" disabled={!dirty}>
                Save
              </Button>
              {saving ? (
                <span className="hb-meta">Saving…</span>
              ) : justSaved ? (
                <span className="hb-meta">Saved</span>
              ) : null}
            </div>
          </>
        )}
      </Card>
    </div>
  );
}

/** The dossier aside's links card (#626, ADR-0030 decision 4): adds, edits,
 * reorders and removes this project's Links. `links` is `undefined` while
 * the read is in flight (`TaskState.linksByProject`'s own "only what a view
 * asked about" doc) — distinct from an empty array, which is a real answer
 * ("this project has none"). A removed link is flagged server-side
 * (`removedAt`), never erased — the mirror's own `links_for_project`
 * already filters to live rows, so a removed link simply stops appearing
 * here; there is no client-side filter to get wrong.
 *
 * Reordering swaps two adjacent rows' `position` in one gesture — two CAS
 * patches, one per row — rather than renumbering the whole list, so an
 * interleaved edit from another device only ever collides with the two
 * rows actually touched. */
function LinksCard({
  projectId,
  links,
  lastProjectLinkWrite,
  onCreateProjectLink,
  onPatchProjectLink,
}: {
  projectId: string;
  links: ProjectLinkDTO[] | undefined;
  lastProjectLinkWrite: TaskProjectLinkResult | null;
  onCreateProjectLink: (projectId: string, url: string, label: string | null, position: number) => void;
  onPatchProjectLink: (
    current: ProjectLinkDTO,
    patch: { url?: string; label?: string | null; position?: number; removedAt?: number | null },
  ) => void;
}) {
  const [urlInput, setUrlInput] = useState("");
  const [labelInput, setLabelInput] = useState("");
  const [editingId, setEditingId] = useState<string | null>(null);

  // Gated on `projectId`, same reasoning `PropertiesCard`'s own failure
  // read carries: `lastProjectLinkWrite` is one broadcast slot shared by
  // every open dossier, so an unguarded read would paint a stranger's
  // failure into this card.
  const failure =
    lastProjectLinkWrite !== null && lastProjectLinkWrite.projectId === projectId
      ? (lastProjectLinkWrite.kind === "ok" ? null : lastProjectLinkWrite.error ?? "That link write did not go through.")
      : null;

  const sortedLinks = links === undefined ? undefined : [...links].sort((a, b) => a.position - b.position);

  function addLink() {
    const trimmedUrl = urlInput.trim();
    if (trimmedUrl === "") {
      return;
    }
    const trimmedLabel = labelInput.trim();
    // Only reachable once the read has answered — Add is disabled while
    // `sortedLinks` is `undefined`, because "no links yet" and "not known
    // yet" would otherwise both mint position 0, and against a project that
    // does have links that 0 duplicates the first row's: the same collision
    // the count-minted position below is written to avoid, arrived at from
    // the other direction.
    // Mint the new position past the largest live one, not from the count:
    // removal is a soft flag, so live positions develop gaps (0,1,2 minus
    // the middle leaves 0,2 with length 2) and a count-minted position
    // would duplicate a live row's — turning the next reorder swap into a
    // value-identical no-op patch. Removed rows cannot count toward the
    // max (the mirror filters them out before this card ever sees them),
    // and they need not: a collision with a removed row's position is
    // harmless, since only live rows are ever sorted or swapped.
    const position = sortedLinks === undefined || sortedLinks.length === 0 ? 0 : sortedLinks[sortedLinks.length - 1].position + 1;
    onCreateProjectLink(projectId, trimmedUrl, trimmedLabel === "" ? null : trimmedLabel, position);
    setUrlInput("");
    setLabelInput("");
  }

  function moveLink(index: number, direction: -1 | 1) {
    if (sortedLinks === undefined) {
      return;
    }
    const swapIndex = index + direction;
    if (swapIndex < 0 || swapIndex >= sortedLinks.length) {
      return;
    }
    const link = sortedLinks[index];
    const other = sortedLinks[swapIndex];
    onPatchProjectLink(link, { position: other.position });
    onPatchProjectLink(other, { position: link.position });
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
      <span className="hb-meta">links</span>
      <Card padding="var(--space-5)" style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
        {sortedLinks === undefined ? (
          <span style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>Reading links…</span>
        ) : sortedLinks.length === 0 ? (
          <span style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>No links yet.</span>
        ) : (
          <ul style={{ listStyle: "none", margin: 0, padding: 0, display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
            {sortedLinks.map((link, index) =>
              editingId === link.id ? (
                <LinkEditRow
                  key={link.id}
                  link={link}
                  onSave={(url, label) => {
                    onPatchProjectLink(link, { url, label });
                    setEditingId(null);
                  }}
                  onCancel={() => setEditingId(null)}
                />
              ) : (
                <li key={link.id} style={{ display: "flex", alignItems: "center", gap: "var(--space-2)" }}>
                  <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column" }}>
                    <a
                      href={link.url}
                      target="_blank"
                      rel="noreferrer"
                      style={{
                        font: "var(--type-body-sm)",
                        color: "var(--accent)",
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                        whiteSpace: "nowrap",
                      }}
                    >
                      {link.label ?? link.url}
                    </a>
                    {link.label !== null ? (
                      <span
                        className="hb-meta"
                        style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}
                      >
                        {link.url}
                      </span>
                    ) : null}
                  </div>
                  <IconButton
                    icon="chevron-down"
                    label="Move up"
                    size="sm"
                    style={{ transform: "rotate(180deg)" }}
                    disabled={index === 0}
                    onClick={() => moveLink(index, -1)}
                  />
                  <IconButton
                    icon="chevron-down"
                    label="Move down"
                    size="sm"
                    disabled={index === sortedLinks.length - 1}
                    onClick={() => moveLink(index, 1)}
                  />
                  <Button variant="ghost" size="sm" onClick={() => setEditingId(link.id)}>
                    Edit
                  </Button>
                  <IconButton
                    icon="trash-2"
                    label="Remove link"
                    size="sm"
                    onClick={() => onPatchProjectLink(link, { removedAt: Date.now() })}
                  />
                </li>
              ),
            )}
          </ul>
        )}

        <form
          onSubmit={(event) => {
            event.preventDefault();
            addLink();
          }}
          style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}
        >
          <Input
            label="URL"
            placeholder="https://…"
            value={urlInput}
            onChange={(event) => setUrlInput(event.target.value)}
          />
          <Input
            label="Label"
            placeholder="optional"
            value={labelInput}
            onChange={(event) => setLabelInput(event.target.value)}
          />
          {failure !== null ? <Badge tone="danger">{failure}</Badge> : null}
          <Button type="submit" size="sm" disabled={urlInput.trim() === "" || sortedLinks === undefined}>
            Add link
          </Button>
        </form>
      </Card>
    </div>
  );
}

/** One link's inline edit form — swapped in for its display row
 * (`LinksCard`'s own `editingId`), the same "click to reveal the fields"
 * shape the rest of this app uses rather than a modal. */
function LinkEditRow({
  link,
  onSave,
  onCancel,
}: {
  link: ProjectLinkDTO;
  onSave: (url: string, label: string | null) => void;
  onCancel: () => void;
}) {
  const [urlInput, setUrlInput] = useState(link.url);
  const [labelInput, setLabelInput] = useState(link.label ?? "");

  function save() {
    const trimmedUrl = urlInput.trim();
    if (trimmedUrl === "") {
      return;
    }
    const trimmedLabel = labelInput.trim();
    onSave(trimmedUrl, trimmedLabel === "" ? null : trimmedLabel);
  }

  return (
    <li style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
      <Input label="URL" value={urlInput} onChange={(event) => setUrlInput(event.target.value)} />
      <Input label="Label" placeholder="optional" value={labelInput} onChange={(event) => setLabelInput(event.target.value)} />
      <div style={{ display: "flex", gap: "var(--space-3)" }}>
        <Button size="sm" onClick={save} disabled={urlInput.trim() === ""}>
          Save
        </Button>
        <Button variant="ghost" size="sm" onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </li>
  );
}

/** The dossier aside's archive affordance (#630, ADR-0030 decision 5) — the
 * project lane's last placeholder, replacing `ComingRegion`. Archiving
 * cascades onto the project's live items server-side, a timestamp-matched
 * stamp (`projects.rs`'s own doc, `cascade_archive_for_project`'s own
 * mechanism); this card's job is naming that consequence concretely before
 * the human commits to it, and offering the reverse gesture once archived.
 *
 * **Confirmation is a two-step reveal within the card, not a modal** — this
 * codebase has no modal primitive (nothing under `components/` is one, and
 * neither `LinksCard` nor `FogCard` uses one for their own destructive
 * gestures), and Cancel is one click away either way. `liveCount` is
 * `roster.ts`'s `liveItemCount`, off the Ledger this screen already holds
 * app-wide — the same "counts are derived, not read" doctrine `countsMeta`
 * follows, scoped to exactly the predicate the server cascade uses
 * (`project_id = ? AND archived_at IS NULL`, `done` included), so the
 * number named here is the number that will move. `null` renders as prose,
 * never `0` — a count this device has no standing to claim yet.
 *
 * No optimistic overlay (`Core::patch_project`'s own doc): `pending` flips
 * true on click and resolves either when the project's own `version` moves
 * (success — the archived/unarchived state repaints from `project` itself)
 * or when `lastProjectWrite.kind !== "ok"` (failure, which clears `pending`
 * directly since no version bump is ever coming to do it) — `RouteCard`'s
 * own pattern, ported verbatim including the failed/busy-clears-the-flag
 * rule the #627/#628/#629 reviews paid for. Unarchiving carries no
 * confirmation step of its own: ADR-0030's brief asks the dialog to name
 * archiving's consequence, not unarchiving's, and unarchiving is the
 * reversing gesture, not a destructive one. */
function ArchiveCard({
  project,
  liveCount,
  lastProjectWrite,
  onPatchProject,
}: {
  project: ProjectDTO;
  liveCount: number | null;
  lastProjectWrite: TaskProjectResult | null;
  onPatchProject: (current: ProjectDTO, patch: { archivedAt?: number | null }) => void;
}) {
  const [confirming, setConfirming] = useState(false);
  const [pending, setPending] = useState(false);
  const [syncedVersion, setSyncedVersion] = useState(project.version);

  if (project.version !== syncedVersion) {
    setSyncedVersion(project.version);
    if (pending) {
      setPending(false);
      setConfirming(false);
    }
  }

  // Gated on `project.id`, same reasoning `PropertiesCard`'s own failure
  // read carries: `lastProjectWrite` is one broadcast slot shared by every
  // open dossier's own patch.
  const failedHere =
    lastProjectWrite !== null && lastProjectWrite.projectId === project.id && lastProjectWrite.kind !== "ok";
  if (pending && failedHere) {
    setPending(false);
  }
  const failure = failedHere ? writeFailureMessage(lastProjectWrite) : null;

  const archived = project.archivedAt !== null;

  function archive() {
    onPatchProject(project, { archivedAt: Date.now() });
    setPending(true);
  }

  function unarchive() {
    onPatchProject(project, { archivedAt: null });
    setPending(true);
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
      <span className="hb-meta">archive</span>
      <Card padding="var(--space-5)" style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
        {archived ? (
          <>
            <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
              Archived. Unarchiving restores exactly the items this archive took down.
            </p>
            <Button variant="secondary" size="sm" onClick={unarchive} disabled={pending}>
              {pending ? "Unarchiving…" : "Unarchive"}
            </Button>
          </>
        ) : confirming ? (
          <>
            <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
              {liveCount === null
                ? "Archiving takes every live item in this project down with it."
                : liveCount === 0
                  ? "This project has no live items — archiving takes down the project alone."
                  : `Archiving takes ${liveCount} live ${liveCount === 1 ? "item" : "items"} down with it.`}
            </p>
            <div style={{ display: "flex", gap: "var(--space-3)" }}>
              <Button variant="danger" size="sm" onClick={archive} disabled={pending}>
                {pending ? "Archiving…" : "Archive project"}
              </Button>
              <Button variant="ghost" size="sm" onClick={() => setConfirming(false)} disabled={pending}>
                Cancel
              </Button>
            </div>
          </>
        ) : (
          <Button variant="secondary" size="sm" onClick={() => setConfirming(true)}>
            Archive project
          </Button>
        )}
        {failure !== null ? <Badge tone="danger">{failure}</Badge> : null}
      </Card>
    </div>
  );
}

/** The dossier reading column's action list (#629, ADR-0030 decision 1):
 * renders a project's Actions in route (`projectPos`) order, reorderable
 * with up/down controls that stop at the ends, and each one expanding
 * **inline beneath its own row** — never into the aside, per the brief's
 * one non-negotiable styling detail — to its own steps checklist
 * (`ActionStepsChecklist`) when selected. `actions` is `undefined` while
 * the read is in flight (`TaskState.actionsByProject`'s own doc) — distinct
 * from an empty array, which is a real answer ("this project has no
 * actions yet").
 *
 * **Reordering overlays immediately.** Unlike `FogCard`/`LinksCard`'s own
 * reorder, `onReorderAction` patches the ordinary item table
 * (`Core::patch_action_position`'s own doc), so the swapped rows repaint
 * the instant the click handler returns — no "waiting" state is drawn.
 * Same "swap two adjacent rows, two CAS patches" gesture `FogCard`'s own
 * `moveFog` uses, and for the same reason: an interleaved edit from
 * another device only ever collides with the two rows actually touched. */
function ActionsCard({
  projectId,
  actions,
  lastActionReorder,
  onReorderAction,
  stepsByItem,
  lastStepWrite,
  onRequestActionSteps,
  onCreateStep,
  onPatchStep,
  syncOutcomeSeq,
}: {
  projectId: string;
  actions: TaskItemDTO[] | undefined;
  lastActionReorder: TaskActionReorderResult | null;
  onReorderAction: (projectId: string, current: TaskItemDTO, position: number) => void;
  stepsByItem: Record<string, StepDTO[]>;
  lastStepWrite: TaskStepResult | null;
  onRequestActionSteps: (itemId: string) => void;
  onCreateStep: (itemId: string, body: string, position: number) => void;
  onPatchStep: (
    current: StepDTO,
    patch: { body?: string; done?: boolean; position?: number; deletedAt?: number | null },
  ) => void;
  syncOutcomeSeq: number;
}) {
  const [expandedId, setExpandedId] = useState<string | null>(null);

  // Fetches the expanded action's steps the moment it is selected, and
  // again on every completed cycle it stays selected for — same "the
  // caller's own effect decides when" shape `Dossier`'s own `onRequestFog`
  // effect uses.
  useEffect(() => {
    if (expandedId !== null) {
      onRequestActionSteps(expandedId);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [expandedId, syncOutcomeSeq]);

  // Gated on `projectId`, same reasoning `FogCard`'s own failure read
  // carries: `lastActionReorder` is one broadcast slot shared by every open
  // dossier, so an unguarded read would paint a stranger's failure into
  // this card.
  const failure =
    lastActionReorder !== null && lastActionReorder.projectId === projectId && lastActionReorder.kind !== "ok"
      ? lastActionReorder.error ?? "That reorder did not go through."
      : null;

  const sortedActions =
    actions === undefined ? undefined : [...actions].sort((a, b) => (a.projectPos ?? 0) - (b.projectPos ?? 0));

  function moveAction(index: number, direction: -1 | 1) {
    if (sortedActions === undefined) {
      return;
    }
    const swapIndex = index + direction;
    if (swapIndex < 0 || swapIndex >= sortedActions.length) {
      return;
    }
    const current = sortedActions[index];
    const other = sortedActions[swapIndex];
    onReorderAction(projectId, current, other.projectPos ?? swapIndex);
    onReorderAction(projectId, other, current.projectPos ?? index);
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
      <span className="hb-meta">actions</span>
      <Card padding="var(--space-5)" style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
        {sortedActions === undefined ? (
          <span style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
            Reading actions…
          </span>
        ) : sortedActions.length === 0 ? (
          <span style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
            No actions on this Route yet.
          </span>
        ) : (
          <ul style={{ listStyle: "none", margin: 0, padding: 0, display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
            {sortedActions.map((action, index) => (
              <li key={action.id} style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
                <div style={{ display: "flex", alignItems: "center", gap: "var(--space-2)" }}>
                  <button
                    type="button"
                    onClick={() => setExpandedId(expandedId === action.id ? null : action.id)}
                    style={{
                      flex: 1,
                      minWidth: 0,
                      textAlign: "left",
                      background: "none",
                      border: "none",
                      padding: 0,
                      cursor: "pointer",
                      font: "var(--type-body-sm)",
                      color: "var(--text-primary)",
                    }}
                  >
                    {action.title}
                  </button>
                  {action.pending ? <Badge mono>pending</Badge> : null}
                  <IconButton
                    icon="chevron-down"
                    label="Move up"
                    size="sm"
                    style={{ transform: "rotate(180deg)" }}
                    disabled={index === 0}
                    onClick={() => moveAction(index, -1)}
                  />
                  <IconButton
                    icon="chevron-down"
                    label="Move down"
                    size="sm"
                    disabled={index === sortedActions.length - 1}
                    onClick={() => moveAction(index, 1)}
                  />
                </div>
                {expandedId === action.id ? (
                  <ActionStepsChecklist
                    itemId={action.id}
                    steps={stepsByItem[action.id]}
                    lastStepWrite={lastStepWrite}
                    onCreateStep={onCreateStep}
                    onPatchStep={onPatchStep}
                  />
                ) : null}
              </li>
            ))}
          </ul>
        )}
        {failure !== null ? <Badge tone="danger">{failure}</Badge> : null}
      </Card>
    </div>
  );
}

/** One expanded action's steps checklist (#629) — rendered inline beneath
 * its row by `ActionsCard`, never in the aside. Ticking, rewording,
 * reordering and adding a Step all share the no-overlay convention every
 * other project-lane write on this screen uses (`Core::patch_step`'s own
 * doc): a change becomes visible once the next completed cycle pulls it
 * back. Deleting a step flags it (ADR-0020) rather than erasing it — the
 * flagged row simply stops appearing here, since a deleted Step demotes to
 * `Presence::Absent` in the mirror (`sync::mirror`'s own `apply_tables`,
 * unlike Fog's `resolved_at`), so there is no client-side "hide it" logic
 * to get wrong. `steps` is `undefined` while the read is in flight
 * (`TaskState.stepsByItem`'s own doc), distinct from an empty array (this
 * action genuinely has no steps yet). */
function ActionStepsChecklist({
  itemId,
  steps,
  lastStepWrite,
  onCreateStep,
  onPatchStep,
}: {
  itemId: string;
  steps: StepDTO[] | undefined;
  lastStepWrite: TaskStepResult | null;
  onCreateStep: (itemId: string, body: string, position: number) => void;
  onPatchStep: (
    current: StepDTO,
    patch: { body?: string; done?: boolean; position?: number; deletedAt?: number | null },
  ) => void;
}) {
  const [bodyInput, setBodyInput] = useState("");
  const [editingId, setEditingId] = useState<string | null>(null);

  // Gated on `itemId`, same reasoning `ActionsCard`'s own failure read
  // carries: `lastStepWrite` is one broadcast slot shared by every
  // expanded checklist on every open dossier.
  const failure =
    lastStepWrite !== null && lastStepWrite.itemId === itemId
      ? (lastStepWrite.kind === "ok" ? null : lastStepWrite.error ?? "That step write did not go through.")
      : null;

  const sortedSteps = steps === undefined ? undefined : [...steps].sort((a, b) => a.position - b.position);

  function addStep() {
    const trimmedBody = bodyInput.trim();
    if (trimmedBody === "") {
      return;
    }
    // Mint the new position past the largest live one, not from the count
    // — `FogCard.addFog`'s own reasoning, ported verbatim: a deleted step
    // drops out of this read without renumbering the rest, so a
    // count-minted position could duplicate a live row's.
    const position = sortedSteps === undefined || sortedSteps.length === 0 ? 0 : sortedSteps[sortedSteps.length - 1].position + 1;
    onCreateStep(itemId, trimmedBody, position);
    setBodyInput("");
  }

  function moveStep(index: number, direction: -1 | 1) {
    if (sortedSteps === undefined) {
      return;
    }
    const swapIndex = index + direction;
    if (swapIndex < 0 || swapIndex >= sortedSteps.length) {
      return;
    }
    const step = sortedSteps[index];
    const other = sortedSteps[swapIndex];
    onPatchStep(step, { position: other.position });
    onPatchStep(other, { position: step.position });
  }

  return (
    <div style={{ marginLeft: "var(--space-6)", display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
      {sortedSteps === undefined ? (
        <span style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>Reading steps…</span>
      ) : sortedSteps.length === 0 ? (
        <span style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
          No steps on this action yet.
        </span>
      ) : (
        <ul style={{ listStyle: "none", margin: 0, padding: 0, display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
          {sortedSteps.map((step, index) =>
            editingId === step.id ? (
              <StepEditRow
                key={step.id}
                step={step}
                onSave={(body) => {
                  onPatchStep(step, { body });
                  setEditingId(null);
                }}
                onCancel={() => setEditingId(null)}
              />
            ) : (
              <li key={step.id} style={{ display: "flex", alignItems: "center", gap: "var(--space-2)" }}>
                <Checkbox
                  checked={step.done}
                  onChange={() => onPatchStep(step, { done: !step.done })}
                  aria-label={step.body}
                />
                <span
                  style={{
                    flex: 1,
                    minWidth: 0,
                    font: "var(--type-body-sm)",
                    color: step.done ? "var(--text-secondary)" : "var(--text-primary)",
                    textDecoration: step.done ? "line-through" : "none",
                  }}
                >
                  {step.body}
                </span>
                <IconButton
                  icon="chevron-down"
                  label="Move up"
                  size="sm"
                  style={{ transform: "rotate(180deg)" }}
                  disabled={index === 0}
                  onClick={() => moveStep(index, -1)}
                />
                <IconButton
                  icon="chevron-down"
                  label="Move down"
                  size="sm"
                  disabled={index === sortedSteps.length - 1}
                  onClick={() => moveStep(index, 1)}
                />
                <Button variant="ghost" size="sm" onClick={() => setEditingId(step.id)}>
                  Edit
                </Button>
                <IconButton
                  icon="trash-2"
                  label="Delete"
                  size="sm"
                  onClick={() => onPatchStep(step, { deletedAt: Date.now() })}
                />
              </li>
            ),
          )}
        </ul>
      )}

      <form
        onSubmit={(event) => {
          event.preventDefault();
          addStep();
        }}
        style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}
      >
        <Input
          label="New step"
          placeholder="A ~2–5 minute concrete physical step"
          value={bodyInput}
          onChange={(event) => setBodyInput(event.target.value)}
        />
        {failure !== null ? <Badge tone="danger">{failure}</Badge> : null}
        <Button type="submit" size="sm" disabled={bodyInput.trim() === "" || sortedSteps === undefined}>
          Add step
        </Button>
      </form>
    </div>
  );
}

/** One step's inline reword form — swapped in for its display row
 * (`ActionStepsChecklist`'s own `editingId`), the same "click to reveal the
 * fields" shape `FogEditRow` uses. */
function StepEditRow({
  step,
  onSave,
  onCancel,
}: {
  step: StepDTO;
  onSave: (body: string) => void;
  onCancel: () => void;
}) {
  const [bodyInput, setBodyInput] = useState(step.body);

  function save() {
    const trimmedBody = bodyInput.trim();
    if (trimmedBody === "") {
      return;
    }
    onSave(trimmedBody);
  }

  return (
    <li style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
      <Input label="Step" value={bodyInput} onChange={(event) => setBodyInput(event.target.value)} />
      <div style={{ display: "flex", gap: "var(--space-3)" }}>
        <Button size="sm" onClick={save} disabled={bodyInput.trim() === ""}>
          Save
        </Button>
        <Button variant="ghost" size="sm" onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </li>
  );
}

/** The dossier reading column's fog card (#628, ADR-0030 decision 1): lists
 * a project's **open** fog in position order, and adds, edits, reorders and
 * resolves it. `fog` is `undefined` while the read is in flight
 * (`TaskState.fogByProject`'s own doc) — distinct from an empty array,
 * which is a real answer ("this project has no open fog right now").
 * Resolving is a stamp, never a delete (`Core::patch_fog`'s own doc): the
 * mirror's own `open_fog_for` already filters resolved rows out, so a
 * resolved segment simply stops appearing here — there is no client-side
 * "hide it" logic to get wrong, and no reopen affordance either, since
 * nothing in this slice's brief asks for one.
 *
 * Reordering swaps two adjacent rows' `position` in one gesture — two CAS
 * patches, one per row — same discipline `LinksCard`'s own `moveLink`
 * follows, and for the same reason: an interleaved edit from another
 * device only ever collides with the two rows actually touched. */
function FogCard({
  projectId,
  fog,
  lastFogWrite,
  onCreateFog,
  onPatchFog,
}: {
  projectId: string;
  fog: FogDTO[] | undefined;
  lastFogWrite: TaskFogResult | null;
  onCreateFog: (projectId: string, question: string, position: number) => void;
  onPatchFog: (
    current: FogDTO,
    patch: { question?: string; position?: number; resolvedAt?: number | null },
  ) => void;
}) {
  const [questionInput, setQuestionInput] = useState("");
  const [editingId, setEditingId] = useState<string | null>(null);

  // Gated on `projectId`, same reasoning `LinksCard`'s own failure read
  // carries: `lastFogWrite` is one broadcast slot shared by every open
  // dossier, so an unguarded read would paint a stranger's failure into
  // this card.
  const failure =
    lastFogWrite !== null && lastFogWrite.projectId === projectId
      ? (lastFogWrite.kind === "ok" ? null : lastFogWrite.error ?? "That fog write did not go through.")
      : null;

  const sortedFog = fog === undefined ? undefined : [...fog].sort((a, b) => a.position - b.position);

  function addFog() {
    const trimmedQuestion = questionInput.trim();
    if (trimmedQuestion === "") {
      return;
    }
    // Only reachable once the read has answered — Add is disabled while
    // `sortedFog` is `undefined`, because "no open fog yet" and "not known
    // yet" would otherwise both mint position 0, and against a project that
    // does have open fog that 0 duplicates the first row's: the same
    // collision the count-minted position below is written to avoid,
    // arrived at from the other direction.
    // Mint the new position past the largest live one, not from the count —
    // `LinksCard.addLink`'s own reasoning, ported verbatim: resolving drops
    // a row out of this read without renumbering the rest (it is a stamp,
    // not a delete, but `open_fog_for` still stops returning it), so open
    // positions develop gaps (0,1,2 minus the middle leaves 0,2 with length
    // 2) and a count-minted position would duplicate a live row's — turning
    // the next reorder swap into a value-identical no-op patch. Resolved
    // rows cannot count toward the max (the mirror filters them out before
    // this card ever sees them), and they need not: a collision with a
    // resolved row's position is harmless, since only open rows are ever
    // sorted or swapped.
    const position = sortedFog === undefined || sortedFog.length === 0 ? 0 : sortedFog[sortedFog.length - 1].position + 1;
    onCreateFog(projectId, trimmedQuestion, position);
    setQuestionInput("");
  }

  function moveFog(index: number, direction: -1 | 1) {
    if (sortedFog === undefined) {
      return;
    }
    const swapIndex = index + direction;
    if (swapIndex < 0 || swapIndex >= sortedFog.length) {
      return;
    }
    const segment = sortedFog[index];
    const other = sortedFog[swapIndex];
    onPatchFog(segment, { position: other.position });
    onPatchFog(other, { position: segment.position });
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
      <span className="hb-meta">fog</span>
      <Card padding="var(--space-5)" style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
        {sortedFog === undefined ? (
          <span style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>Reading fog…</span>
        ) : sortedFog.length === 0 ? (
          <span style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
            No open fog on this Route.
          </span>
        ) : (
          <ul style={{ listStyle: "none", margin: 0, padding: 0, display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
            {sortedFog.map((segment, index) =>
              editingId === segment.id ? (
                <FogEditRow
                  key={segment.id}
                  fog={segment}
                  onSave={(question) => {
                    onPatchFog(segment, { question });
                    setEditingId(null);
                  }}
                  onCancel={() => setEditingId(null)}
                />
              ) : (
                <li key={segment.id} style={{ display: "flex", alignItems: "center", gap: "var(--space-2)" }}>
                  <span
                    style={{
                      flex: 1,
                      minWidth: 0,
                      font: "var(--type-body-sm)",
                      color: "var(--text-primary)",
                    }}
                  >
                    {segment.question}
                  </span>
                  <IconButton
                    icon="chevron-down"
                    label="Move up"
                    size="sm"
                    style={{ transform: "rotate(180deg)" }}
                    disabled={index === 0}
                    onClick={() => moveFog(index, -1)}
                  />
                  <IconButton
                    icon="chevron-down"
                    label="Move down"
                    size="sm"
                    disabled={index === sortedFog.length - 1}
                    onClick={() => moveFog(index, 1)}
                  />
                  <Button variant="ghost" size="sm" onClick={() => setEditingId(segment.id)}>
                    Edit
                  </Button>
                  <IconButton
                    icon="check"
                    label="Resolve"
                    size="sm"
                    onClick={() => onPatchFog(segment, { resolvedAt: Date.now() })}
                  />
                </li>
              ),
            )}
          </ul>
        )}

        <form
          onSubmit={(event) => {
            event.preventDefault();
            addFog();
          }}
          style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}
        >
          <Input
            label="Open question"
            placeholder="What blocks this from being an action?"
            value={questionInput}
            onChange={(event) => setQuestionInput(event.target.value)}
          />
          {failure !== null ? <Badge tone="danger">{failure}</Badge> : null}
          <Button type="submit" size="sm" disabled={questionInput.trim() === "" || sortedFog === undefined}>
            Add fog
          </Button>
        </form>
      </Card>
    </div>
  );
}

/** One fog segment's inline reword form — swapped in for its display row
 * (`FogCard`'s own `editingId`), the same "click to reveal the fields"
 * shape `LinkEditRow` uses. */
function FogEditRow({
  fog,
  onSave,
  onCancel,
}: {
  fog: FogDTO;
  onSave: (question: string) => void;
  onCancel: () => void;
}) {
  const [questionInput, setQuestionInput] = useState(fog.question);

  function save() {
    const trimmedQuestion = questionInput.trim();
    if (trimmedQuestion === "") {
      return;
    }
    onSave(trimmedQuestion);
  }

  return (
    <li style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
      <Input label="Open question" value={questionInput} onChange={(event) => setQuestionInput(event.target.value)} />
      <div style={{ display: "flex", gap: "var(--space-3)" }}>
        <Button size="sm" onClick={save} disabled={questionInput.trim() === ""}>
          Save
        </Button>
        <Button variant="ghost" size="sm" onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </li>
  );
}
