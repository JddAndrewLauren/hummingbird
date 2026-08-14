import { Card } from "../components/core/Card";
import { Icon } from "../components/core/Icon";
import { ItemPanel } from "../components/domain/ItemPanel";
import { MarkDoneButton } from "../components/domain/MarkDoneButton";
import { StageBadge } from "../components/domain/StageBadge";
import { relativeAge } from "../shell/sync-status";
import type { ProjectDTO, TaskItemDTO, TriageDestinationName } from "../store/protocol";
import type { TaskTriageResult } from "../store/store";
import type { TriageEdits } from "../store/worker-client";
import { canMarkDone } from "./item-actions";
import { triageFailureFor } from "./write-failure";

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
  onTriage?: (
    itemId: string,
    destination: TriageDestinationName | null,
    edits: TriageEdits,
  ) => void;
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
  const editorId = `triage-editor-${item.id}`;

  // Matched by item id — see `write-failure.ts`, which owns this and the
  // sentence Now says when no row is mounted to say it (#418).
  const triageError = triageFailureFor(lastTriage, item.id);

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
            {item.title}
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
        <ItemPanel
          mode="triage"
          id={editorId}
          item={item}
          projects={projects}
          onTriage={onTriage}
          lastTriage={lastTriage}
          onGrillMe={onGrillMe}
          grillMeId={grillMeButtonId(item.id)}
        />
      ) : null}
    </Card>
  );
}
