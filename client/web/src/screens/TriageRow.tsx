import { useState } from "react";
import { Button } from "../components/core/Button";
import { Card } from "../components/core/Card";
import { Icon } from "../components/core/Icon";
import { MarkDoneButton } from "../components/domain/MarkDoneButton";
import { StageBadge } from "../components/domain/StageBadge";
import { Input } from "../components/forms/Input";
import { Select } from "../components/forms/Select";
import { Textarea } from "../components/forms/Textarea";
import { relativeAge } from "../shell/sync-status";
import type { ProjectDTO, TaskItemDTO, TriageDestinationName } from "../store/protocol";
import type { TaskTriageResult } from "../store/store";
import type { TriageEdits } from "../store/worker-client";
import { canGrill, canMarkDone } from "./item-actions";
import { PRIORITY_OPTIONS } from "./priority";
import { triageFailureFor } from "./write-failure";
import {
  buildTriageEdits,
  effectiveDraft,
  triageDraftProblems,
  type TriageDraft,
} from "./triage-form";

/** The free-vocabulary contexts the forms offer. Free vocab in the schema
 * (`items.context`), a fixed list here: this is a personal system and these are
 * the places its owner actually works. */
const CONTEXTS = ["@home", "@computer", "@phone", "@errands", "@garden", "@waiting"];

const SIZES: Array<{ value: TriageDraft["size"]; label: string }> = [
  { value: "", label: "Not set" },
  { value: "quick", label: "Quick" },
  { value: "short", label: "Short" },
  { value: "deep", label: "Deep" },
];

const ENERGIES: Array<{ value: TriageDraft["energy"]; label: string }> = [
  { value: "", label: "Not set" },
  { value: "low", label: "Low" },
  { value: "medium", label: "Medium" },
  { value: "high", label: "High" },
];

/** The DOM id a row's own "Grill me" button carries (#355, ADR-0023) — so
 * `TriageScreen` can find and re-focus it on Back after the takeover
 * unmounts and remounts the whole list, the same "look it up by id rather
 * than hold a ref across an unmount" contract `shell/CapturePopover.tsx`'s
 * `CAPTURE_TRIGGER_ID` uses for its own trigger. */
export function grillMeButtonId(itemId: string): string {
  return `triage-grill-me-${itemId}`;
}

export interface TriageRowProps {
  item: TaskItemDTO;
  projects: ProjectDTO[];
  /** Whether this row is the selected, expanded one. Selection is the
   * screen's, not the row's: only one row is open at a time. */
  expanded: boolean;
  onToggle: () => void;
  nowMs: number;
  /** S13/#111's triage mutation. Absent for a render with no worker behind it
   * (demo mode), in which case the row is readable and never expands into an
   * editor that could not send anything. */
  onTriage?: (itemId: string, destination: TriageDestinationName, edits: TriageEdits) => void;
  /** The one-click "mark done" checkmark — `Core::act`'s `complete`, NOT a
   * triage: a capture that turned out already finished skips the editor
   * entirely. Absent in demo mode, same as `onTriage`. This is the recorded
   * amendment to "Triage is pre-action by definition" (CONTEXT.md): the
   * detail-panel act vocabulary still offers nothing here, but finishing is
   * one click on every screen (`item-actions.ts`'s `canMarkDone`). */
  onComplete?: (itemId: string) => void;
  /** "Grill me" (#355, ADR-0023): opens the center-column takeover over
   * this item, decided by `item-actions.ts`'s `canGrill`. Absent in demo
   * mode, same as `onTriage` — beside "Send to grilling", never a
   * replacement for it. */
  onGrillMe?: (itemId: string) => void;
  /** The most recent triage result any row got back (`TaskState.lastTriage`)
   * — matched here by the item id the result itself carries, the same
   * broadcast-recognition contract `NowScreen`'s `actError` uses for
   * `lastAct`. A failure belongs to whichever item it names, never to
   * "whichever row is open"; an `"ok"` for this item is what clears the
   * typing below (issue #222 — a draft must survive a failed write). */
  lastTriage?: TaskTriageResult | null;
}

/** One triage-inbox row: a single line by default, the full editor when
 * selected.
 *
 * The collapsed line is the shape the design kit proposed and the demo fixtures
 * have always rendered — badge, title, provenance and age — because that is
 * what an inbox is for: reading what came in. Everything editable is one click
 * away rather than permanently open, which is the difference between a list of
 * eleven captures and eleven forms.
 *
 * Every decision is somebody else's: which fields changed is
 * `triage-form.ts`'s `buildTriageEdits`, what cannot be sent is
 * `triageDraftProblems`, priority's encoding is `priority.ts`, and the age
 * wording is `shell/sync-status.ts`'s `relativeAge`. This component threads
 * React state through them. */
