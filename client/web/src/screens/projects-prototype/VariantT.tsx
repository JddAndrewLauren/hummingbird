// PROTOTYPE (#449) — throwaway. Variant T: not a Projects-page layout but
// the one flow #449 adds elsewhere — triage's project <Select> gaining
// "+ New project", with the settled v1 posture (create round-trips before
// the picker updates; no optimistic projection) and copy-at-mint filling a
// context-less draft from the project's default_context.

import { useState } from "react";
import { Button } from "../../components/core/Button";
import { Card } from "../../components/core/Card";
import { Select } from "../../components/forms/Select";
import { AddRow } from "./bits";
import { CONTEXTS } from "./fixture";
import type { VariantProps } from "./world";

const NEW_SENTINEL = "__new-project__";

export function VariantT({ projects, api }: VariantProps) {
  const [projectId, setProjectId] = useState("");
  const [context, setContext] = useState("");
  const [copied, setCopied] = useState(false);
  const [namingNew, setNamingNew] = useState(false);
  const [pendingName, setPendingName] = useState<string | null>(null);

  const live = projects.filter((project) => project.archivedAt === null);

  function pick(value: string) {
    if (value === NEW_SENTINEL) {
      setNamingNew(true);
      return;
    }
    setProjectId(value);
    // copy-at-mint: only when the draft has no context of its own, and the
    // copy is visible and editable, never a hidden join.
    const project = live.find((candidate) => candidate.id === value);
    if (project?.defaultContext && !context) {
      setContext(project.defaultContext);
      setCopied(true);
    } else {
      setCopied(false);
    }
  }

  function create(name: string) {
    setPendingName(name);
    void api.createProject(name).then((id) => {
      setPendingName(null);
      setNamingNew(false);
      setProjectId(id);
      setCopied(false);
    });
  }

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        gap: "var(--space-6)",
        maxWidth: 640,
      }}
    >
      <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
        A mock of one triage row, not a page layout — the project picker grows "+ New project",
        and picking a project copies its default context into a context-less draft.
      </p>

      <Card style={{ display: "flex", flexDirection: "column", gap: "var(--space-5)" }}>
        <span className="hb-meta">triage · unsorted capture</span>
        <div>
          <p style={{ font: "var(--type-body-strong)", color: "var(--text-primary)" }}>
            Order more potting mix
          </p>
          <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)", marginTop: 2 }}>
            Captured 2h ago · dictation
          </p>
        </div>

        <div style={{ display: "flex", gap: "var(--space-5)", alignItems: "flex-end", flexWrap: "wrap" }}>
          <div style={{ flex: 1, minWidth: 200 }}>
            {namingNew ? (
              pendingName === null ? (
                <AddRow placeholder="New project name" buttonLabel="Create" onAdd={create} />
              ) : (
                <Button variant="secondary" size="sm" loading fullWidth>
                  Creating {pendingName}
                </Button>
              )
            ) : (
              <Select
                label="Project"
                size="sm"
                value={projectId}
                onChange={(event) => pick(event.target.value)}
                options={[
                  { value: "", label: "no project" },
                  ...live.map((project) => ({ value: project.id, label: project.name })),
                  { value: NEW_SENTINEL, label: "+ New project" },
                ]}
              />
            )}
          </div>
          <div style={{ flex: 1, minWidth: 160 }}>
            <Select
              label="Context"
              size="sm"
              value={context}
              onChange={(event) => {
                setContext(event.target.value);
                setCopied(false);
              }}
              options={[{ value: "", label: "no context" }, ...CONTEXTS]}
            />
          </div>
          <Button variant="primary" size="sm" iconLeft="sparkles" disabled>
            Promote
          </Button>
        </div>

        {copied ? (
          <span className="hb-meta">context copied from the project's default — edit or clear it freely</span>
        ) : null}
        {namingNew && pendingName === null ? (
          <span className="hb-meta">v1 round-trips — the picker updates when the create lands</span>
        ) : null}
      </Card>

      <Card style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
        <span className="hb-meta">world state — proves the create landed</span>
        {live.map((project) => (
          <div
            key={project.id}
            style={{ display: "flex", gap: "var(--space-5)", alignItems: "baseline" }}
          >
            <span style={{ font: "var(--type-body-sm)", color: "var(--text-primary)", flex: 1 }}>
              {project.name}
            </span>
            <span className="hb-meta">
              {project.defaultContext ? `default ${project.defaultContext}` : "no default context"}
            </span>
          </div>
        ))}
      </Card>

    </div>
  );
}
