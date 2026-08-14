// PROTOTYPE (#449) — throwaway. Variant A: master–detail ledger. The project
// list is always on screen (an email-client posture); the detail pane holds
// everything about the selected project in one dense scroll. Bets that the
// portfolio is small enough that switching projects constantly is the real
// usage pattern.

import { useState } from "react";
import { Badge } from "../../components/core/Badge";
import { Button } from "../../components/core/Button";
import { Card } from "../../components/core/Card";
import { Icon } from "../../components/core/Icon";
import { IconButton } from "../../components/core/IconButton";
import { StageBadge } from "../../components/domain/StageBadge";
import { EmptyState } from "../../components/feedback/EmptyState";
import { Switch } from "../../components/forms/Switch";
import {
  AddRow,
  ArchiveDialog,
  ContextSelect,
  FogEntry,
  InlineText,
  LinksEditor,
  ProtoSection,
  RepoField,
  StepsChecklist,
} from "./bits";
import { countsMeta, liveActions } from "./fixture";
import type { ProtoProject } from "./fixture";
import type { VariantProps } from "./world";

export function VariantA({ projects, api, selectedId, onSelect }: VariantProps) {
  const [showArchived, setShowArchived] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [creating, setCreating] = useState(false);
  const [openSteps, setOpenSteps] = useState<string | null>(null);

  const live = projects.filter((project) => project.archivedAt === null);
  const archived = projects.filter((project) => project.archivedAt !== null);
  const selected =
    projects.find((project) => project.id === selectedId) ?? live[0] ?? null;

  function create(name: string) {
    setCreating(true);
    void api.createProject(name).then((id) => {
      setCreating(false);
      onSelect(id);
    });
  }

  function listRow(project: ProtoProject) {
    const isSelected = selected?.id === project.id;
    const isArchived = project.archivedAt !== null;
    return (
      <button
        key={project.id}
        type="button"
        onClick={() => onSelect(project.id)}
        style={{
          display: "flex",
          flexDirection: "column",
          alignItems: "stretch",
          gap: 2,
          textAlign: "left",
          padding: "var(--space-4) var(--space-5)",
          border: "none",
          borderRadius: "var(--radius-md)",
          background: isSelected ? "var(--accent-quiet)" : "transparent",
          cursor: "pointer",
          opacity: isArchived ? 0.55 : 1,
        }}
      >
        <span
          style={{
            font: "var(--type-body-strong)",
            color: isSelected ? "var(--text-brand)" : "var(--text-primary)",
          }}
        >
          {project.name}
        </span>
        <span className="hb-meta">{isArchived ? "archived" : countsMeta(project)}</span>
      </button>
    );
  }

  return (
    <div
      style={{
        display: "grid",
        gridTemplateColumns: "280px minmax(0, 1fr)",
        gap: "var(--space-8)",
        alignItems: "start",
      }}
    >
      <Card padding="var(--space-4)" style={{ position: "sticky", top: 0 }}>
        <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
          <div style={{ padding: "var(--space-2) var(--space-5) var(--space-4)" }}>
            <AddRow placeholder="New project" buttonLabel="Create" onAdd={create} />
            {creating ? (
              <span className="hb-meta" style={{ display: "block", marginTop: "var(--space-3)" }}>
                creating — appears when the round trip lands
              </span>
            ) : null}
          </div>
          {live.map(listRow)}
          {live.length === 0 ? (
            <p
              style={{
                font: "var(--type-body-sm)",
                color: "var(--text-secondary)",
                padding: "0 var(--space-5)",
              }}
            >
              No live projects.
            </p>
          ) : null}
          <div
            style={{
              borderTop: "1px solid var(--border-subtle)",
              marginTop: "var(--space-3)",
              paddingTop: "var(--space-4)",
              paddingLeft: "var(--space-5)",
              paddingRight: "var(--space-5)",
            }}
          >
            <Switch
              checked={showArchived}
              onChange={() => setShowArchived((current) => !current)}
              label={`Archived (${archived.length})`}
            />
          </div>
          {showArchived ? archived.map(listRow) : null}
        </div>
      </Card>

      {selected === null ? (
        <Card padding="var(--space-3)">
          <EmptyState
            icon="route"
            headingLevel={2}
            title="No projects yet"
            body="A project holds a Route — its Destination, its Fog — and the actions minted toward it."
          />
        </Card>
      ) : (
        <div
          key={selected.id}
          style={{ display: "flex", flexDirection: "column", gap: "var(--space-8)", minWidth: 0 }}
        >
          <div>
            <div style={{ display: "flex", alignItems: "flex-start", gap: "var(--space-5)" }}>
              <div style={{ flex: 1, minWidth: 0 }}>
                <InlineText
                  value={selected.name}
                  onCommit={(name) => api.patchProject(selected.id, { name })}
                  font="var(--type-h2)"
                  label="project name"
                />
              </div>
              {selected.archivedAt !== null ? (
                <Button
                  variant="quiet"
                  size="sm"
                  iconLeft="archive-restore"
                  onClick={() => api.unarchiveProject(selected.id)}
                >
                  Unarchive
                </Button>
              ) : (
                <Button
                  variant="ghost"
                  size="sm"
                  iconLeft="archive"
                  onClick={() => setConfirming(true)}
                >
                  Archive
                </Button>
              )}
            </div>
            <div
              style={{
                display: "flex",
                alignItems: "center",
                gap: "var(--space-6)",
                marginTop: "var(--space-4)",
                flexWrap: "wrap",
              }}
            >
              {selected.archivedAt !== null ? <Badge mono>archived</Badge> : null}
              <RepoField
                repo={selected.githubRepo}
                onCommit={(githubRepo) => api.patchProject(selected.id, { githubRepo })}
              />
              <ContextSelect
                value={selected.defaultContext}
                onChange={(defaultContext) => api.patchProject(selected.id, { defaultContext })}
              />
              <span className="hb-meta">{countsMeta(selected)}</span>
            </div>
          </div>

          <Card accent style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
            <span className="hb-meta">route · destination</span>
            <InlineText
              value={selected.route.destination}
              onCommit={(destination) => api.setRoute(selected.id, { destination })}
              font="var(--type-body-strong)"
              placeholder="Where is this going?"
              label="destination"
            />
            <InlineText
              value={selected.route.notes}
              onCommit={(notes) => api.setRoute(selected.id, { notes })}
              font="var(--type-body-sm)"
              color="var(--text-secondary)"
              placeholder="Route notes"
              allowEmpty
              label="route notes"
            />
          </Card>

          <ProtoSection title="Actions" meta="ordered · project_pos">
            <Card padding="var(--space-3)">
              <div style={{ display: "flex", flexDirection: "column" }}>
                {liveActions(selected).map((action, index, all) => (
                  <div
                    key={action.id}
                    style={{ borderTop: index > 0 ? "1px solid var(--border-subtle)" : "none" }}
                  >
                    <div
                      style={{
                        display: "flex",
                        alignItems: "center",
                        gap: "var(--space-4)",
                        padding: "var(--space-3) var(--space-4)",
                        minHeight: 44,
                      }}
                    >
                      <div style={{ display: "flex", flexDirection: "column" }}>
                        <IconButton
                          icon="arrow-up"
                          label={`Move ${action.title} up`}
                          size="sm"
                          disabled={index === 0}
                          onClick={() => api.moveAction(selected.id, action.id, -1)}
                        />
                        <IconButton
                          icon="arrow-down"
                          label={`Move ${action.title} down`}
                          size="sm"
                          disabled={index === all.length - 1}
                          onClick={() => api.moveAction(selected.id, action.id, 1)}
                        />
                      </div>
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
                      <div
                        style={{
                          padding: "var(--space-2) var(--space-6) var(--space-5) 56px",
                        }}
                      >
                        <StepsChecklist
                          steps={action.steps}
                          onTick={(stepId) => api.tickStep(selected.id, action.id, stepId)}
                          onAdd={(body) => api.addStep(selected.id, action.id, body)}
                          onEdit={(stepId, body) => api.editStep(selected.id, action.id, stepId, body)}
                          onDelete={(stepId) => api.deleteStep(selected.id, action.id, stepId)}
                        />
                      </div>
                    ) : null}
                  </div>
                ))}
                {liveActions(selected).length === 0 ? (
                  <p
                    style={{
                      font: "var(--type-body-sm)",
                      color: "var(--text-secondary)",
                      padding: "var(--space-4)",
                    }}
                  >
                    No live actions. /to-actions mints them; this page reorders them.
                  </p>
                ) : null}
              </div>
            </Card>
          </ProtoSection>

          <ProtoSection title="Fog" meta={`${selected.fog.filter((f) => f.resolvedAt === null).length} open`}>
            <Card style={{ display: "flex", flexDirection: "column", gap: "var(--space-5)" }}>
              {selected.fog.map((entry) => (
                <FogEntry
                  key={entry.id}
                  entry={entry}
                  onEdit={(question) => api.editFog(selected.id, entry.id, question)}
                  onResolve={(resolved) => api.resolveFog(selected.id, entry.id, resolved)}
                />
              ))}
              {selected.fog.length === 0 ? (
                <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
                  No fog. Nothing is unclear about this project — or nothing has been asked yet.
                </p>
              ) : null}
              <AddRow placeholder="What is still unclear?" onAdd={(q) => api.addFog(selected.id, q)} />
            </Card>
          </ProtoSection>

          <ProtoSection title="Links">
            <Card>
              <LinksEditor
                links={selected.links}
                onAdd={(url, label) => api.addLink(selected.id, url, label)}
                onRemove={(linkId) => api.removeLink(selected.id, linkId)}
              />
            </Card>
          </ProtoSection>

          <span className="hb-meta" style={{ display: "flex", alignItems: "center", gap: "var(--space-3)" }}>
            <Icon name="route" size={13} /> {selected.id}
          </span>
        </div>
      )}

      {confirming && selected ? (
        <ArchiveDialog
          project={selected}
          onCancel={() => setConfirming(false)}
          onConfirm={() => {
            api.archiveProject(selected.id);
            setConfirming(false);
          }}
        />
      ) : null}
    </div>
  );
}