export function TriageRow({
  item,
  projects,
  expanded,
  onToggle,
  nowMs,
  onTriage,
  onComplete,
  onGrillMe,
  lastTriage,
}: TriageRowProps) {
  // Only what the person has typed is state — see `effectiveDraft`'s doc for
  // why the rest is derived per render rather than seeded once.
  const [touched, setTouched] = useState<Partial<TriageDraft>>({});
  const draft = effectiveDraft(item, touched);
  const problems = triageDraftProblems(draft);
  const blocked = Object.keys(problems).length > 0;
  const editorId = `triage-editor-${item.id}`;

  function set(field: keyof TriageDraft, value: string): void {
    setTouched((current) => ({ ...current, [field]: value }));
  }

  // Reviewer finding on issue #222 (the capture/triage twin of PR #207's
  // act-failure defect): the typing used to clear the instant Promote was
  // clicked, optimistically, so a failed write lost the reader's edits AND
  // said nothing about the failure. It now stays put — `promote` below only
  // ever sends the mutation — and clears here, once and only once a result
  // actually reports `"ok"` for THIS item. The React-docs "adjusting state
  // when a prop changes" pattern, guarded on the result's own `seed` so a
  // broadcast already observed is never reprocessed, and keyed by the itemId
  // the result carries — a success on another row's item cannot wipe this
  // row's still-in-flight edits.
  const [processedTriageSeed, setProcessedTriageSeed] = useState<string | null>(null);
  if (lastTriage && lastTriage.seed !== processedTriageSeed) {
    setProcessedTriageSeed(lastTriage.seed);
    if (lastTriage.kind === "ok" && lastTriage.itemId === item.id) {
      setTouched({});
    }
  }

  // Matched by item id — see `write-failure.ts`, which owns this and the
  // sentence Now says when no row is mounted to say it (#418).
  const triageError = triageFailureFor(lastTriage, item.id);

  function promote(destination: TriageDestinationName): void {
    if (!onTriage || blocked) {
      return;
    }
    onTriage(item.id, destination, buildTriageEdits(draft, item));
  }

  return (
    <Card padding="0" style={{ display: "flex", flexDirection: "column" }}>
      {/* The checkmark is a SIBLING of the toggle button, never a child — a
          button nested in a button is invalid HTML, so the collapsed row is
          this flex pair. */}
      <div style={{ display: "flex", alignItems: "center" }}>
        <button
          type="button"
          onClick={onToggle}
          aria-expanded={expanded}
          aria-controls={expanded ? editorId : undefined}
          style={{
            display: "flex",
            alignItems: "center",
            gap: "var(--space-5)",
            flexWrap: "wrap",
            width: "100%",
            padding: "var(--space-5)",
            background: "transparent",
            border: "none",
            borderRadius: "var(--radius-card)",
            textAlign: "left",
            font: "inherit",
            color: "inherit",
            cursor: "pointer",
          }}
        >
          <StageBadge stage="triage" />
          {/* Wrap-then-ellipsis, the same contract `ItemRow` and the demo rows
              use: the `220px` basis is a floor, so the meta wraps onto its own
              line before the title is starved. */}
          <span
            style={{
              flex: "1 1 220px",
              minWidth: 0,
              font: "var(--type-body)",
              color: "var(--text-primary)",
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
            }}
          >
            {draft.title}
          </span>
          <span className="hb-meta" style={{ flex: "0 0 auto", whiteSpace: "nowrap" }}>
            {/* Provenance, then age. `source` is null on anything typed here
                rather than swept in, and "typed here" is the honest reading of
                that — never a fabricated source name. */}
            {item.source ?? "typed here"} · {relativeAge(Math.max(0, nowMs - item.createdAt))}
          </span>
          {item.pending ? (
            <span
              title="Not yet confirmed by the server"
              className="hb-meta"
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: "var(--space-2)",
                flex: "0 0 auto",
                whiteSpace: "nowrap",
              }}
            >
              <Icon name="loader-circle" size={13} />
              Pending
            </span>
          ) : null}
          {/* One chevron, rotated — `Icon`'s vocabulary carries `chevron-down`
              and adding an "up" twin for a state change would be a second glyph
              saying the same thing. The rotation is a token-timed flit, so the
              row's state change is animated rather than a jump. */}
          <span
            style={{
              display: "inline-flex",
              flex: "0 0 auto",
              transform: expanded ? "rotate(180deg)" : "none",
              transition: "transform var(--dur-base) var(--ease-flit)",
            }}
          >
            <Icon name="chevron-down" size={16} color="var(--text-muted)" />
          </span>
        </button>
        {onComplete && canMarkDone(item) ? (
          <MarkDoneButton
            title={item.title}
            disabled={item.pending}
            style={{ marginRight: "var(--space-4)" }}
            onClick={() => onComplete(item.id)}
          />
        ) : null}
      </div>

      {/* Outside the expanded block on purpose: a failure belongs to the item,
          and it must still be on screen when the result lands after the reader
          has collapsed the row. `role="alert"`: the paragraph appears with no
          other change on the page, so colour alone would never reach a screen
          reader.

          That reasoning covers this row on **Triage**, where the rows stand in
          a list and collapsing one leaves it mounted. It never covered Now,
          where the row is the slot and closing it unmounts this component
          entirely — so the failure had nowhere to land at all. Now says it
          itself in that case (`write-failure.ts`'s `strandedTriageFailure`,
          #418); this paragraph keeps the failure on the item wherever the row
          survives, and the two are mutually exclusive by construction. */}
      {triageError ? (
        <p
          role="alert"
          style={{
            font: "var(--type-body-sm)",
            color: "var(--status-danger-fg)",
            padding: "0 var(--space-5) var(--space-4)",
            margin: 0,
          }}
        >
          {triageError}
        </p>
      ) : null}

      {expanded && onTriage ? (
        <div
          id={editorId}
          style={{
            display: "flex",
            flexDirection: "column",
            gap: "var(--space-5)",
            padding: "var(--space-5)",
            paddingTop: "var(--space-4)",
            borderTop: "1px solid var(--border-subtle)",
          }}
        >
          <Input
            label="Title"
            size="sm"
            value={draft.title}
            error={problems.title}
            onChange={(event) => set("title", event.target.value)}
          />
          <Textarea
            label="Description"
            rows={3}
            value={draft.description}
            placeholder="The only free-prose field — never a checklist"
            onChange={(event) => set("description", event.target.value)}
          />
          <div
            style={{
              display: "grid",
              // The inner `min()` matters once the container is itself under
              // 160px: `auto-fit` cannot drop below one track, and that one
              // track would otherwise hold its 160px and overflow.
              gridTemplateColumns: "repeat(auto-fit, minmax(min(160px, 100%), 1fr))",
              gap: "var(--space-4)",
              alignItems: "start",
            }}
          >
            <Select
              label="Project"
              size="sm"
              value={draft.projectId}
              onChange={(event) => set("projectId", event.target.value)}
              options={[
                { value: "", label: "No project" },
                ...projects.map((project) => ({ value: project.id, label: project.name })),
              ]}
            />
            <Select
              label="Priority"
              size="sm"
              value={draft.priority}
              onChange={(event) => set("priority", event.target.value)}
              options={PRIORITY_OPTIONS}
            />
            <Select
              label="Size"
              size="sm"
              value={draft.size}
              onChange={(event) => set("size", event.target.value)}
              options={SIZES}
            />
            <Select
              label="Energy"
              size="sm"
              value={draft.energy}
              onChange={(event) => set("energy", event.target.value)}
              options={ENERGIES}
            />
            <Select
              label="Context"
              size="sm"
              value={draft.context}
              onChange={(event) => set("context", event.target.value)}
              options={[
                { value: "", label: "Not set" },
                ...CONTEXTS.map((context) => ({ value: context, label: context })),
              ]}
            />
            <Input
              label="Deadline"
              size="sm"
              value={draft.deadline}
              placeholder="YYYY-MM-DD"
              hint={problems.deadline ? undefined : "Day, or day and time"}
              error={problems.deadline}
              onChange={(event) => set("deadline", event.target.value)}
            />
            <Input
              label="Scheduled date"
              size="sm"
              type="date"
              value={draft.scheduledDate}
              error={problems.scheduledDate}
              onChange={(event) => set("scheduledDate", event.target.value)}
            />
          </div>
          {/* The source is shown, never edited: provenance belongs to whatever
              captured the item (`TriageEdits`' own doc). */}
          <span className="hb-meta">
            {item.sourceUrl
              ? `source ${item.source ?? "unknown"} · ${item.sourceUrl}`
              : `source ${item.source ?? "typed here"}`}
          </span>
          <div style={{ display: "flex", gap: "var(--space-4)", justifyContent: "flex-end" }}>
            {onGrillMe && canGrill(item.stage) ? (
              <Button
                id={grillMeButtonId(item.id)}
                size="sm"
                variant="secondary"
                iconLeft="sparkles"
                disabled={item.pending}
                onClick={() => onGrillMe(item.id)}
              >
                Grill me
              </Button>
            ) : null}
            <Button
              size="sm"
              variant="secondary"
              iconLeft="help-circle"
              disabled={item.pending || blocked}
              onClick={() => promote("grilling")}
            >
              Send to grilling
            </Button>
            <Button
              size="sm"
              variant="primary"
              iconLeft="check"
              disabled={item.pending || blocked}
              onClick={() => promote("ready")}
            >
              Promote to ready
            </Button>
          </div>
        </div>
      ) : null}
    </Card>
  );
}
