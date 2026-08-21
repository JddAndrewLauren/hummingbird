import { useEffect, useState } from "react";
import { Badge } from "../components/core/Badge";
import { Button } from "../components/core/Button";
import { Card } from "../components/core/Card";
import { IconButton } from "../components/core/IconButton";
import { EmptyState } from "../components/feedback/EmptyState";
import { Input } from "../components/forms/Input";
import { Switch } from "../components/forms/Switch";
import type { ProjectDTO, ProjectLinkDTO } from "../store/protocol";
import type { TaskProjectLinkResult, TaskProjectResult, TaskState } from "../store/store";
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
// inline steps #629's, archive #630's. Properties (#625) and links (#626)
// are both real cards now — each remaining placeholder still says what is
// coming, so an operator meets an unbuilt region rather than a broken one.

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
}

export function ProjectsScreen({
  task,
  onCreateProject,
  onPatchProject,
  onRequestProjectLinks,
  onCreateProjectLink,
  onPatchProjectLink,
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
      links={task.linksByProject[open.project.id]}
      lastProjectLinkWrite={task.lastProjectLinkWrite}
      onRequestProjectLinks={onRequestProjectLinks}
      onCreateProjectLink={onCreateProjectLink}
      onPatchProjectLink={onPatchProjectLink}
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
  links,
  lastProjectLinkWrite,
  onRequestProjectLinks,
  onCreateProjectLink,
  onPatchProjectLink,
  syncOutcomeSeq,
}: {
  row: ProjectRow;
  lastProjectWrite: TaskProjectResult | null;
  onBack: () => void;
  onPatchProject: (
    current: ProjectDTO,
    patch: { githubRepo?: string | null; defaultContext?: string | null },
  ) => void;
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
          <LinksCard
            projectId={projectId}
            links={links}
            lastProjectLinkWrite={lastProjectLinkWrite}
            onCreateProjectLink={onCreateProjectLink}
            onPatchProjectLink={onPatchProjectLink}
          />
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
          <Button type="submit" size="sm" disabled={urlInput.trim() === ""}>
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
