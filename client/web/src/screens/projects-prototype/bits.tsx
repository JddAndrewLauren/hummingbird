// PROTOTYPE (#449) — throwaway. Shared *leaf* editors only — click-to-edit
// text, a steps checklist, links, fog entries, the archive dialog. Layout
// and hierarchy stay per-variant (sharing those would defeat the prototype);
// these are shared because their behaviour is settled by the plan, not up
// for design.

import { useEffect, useRef, useState } from "react";
import type { CSSProperties, KeyboardEvent, ReactNode } from "react";
import { Button } from "../../components/core/Button";
import { Card } from "../../components/core/Card";
import { Icon } from "../../components/core/Icon";
import { IconButton } from "../../components/core/IconButton";
import { Checkbox } from "../../components/forms/Checkbox";
import { Input } from "../../components/forms/Input";
import { Select } from "../../components/forms/Select";
import { CONTEXTS, repoUrl } from "./fixture";
import type { ProtoFog, ProtoLink, ProtoProject, ProtoStep } from "./fixture";

/** Text that turns into an input on click. Enter or blur commits; Escape
 * cancels. An empty commit is ignored unless `allowEmpty`. */
export function InlineText({
  value,
  onCommit,
  font = "var(--type-body)",
  color = "var(--text-primary)",
  placeholder = "—",
  allowEmpty = false,
  label,
}: {
  value: string;
  onCommit: (next: string) => void;
  font?: string;
  color?: string;
  placeholder?: string;
  allowEmpty?: boolean;
  /** Accessible name for the edit affordance. */
  label: string;
}) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(value);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (editing) inputRef.current?.select();
  }, [editing]);

  if (editing) {
    return (
      <input
        ref={inputRef}
        value={draft}
        aria-label={label}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={() => {
          setEditing(false);
          if (draft.trim() || allowEmpty) onCommit(draft.trim());
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter") (event.target as HTMLInputElement).blur();
          if (event.key === "Escape") {
            setDraft(value);
            setEditing(false);
          }
        }}
        style={{
          font,
          color,
          background: "var(--surface-card)",
          border: "1px solid var(--border-default)",
          borderRadius: "var(--radius-sm)",
          padding: "0 var(--space-2)",
          width: "100%",
          boxSizing: "border-box",
        }}
      />
    );
  }

  return (
    <button
      type="button"
      aria-label={`Edit ${label}`}
      onClick={() => {
        setDraft(value);
        setEditing(true);
      }}
      style={{
        font,
        color: value ? color : "var(--text-tertiary, var(--text-secondary))",
        background: "none",
        border: "none",
        padding: 0,
        textAlign: "left",
        cursor: "text",
        display: "inline-flex",
        alignItems: "baseline",
        gap: "var(--space-3)",
        maxWidth: "100%",
      }}
    >
      <span style={{ minWidth: 0 }}>{value || placeholder}</span>
      <Icon name="pencil" size={13} color="var(--text-secondary)" style={{ opacity: 0.55 }} />
    </button>
  );
}

/** One committing text field with an add button — fog questions, steps,
 * links, new projects all take this shape. */
export function AddRow({
  placeholder,
  buttonLabel = "Add",
  onAdd,
  size = "sm",
}: {
  placeholder: string;
  buttonLabel?: string;
  onAdd: (value: string) => void;
  size?: "sm" | "md";
}) {
  const [draft, setDraft] = useState("");
  function commit() {
    if (!draft.trim()) return;
    onAdd(draft.trim());
    setDraft("");
  }
  function onKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "Enter") commit();
  }
  return (
    <div style={{ display: "flex", gap: "var(--space-4)", alignItems: "center" }}>
      <div style={{ flex: 1 }}>
        <Input
          size={size}
          placeholder={placeholder}
          aria-label={placeholder}
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={onKeyDown}
        />
      </div>
      <Button variant="secondary" size={size} iconLeft="plus" onClick={commit} disabled={!draft.trim()}>
        {buttonLabel}
      </Button>
    </div>
  );
}

/** The per-action checklist: tick, add, edit, delete. The pencil swaps the
 * row to an input; Enter or blur commits, Escape cancels. */
