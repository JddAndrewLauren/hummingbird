import { useState } from "react";
import { Badge } from "../components/core/Badge";
import { Button } from "../components/core/Button";
import { Card } from "../components/core/Card";
import { EmptyState } from "../components/feedback/EmptyState";
import { Input } from "../components/forms/Input";
import { Switch } from "../components/forms/Switch";
import type { ProjectDTO } from "../store/protocol";
import type { TaskProjectResult, TaskState } from "../store/store";
import { Aside, Column, TwoColumn } from "./layout";
import {
  awaitingCreate,
  countsMeta,
  githubRepoUrl,
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
// **The dossier is a shell.** This slice ships the frame, the name, the back
// affordance and labelled empty regions naming what fills them; route
// destination and notes are #627's, fog #628's, the action list and its
// inline steps #629's, properties #625's, links #626's, archive #630's. Each
// placeholder says what is coming, so an operator meets an unbuilt region
// rather than a broken one.

const WAITING_COPY = "creating — appears when the round trip lands";

export interface ProjectsScreenProps {
  task: TaskState;
  onCreateProject: (name: string) => void;
  /** #625: the dossier's properties card write — `patch` carries only the
   * fields the card actually changed. */
  onPatchProject: (
    current: ProjectDTO,
    patch: { githubRepo?: string | null; defaultContext?: string | null },
  ) => void;
}

export function ProjectsScreen({ task, onCreateProject, onPatchProject }: ProjectsScreenProps) {
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

/** One region of the dossier that a later slice fills. Named and labelled so
 * the operator meets an unbuilt region rather than a broken one. */
function ComingRegion({ label, body }: { label: string; body: string }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
      <span className="hb-meta">{label}</span>
      <Card padding="var(--space-5)">
        <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>{body}</p>
      </Card>
    </div>
  );
}

function Dossier({
  row,
  lastProjectWrite,
  onBack,
  onPatchProject,
}: {
  row: ProjectRow;
  lastProjectWrite: TaskProjectResult | null;
  onBack: () => void;
  onPatchProject: (
    current: ProjectDTO,
    patch: { githubRepo?: string | null; defaultContext?: string | null },
  ) => void;
}) {
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
          <ComingRegion
            label="route · destination"
            body="The Route's destination and notes land here."
          />
          <ComingRegion
            label="actions"
            body="This project's ordered actions land here, each expanding to its own steps."
          />
          <ComingRegion label="fog" body="The open questions on this Route land here." />
        </Column>
        <Aside label="Project properties">
          <PropertiesCard
            project={row.project}
            lastProjectWrite={lastProjectWrite}
            onPatchProject={onPatchProject}
          />
          <ComingRegion label="links" body="This project's links land here." />
          <ComingRegion label="archive" body="Archiving this project lands here." />
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
 * stale value already on screen. */
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
  const [syncedVersion, setSyncedVersion] = useState(project.version);

  if (project.version !== syncedVersion) {
    setSyncedVersion(project.version);
    setRepoInput(project.githubRepo ?? "");
    setContextInput(project.defaultContext ?? "");
  }

  const trimmedRepo = repoInput.trim();
  const trimmedContext = contextInput.trim();
  const repoChanged = trimmedRepo !== (project.githubRepo ?? "");
  const contextChanged = trimmedContext !== (project.defaultContext ?? "");
  const dirty = repoChanged || contextChanged;
  const link = githubRepoUrl(project.githubRepo);
  const failure = writeFailureMessage(lastProjectWrite);

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
          hint={link ?? undefined}
        />
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
