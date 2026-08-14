// PROTOTYPE (#449) — throwaway. Variant B: gallery → dossier. Two levels:
// a card grid of the whole portfolio, then a full-page drill-in per project
// using the sanctioned TwoColumn/Column/Aside skeleton — actions and fog in
// the reading column, properties/links/steps/archive in the aside. Bets that
// one project at a time deserves the whole screen, the way the old
// RoutesScreen framed it.

import { useState } from "react";
import { Badge } from "../../components/core/Badge";
import { Button } from "../../components/core/Button";
import { Card } from "../../components/core/Card";
import { IconButton } from "../../components/core/IconButton";
import { ItemRow } from "../../components/domain/ItemRow";
import { Switch } from "../../components/forms/Switch";
import { Aside, Column, TwoColumn } from "../layout";
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

export function VariantB(props: VariantProps) {
  const open = props.projects.find((project) => project.id === props.selectedId);
  return open ? <Dossier {...props} project={open} /> : <Gallery {...props} />;
}

function Gallery({ projects, api, onSelect }: VariantProps) {
  const [showArchived, setShowArchived] = useState(false);
  const [creating, setCreating] = useState(false);
  const visible = projects.filter(
    (project) => showArchived || project.archivedAt === null,
  );
  const archivedCount = projects.filter((project) => project.archivedAt !== null).length;

  function create(name: string) {
    setCreating(true);
    void api.createProject(name).then((id) => {
      setCreating(false);
      onSelect(id);
    });
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-6)" }}>
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
        <span className="hb-meta">
          {projects.length - archivedCount} live · {archivedCount} archived
        </span>
        <Switch
          checked={showArchived}
          onChange={() => setShowArchived((current) => !current)}
          label="Show archived"
        />
      </div>
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(auto-fill, minmax(240px, 1fr))",
          gap: "var(--space-6)",
        }}
      >
        {visible.map((project) => (
          <Card
            key={project.id}
            interactive
            onClick={() => onSelect(project.id)}
            style={{
              display: "flex",
              flexDirection: "column",
              gap: "var(--space-4)",
              opacity: project.archivedAt !== null ? 0.6 : 1,
            }}
          >
            <div style={{ display: "flex", alignItems: "baseline", gap: "var(--space-4)" }}>
              <h3 style={{ font: "var(--type-h3)", color: "var(--text-primary)", flex: 1 }}>
                {project.name}
              </h3>
              {project.archivedAt !== null ? <Badge mono>archived</Badge> : null}
            </div>
            <p
              style={{
                font: "var(--type-body-sm)",
                color: "var(--text-secondary)",
                display: "-webkit-box",
                WebkitLineClamp: 2,
                WebkitBoxOrient: "vertical",
                overflow: "hidden",
                minHeight: "2.6em",
              }}
            >
              {project.route.destination || "No destination yet."}
            </p>
            <div style={{ display: "flex", alignItems: "center", gap: "var(--space-4)" }}>
              <span className="hb-meta" style={{ flex: "0 0 auto" }}>
                {countsMeta(project)}
              </span>
              {project.githubRepo ? (
                <Badge icon="link" mono style={{ minWidth: 0, overflow: "hidden" }}>
                  <span
                    style={{
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                    }}
                  >
                    {project.githubRepo}
                  </span>
                </Badge>
              ) : null}
            </div>
          </Card>
        ))}
        <Card
          padding="var(--space-6)"
          style={{
            borderStyle: "dashed",
            display: "flex",
            flexDirection: "column",
            gap: "var(--space-4)",
            justifyContent: "center",
            boxShadow: "none",
          }}
        >
          <span style={{ font: "var(--type-body-strong)", color: "var(--text-primary)" }}>
            New project
          </span>
          <AddRow placeholder="Name" buttonLabel="Create" onAdd={create} />
          {creating ? (
            <span className="hb-meta">creating — appears when the round trip lands</span>
          ) : null}
        </Card>
      </div>
    </div>
  );
}