export function StepsChecklist({
  steps,
  onTick,
  onAdd,
  onEdit,
  onDelete,
}: {
  steps: ProtoStep[];
  onTick: (stepId: string) => void;
  onAdd: (body: string) => void;
  onEdit: (stepId: string, body: string) => void;
  onDelete: (stepId: string) => void;
}) {
  const [editingId, setEditingId] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const editRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (editingId !== null) editRef.current?.select();
  }, [editingId]);
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
      {steps.map((step) => (
        <div key={step.id} style={{ display: "flex", alignItems: "center", gap: "var(--space-3)" }}>
          {editingId === step.id ? (
            <input
              ref={editRef}
              value={draft}
              aria-label="Edit step"
              onChange={(event) => setDraft(event.target.value)}
              onBlur={() => {
                if (draft.trim()) onEdit(step.id, draft.trim());
                setEditingId(null);
              }}
              onKeyDown={(event) => {
                if (event.key === "Enter") (event.target as HTMLInputElement).blur();
                if (event.key === "Escape") setEditingId(null);
              }}
              style={{
                flex: 1,
                font: "var(--type-body-sm)",
                color: "var(--text-primary)",
                background: "var(--surface-card)",
                border: "1px solid var(--border-default)",
                borderRadius: "var(--radius-sm)",
                padding: "var(--space-2) var(--space-3)",
              }}
            />
          ) : (
            <div style={{ flex: 1, minWidth: 0 }}>
              <Checkbox checked={step.done} label={step.body} onChange={() => onTick(step.id)} />
            </div>
          )}
          <IconButton
            icon="pencil"
            label={`Edit step: ${step.body}`}
            size="sm"
            onClick={() => {
              setDraft(step.body);
              setEditingId(step.id);
            }}
          />
          <IconButton
            icon="trash-2"
            label={`Delete step: ${step.body}`}
            size="sm"
            onClick={() => onDelete(step.id)}
          />
        </div>
      ))}
      {steps.length === 0 ? (
        <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
          No steps. Steps are 2–5 minute physical actions on this action's checklist.
        </p>
      ) : null}
      <AddRow placeholder="Add a step" onAdd={onAdd} />
    </div>
  );
}

/** A fog entry — question editable in place, resolve toggles. */
export function FogEntry({
  entry,
  onEdit,
  onResolve,
}: {
  entry: ProtoFog;
  onEdit: (question: string) => void;
  onResolve: (resolved: boolean) => void;
}) {
  const resolved = entry.resolvedAt !== null;
  return (
    <div
      style={{
        display: "flex",
        gap: "var(--space-4)",
        alignItems: "flex-start",
        opacity: resolved ? 0.55 : 1,
      }}
    >
      <Icon
        name="cloud-fog"
        size={16}
        color={resolved ? "var(--text-secondary)" : "var(--stage-grilling)"}
        style={{ marginTop: 3, flex: "0 0 auto" }}
      />
      <div style={{ flex: 1, minWidth: 0 }}>
        <span style={{ textDecoration: resolved ? "line-through" : "none", display: "block" }}>
          <InlineText
            value={entry.question}
            onCommit={onEdit}
            font="var(--type-body-strong)"
            label="fog question"
          />
        </span>
      </div>
      <Button
        variant={resolved ? "ghost" : "secondary"}
        size="sm"
        iconLeft={resolved ? "rotate-ccw" : "check"}
        onClick={() => onResolve(!resolved)}
      >
        {resolved ? "Reopen" : "Resolve"}
      </Button>
    </div>
  );
}

/** Links CRUD: rows plus a two-field add form. */
export function LinksEditor({
  links,
  onAdd,
  onRemove,
}: {
  links: ProtoLink[];
  onAdd: (url: string, label: string) => void;
  onRemove: (linkId: string) => void;
}) {
  const [url, setUrl] = useState("");
  const [label, setLabel] = useState("");
  function commit() {
    if (!url.trim()) return;
    onAdd(url.trim(), label.trim());
    setUrl("");
    setLabel("");
  }
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
      {links.map((link) => (
        <div key={link.id} style={{ display: "flex", alignItems: "center", gap: "var(--space-4)" }}>
          <Icon name="link" size={14} color="var(--text-secondary)" style={{ flex: "0 0 auto" }} />
          <a
            href={link.url}
            target="_blank"
            rel="noreferrer"
            style={{
              font: "var(--type-body-sm)",
              color: "var(--text-brand)",
              textDecoration: "none",
              flex: 1,
              minWidth: 0,
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
            }}
          >
            {link.label || link.url}
          </a>
          {link.label ? (
            <span className="hb-meta" style={{ maxWidth: 180, overflow: "hidden", textOverflow: "ellipsis" }}>
              {link.url.replace(/^https?:\/\//, "")}
            </span>
          ) : null}
          <IconButton icon="x" label={`Remove link ${link.label || link.url}`} size="sm" onClick={() => onRemove(link.id)} />
        </div>
      ))}
      <div style={{ display: "flex", gap: "var(--space-3)", alignItems: "center" }}>
        <div style={{ flex: 2 }}>
          <Input
            size="sm"
            placeholder="https://…"
            aria-label="Link URL"
            value={url}
            onChange={(event) => setUrl(event.target.value)}
            onKeyDown={(event) => event.key === "Enter" && commit()}
          />
        </div>
        <div style={{ flex: 1 }}>
          <Input
            size="sm"
            placeholder="label"
            aria-label="Link label"
            value={label}
            onChange={(event) => setLabel(event.target.value)}
            onKeyDown={(event) => event.key === "Enter" && commit()}
          />
        </div>
        <Button variant="secondary" size="sm" iconLeft="plus" onClick={commit} disabled={!url.trim()}>
          Add
        </Button>
      </div>
    </div>
  );
}

/** The derived-URL display of `github_repo` — canonical owner/repo stored,
 * URL derived; editable as the bare owner/repo string. */
export function RepoField({
  repo,
  onCommit,
}: {
  repo: string | null;
  onCommit: (next: string | null) => void;
}) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: "var(--space-3)", minWidth: 0 }}>
      <InlineText
        value={repo ?? ""}
        onCommit={(next) => onCommit(next || null)}
        font="var(--type-body-sm)"
        placeholder="owner/repo"
        allowEmpty
        label="GitHub repo"
      />
      {repo ? (
        <a
          href={repoUrl(repo)}
          target="_blank"
          rel="noreferrer"
          aria-label={`Open ${repoUrl(repo)}`}
          style={{ display: "inline-flex", color: "var(--text-brand)" }}
        >
          <Icon name="external-link" size={14} />
        </a>
      ) : null}
    </div>
  );
}

