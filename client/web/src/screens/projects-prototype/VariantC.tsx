// PROTOTYPE (#449) — throwaway. Variant C: the outline. No detail view at
// all — every project is an expandable row in one column, and route, fog,
// actions, steps, links and properties all edit inline under it. Several can
// be open at once. Bets that the portfolio IS the page: scanning and nudging
// many projects matters more than dwelling in one.

import { useState } from "react";
import { Badge } from "../../components/core/Badge";
import { Button } from "../../components/core/Button";
import { Card } from "../../components/core/Card";
import { Icon } from "../../components/core/Icon";
import { IconButton } from "../../components/core/IconButton";
import { StageBadge } from "../../components/domain/StageBadge";
import { Switch } from "../../components/forms/Switch";
import {
  AddRow,
  ArchiveDialog,
  ContextSelect,
  FogEntry,
  InlineText,
  LinksEditor,
  RepoField,
  StepsChecklist,
} from "./bits";
import { countsMeta, liveActions } from "./fixture";
import type { ProtoProject } from "./fixture";
import type { VariantProps } from "./world";

export function VariantC({ projects, api, selectedId, onSelect }: VariantProps) {
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(
    () => new Set(selectedId ? [selectedId] : []),
  );
  const [showArchived, setShowArchived] = useState(false);
  const [confirmingId, setConfirmingId] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);

  const visible = projects.filter(
    (project) => showArchived || project.archivedAt === null,
  );
  const archivedCount = projects.filter((project) => project.archivedAt !== null).length;
  const confirming = projects.find((project) => project.id === confirmingId) ?? null;

  function toggle(id: string) {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
        onSelect(id);
      }
      return next;
    });
  }

  function create(name: string) {
    setCreating(true);
    void api.createProject(name).then((id) => {
      setCreating(false);
      setExpanded((current) => new Set(current).add(id));
      onSelect(id);
    });
  }

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        gap: "var(--space-5)",
        maxWidth: "var(--content-max)",
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: "var(--space-6)" }}>
        <div style={{ flex: 1 }}>
          <AddRow placeholder="New project" buttonLabel="Create" onAdd={create} />
        </div>
        <Switch
          checked={showArchived}
          onChange={() => setShowArchived((current) => !current)}
          label={`Archived (${archivedCount})`}
        />
      </div>
      {creating ? (
        <span className="hb-meta">creating — appears when the round trip lands</span>
      ) : null}

      {visible.map((project) => (
        <OutlineRow
          key={project.id}
          project={project}
          api={api}
          open={expanded.has(project.id)}
          onToggle={() => toggle(project.id)}
          onArchive={() => setConfirmingId(project.id)}
        />
      ))}

      {confirming ? (
        <ArchiveDialog
          project={confirming}
          onCancel={() => setConfirmingId(null)}
          onConfirm={() => {
            api.archiveProject(confirming.id);
            setConfirmingId(null);
          }}
        />
      ) : null}
    </div>
  );
}