function Dossier({
  project,
  api,
  onSelect,
}: VariantProps & { project: ProtoProject }) {
  const [confirming, setConfirming] = useState(false);
  const live = liveActions(project);
  const [asideActionId, setAsideActionId] = useState<string | null>(live[0]?.id ?? null);
  const asideAction =
    live.find((action) => action.id === asideActionId) ?? live[0] ?? null;

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-6)" }}>
      <div style={{ display: "flex", alignItems: "center", gap: "var(--space-5)" }}>
        <Button variant="ghost" size="sm" iconLeft="arrow-left" onClick={() => onSelect(null)}>
          All projects
        </Button>
        <span className="hb-meta">{countsMeta(project)}</span>
        {project.archivedAt !== null ? <Badge mono>archived</Badge> : null}
      </div>

      <div>
        <InlineText
          value={project.name}
          onCommit={(name) => api.patchProject(project.id, { name })}
          font="var(--type-h1)"
          label="project name"
        />
      </div>

      <TwoColumn>
        <Column>
          <div>
            <span className="hb-meta">route · destination</span>
            <div style={{ marginTop: "var(--space-4)" }}>
              <InlineText
                value={project.route.destination}
                onCommit={(destination) => api.setRoute(project.id, { destination })}
                font="var(--type-h3)"
                placeholder="Where is this going?"
                label="destination"
              />
            </div>
            <div style={{ marginTop: "var(--space-3)", maxWidth: 560 }}>
              <InlineText
                value={project.route.notes}
                onCommit={(notes) => api.setRoute(project.id, { notes })}
                font="var(--type-body)"
                color="var(--text-secondary)"
                placeholder="Route notes"
                allowEmpty
                label="route notes"
              />
            </div>
          </div>

          <ProtoSection title="Actions" meta="click a row for its steps">
            <Card padding="var(--space-3)">
              {live.map((action, index) => (
                <div
                  key={action.id}
                  style={{ display: "flex", alignItems: "center", gap: "var(--space-2)" }}
                >
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <ItemRow
                      title={action.title}
                      stage={action.stage}
                      steps={
                        action.steps.length > 0
                          ? `${action.steps.filter((step) => step.done).length}/${action.steps.length}`
                          : undefined
                      }
                      onClick={() => setAsideActionId(action.id)}
                      style={
                        asideAction?.id === action.id
                          ? { background: "var(--surface-quiet)", borderRadius: "var(--radius-md)" }
                          : undefined
                      }
                    />
                  </div>
                  <div style={{ display: "flex", flexDirection: "column" }}>
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
                  </div>
                </div>
              ))}
              {live.length === 0 ? (
                <p
                  style={{
                    font: "var(--type-body-sm)",
                    color: "var(--text-secondary)",
                    padding: "var(--space-4)",
                  }}
                >
                  No live actions.
                </p>
              ) : null}
            </Card>
          </ProtoSection>

          <ProtoSection
            title="Fog"
            meta={`${project.fog.filter((entry) => entry.resolvedAt === null).length} open`}
          >
            <Card style={{ display: "flex", flexDirection: "column", gap: "var(--space-5)" }}>
              {project.fog.map((entry) => (
                <FogEntry
                  key={entry.id}
                  entry={entry}
                  onEdit={(question) => api.editFog(project.id, entry.id, question)}
                  onResolve={(resolved) => api.resolveFog(project.id, entry.id, resolved)}
                />
              ))}
              <AddRow
                placeholder="What is still unclear?"
                onAdd={(question) => api.addFog(project.id, question)}
              />
            </Card>
          </ProtoSection>
        </Column>

        <Aside label="Project properties">
          <span className="hb-meta">properties</span>
          <Card style={{ display: "flex", flexDirection: "column", gap: "var(--space-5)" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
              <span className="hb-meta">github</span>
              <RepoField
                repo={project.githubRepo}
                onCommit={(githubRepo) => api.patchProject(project.id, { githubRepo })}
              />
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
              <span className="hb-meta">default context — copied at mint, never joined</span>
              <ContextSelect
                value={project.defaultContext}
                onChange={(defaultContext) => api.patchProject(project.id, { defaultContext })}
              />
            </div>
          </Card>

          <Card style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
            <span className="hb-meta">links</span>
            <LinksEditor
              links={project.links}
              onAdd={(url, label) => api.addLink(project.id, url, label)}
              onRemove={(linkId) => api.removeLink(project.id, linkId)}
            />
          </Card>

          {asideAction ? (
            <Card style={{ display: "flex", flexDirection: "column", gap: "var(--space-5)" }}>
              <span className="hb-meta">steps · {asideAction.title}</span>
              <StepsChecklist
                steps={asideAction.steps}
                onTick={(stepId) => api.tickStep(project.id, asideAction.id, stepId)}
                onAdd={(body) => api.addStep(project.id, asideAction.id, body)}
                onEdit={(stepId, body) => api.editStep(project.id, asideAction.id, stepId, body)}
                onDelete={(stepId) => api.deleteStep(project.id, asideAction.id, stepId)}
              />
            </Card>
          ) : null}

          <Card style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
            <span className="hb-meta">archive</span>
            <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
              Archiving takes this project and its live items down together; unarchiving restores
              exactly what it took.
            </p>
            {project.archivedAt !== null ? (
              <Button
                variant="quiet"
                size="sm"
                iconLeft="archive-restore"
                onClick={() => api.unarchiveProject(project.id)}
              >
                Unarchive
              </Button>
            ) : (
              <Button variant="ghost" size="sm" iconLeft="archive" onClick={() => setConfirming(true)}>
                Archive project
              </Button>
            )}
          </Card>
        </Aside>
      </TwoColumn>

      {confirming ? (
        <ArchiveDialog
          project={project}
          onCancel={() => setConfirming(false)}
          onConfirm={() => {
            api.archiveProject(project.id);
            setConfirming(false);
          }}
        />
      ) : null}
    </div>
  );
}