export function ContextSelect({
  value,
  onChange,
  label = "Default context",
}: {
  value: string | null;
  onChange: (next: string | null) => void;
  label?: string;
}) {
  return (
    <Select
      size="sm"
      aria-label={label}
      value={value ?? ""}
      options={[{ value: "", label: "no default context" }, ...CONTEXTS]}
      onChange={(event) => onChange(event.target.value || null)}
    />
  );
}

/** The archive warning dialog — copy is settled by #449 and shared across
 * variants on purpose: it states the cascade honestly. */
export function ArchiveDialog({
  project,
  onCancel,
  onConfirm,
}: {
  project: ProtoProject;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const liveCount = project.actions.filter(
    (action) => action.archivedAt === null,
  ).length;
  const preArchived = project.actions.length - liveCount;
  const footerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    // Focus lands on Cancel when the dialog opens — the safe control first.
    footerRef.current?.querySelector("button")?.focus();
    function onKey(event: globalThis.KeyboardEvent) {
      if (event.key === "Escape") onCancel();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onCancel]);

  return (
    <div
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onCancel();
      }}
      style={{
        position: "fixed",
        inset: 0,
        background: "var(--surface-scrim)",
        display: "grid",
        placeItems: "center",
        zIndex: 120,
      }}
    >
      <Card
        elevation={3}
        role="dialog"
        aria-modal="true"
        aria-labelledby="proto-archive-title"
        style={{ maxWidth: 440, margin: "var(--space-6)" }}
      >
        <h3 id="proto-archive-title" style={{ font: "var(--type-h3)", color: "var(--text-primary)" }}>
          Archive {project.name}?
        </h3>
        <p style={{ font: "var(--type-body)", color: "var(--text-secondary)", marginTop: "var(--space-4)" }}>
          Archives this project and its {liveCount} live item{liveCount === 1 ? "" : "s"} — they and
          their steps leave every surface together. Unarchiving restores exactly those{" "}
          {liveCount === 1 ? "that one" : String(liveCount)}
          {preArchived > 0
            ? `; the ${preArchived} archived beforehand stay${preArchived === 1 ? "s" : ""} archived.`
            : "."}
        </p>
        <div
          ref={footerRef}
          style={{
            display: "flex",
            justifyContent: "flex-end",
            gap: "var(--space-4)",
            marginTop: "var(--space-6)",
          }}
        >
          <Button variant="secondary" onClick={onCancel}>
            Cancel
          </Button>
          <Button variant="danger" iconLeft="archive" onClick={onConfirm}>
            Archive project
          </Button>
        </div>
      </Card>
    </div>
  );
}

/** Section heading in the prototype's own registers. */
export function ProtoSection({
  title,
  meta,
  children,
  style,
}: {
  title: string;
  meta?: string;
  children: ReactNode;
  style?: CSSProperties;
}) {
  return (
    <div style={style}>
      <div
        style={{
          display: "flex",
          alignItems: "baseline",
          justifyContent: "space-between",
          marginBottom: "var(--space-4)",
        }}
      >
        <h3 style={{ font: "var(--type-h3)", color: "var(--text-primary)" }}>{title}</h3>
        {meta ? <span className="hb-meta">{meta}</span> : null}
      </div>
      {children}
    </div>
  );
}