function OutlineRow({
  project,
  api,
  open,
  onToggle,
  onArchive,
}: {
  project: ProtoProject;
  api: VariantProps["api"];
  open: boolean;
  onToggle: () => void;
  onArchive: () => void;
}) {
  const [openSteps, setOpenSteps] = useState<string | null>(null);
  const isArchived = project.archivedAt !== null;
  const live = liveActions(project);

  return (
    <Card padding="0" style={{ opacity: isArchived && !open ? 0.6 : 1 }}>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: "var(--space-4)",
          padding: "var(--space-4) var(--space-5)",
          minHeight: 44,
        }}
      >
        <button
          type="button"
          onClick={onToggle}
          aria-expanded={open}
          aria-label={`${open ? "Collapse" : "Expand"} ${project.name}`}
          style={{
            display: "flex",
            alignItems: "center",
            gap: "var(--space-4)",
            flex: 1,
            minWidth: 0,
            background: "none",
            border: "none",
            padding: 0,
            cursor: "pointer",
            textAlign: "left",
          }}
        >
          <Icon
            name="chevron-right"
            size={16}
            color="var(--text-secondary)"
            style={{
              transform: open ? "rotate(90deg)" : "none",
              transition: "transform 120ms var(--ease-flit)",
              flex: "0 0 auto",
            }}
          />
          <span style={{ font: "var(--type-body-strong)", color: "var(--text-primary)" }}>
            {project.name}
          </span>
          <span
            style={{
              font: "var(--type-body-sm)",
              color: "var(--text-secondary)",
              flex: 1,
              minWidth: 0,
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
            }}
          >
            {project.route.destination}
          </span>
        </button>
        {isArchived ? <Badge mono>archived</Badge> : null}
        <span className="hb-meta" style={{ flex: "0 0 auto" }}>
          {countsMeta(project)}
        </span>
        {isArchived ? (
          <IconButton
            icon="archive-restore"
            label={`Unarchive ${project.name}`}
            size="sm"
            onClick={() => api.unarchiveProject(project.id)}
          />
        ) : (
          <IconButton icon="archive" label={`Archive ${project.name}`} size="sm" onClick={onArchive} />
        )}
      </div>

      {open ? (
        <div
          style={{
            borderTop: "1px solid var(--border-subtle)",
            padding: "var(--space-5) var(--space-6) var(--space-6) 44px",
            display: "flex",
            flexDirection: "column",
            gap: "var(--space-6)",
          }}
        >
          <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
            <span className="hb-meta">route</span>
            <InlineText
              value={project.route.destination}
              onCommit={(destination) => api.setRoute(project.id, { destination })}
              font="var(--type-body-strong)"
              placeholder="Where is this going?"
              label={`${project.name} destination`}
            />
            <InlineText
              value={project.route.notes}
              onCommit={(notes) => api.setRoute(project.id, { notes })}
              font="var(--type-body-sm)"
              color="var(--text-secondary)"
              placeholder="Route notes"
              allowEmpty
              label={`${project.name} route notes`}
            />
          </div>

          <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
            <span className="hb-meta">fog</span>
            {project.fog.map((entry) => (
              <FogEntry
                key={entry.id}
                entry={entry}
                onEdit={(question) => api.editFog(project.id, entry.id, question)}
                onResolve={(resolved) => api.resolveFog(project.id, entry.id, resolved)}
              />
            ))}
            <AddRow placeholder="What is still unclear?" onAdd={(q) => api.addFog(project.id, q)} />
          </div>

          <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
            <span className="hb-meta">actions</span>
            {live.map((action, index) => (
              <div key={action.id}>
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: "var(--space-4)",
                    minHeight: 36,
                  }}
                >
                  <Icon name="grip-vertical" size={14} color="var(--text-secondary)" />
                  <StageBadge stage={action.stage} compact />
                  <span
                    style={{
                      font: "var(--type-body)",
                      color: "var(--text-primary)",
                      flex: 1,
                      minWidth: 0,
                    }}
                  >
                    {action.title}
                  </span>
                  {action.context ? <span className="hb-meta">{action.context}</span> : null}
                  <IconButton
                    icon="arrow-up"
                    label={`Move ${action.title} up`}
                    size="sm"
                    disabled={index === 0}
                    onClick={() => api.moveAction(project.id, action.id, -1)}
                  />
                  <IconButton
                    icon="arrow-down"
                    label={`Move ${action.title} down`}
                    size="sm"
                    disabled={index === live.length - 1}
                    onClick={() => api.moveAction(project.id, action.id, 1)}
                  />
                  <Button
                    variant="ghost"
                    size="sm"
                    iconLeft="list-checks"
                    onClick={() =>
                      setOpenSteps((current) => (current === action.id ? null : action.id))
                    }
                  >
                    {action.steps.filter((step) => step.done).length}/{action.steps.length}
                  </Button>
                </div>
                {openSteps === action.id ? (
                  <div style={{ padding: "var(--space-3) 0 var(--space-4) 44px" }}>
                    <StepsChecklist
                      steps={action.steps}
                      onTick={(stepId) => api.tickStep(project.id, action.id, stepId)}
                      onAdd={(body) => api.addStep(project.id, action.id, body)}
                      onEdit={(stepId, body) => api.editStep(project.id, action.id, stepId, body)}
                      onDelete={(stepId) => api.deleteStep(project.id, action.id, stepId)}
                    />
                  </div>
                ) : null}
              </div>
            ))}
            {live.length === 0 ? (
              <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
                No live actions.
              </p>
            ) : null}
          </div>

          <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
            <span className="hb-meta">links</span>
            <LinksEditor
              links={project.links}
              onAdd={(url, label) => api.addLink(project.id, url, label)}
              onRemove={(linkId) => api.removeLink(project.id, linkId)}
            />
          </div>

          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: "var(--space-6)",
              borderTop: "1px solid var(--border-subtle)",
              paddingTop: "var(--space-5)",
              flexWrap: "wrap",
            }}
          >
            <span className="hb-meta">github</span>
            <RepoField
              repo={project.githubRepo}
              onCommit={(githubRepo) => api.patchProject(project.id, { githubRepo })}
            />
            <span className="hb-meta">default context</span>
            <ContextSelect
              value={project.defaultContext}
              onChange={(defaultContext) => api.patchProject(project.id, { defaultContext })}
            />
          </div>
        </div>
      ) : null}
    </Card>
  );
}
