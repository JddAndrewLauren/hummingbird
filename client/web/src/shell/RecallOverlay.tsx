import { useLayoutEffect, useRef, useState } from "react";
import { Badge } from "../components/core/Badge";
import { Card } from "../components/core/Card";
import { IconButton } from "../components/core/IconButton";
import { ItemPanel } from "../components/domain/ItemPanel";
import { StageBadge } from "../components/domain/StageBadge";
import { EmptyState } from "../components/feedback/EmptyState";
import { Input } from "../components/forms/Input";
import type { ProjectDTO, RecallRowDTO } from "../store/protocol";
import type { TaskTriageResult } from "../store/store";
import type { TriageEdits } from "../store/worker-client";
import { relativeAge } from "./sync-status";
import { RECALL_TRIGGER_ID } from "./trigger-ids";
import { useIsPhone } from "./useIsPhone";

/** The DOM id the header's Search button carries, so this overlay can
 * measure what it hangs from — the identical trick `CapturePopover.tsx`
 * uses for `CAPTURE_TRIGGER_ID`, for the identical reason (`IconButton`
 * forwards no ref). Re-exported rather than defined here: it now lives in
 * the JSX-free `trigger-ids.ts` so `visual/surfaces.spec.ts` can import it
 * too (that file's own header says why). */
export { RECALL_TRIGGER_ID };

/** The query field's own DOM id — `capture-hotkey.ts`'s `CAPTURE_INPUT_ID`
 * pattern, reused for the identical reason: `Input` forwards no ref, so
 * focusing it programmatically (rather than via the `autoFocus` prop, which
 * the lint config bans for accessibility) means finding it by id instead. */
const RECALL_QUERY_ID = "shell-recall-query";

const ANCHOR_GAP = 8;
const BOTTOM_ROOM = 24;

export interface RecallOverlayProps {
  open: boolean;
  /** The query as currently typed. Controlled here rather than held inside
   * this component: `App.tsx` owns it so the wiring hook that requests
   * `Core::search` can key its effect on the same value without a second,
   * out-of-band copy. */
  query: string;
  onQueryChange: (query: string) => void;
  onClose: () => void;
  /** `TaskState.search`'s last answer for the current `query`, already
   * mapped to the wire's camelCase DTO. `null` until the first answer for a
   * non-empty query arrives — rendered as "searching", never as "nothing
   * matched" (the same "no answer vs. an empty one" contract every other
   * read in this app keeps). Ignored entirely while `query` is blank: an
   * empty or whitespace-only query lists nothing, decided here rather than
   * waiting on a round trip to the core to say the same thing. */
  rows: RecallRowDTO[] | null;
  /** The un-capped match count `rows.length` may fall short of — the "N
   * more" line reads this directly, never `rows.length` (decision 8: the
   * cap and the count are core-decided). */
  total: number;
  /** The Project select an expanded live result's edit form offers — the
   * same list `ItemPanel` takes everywhere else. `[]` (never omitted) is a
   * legitimate value, same as there. */
  projects: ProjectDTO[];
  /** #122's stage-agnostic edit, the exact mutation `ItemPanel`'s detail-mode
   * Save already calls — this overlay mints no second write path. `App.tsx`
   * passes `handleTriage` through unconditionally (since #456); a result row
   * with no `stage: "live"` group still renders with no Edit affordance,
   * same as a Done or archived one. */
  onTriage?: (itemId: string, destination: "ready" | null, edits: TriageEdits) => void;
  /** The most recent triage result any editor got back (`TaskState.lastTriage`)
   * — what clears a live result's typing on an `"ok"` naming it (#222) and
   * what an expanded live result renders as a failure, exactly as `ItemPanel`
   * already does for Now and Triage. */
  lastTriage?: TaskTriageResult | null;
  /** #771's bound Obsidian vault name, forwarded to `ItemPanel` — which
   * carries the whole of what it means. */
  vaultName?: string | null;
  /** `useSyncWiring`'s re-sampled clock (`App.tsx`'s `syncNowMs`) — the same
   * "now" `DoneScreen.tsx`, `TriageRow.tsx` and `LedgerScreen.tsx`'s own
   * `relativeAge` calls read, never `Date.now()` taken fresh here: a
   * component render must stay a pure function of its props (the lint rule
   * this app's `react-hooks/purity` config enforces), and this is the prop
   * that keeps it one. */
  nowMs: number;
}

/** One label for a [`RecallRowDTO`]'s `group` — the same three buckets
 * `Core::search` orders by, restated for a reader rather than a sort. */
const GROUP_LABEL: Record<RecallRowDTO["group"], string> = {
  live: "live",
  done: "done",
  archived: "archived",
};

/** #479's selection: clicking a row expands it inline, in place — the result
 * list stays on screen and nothing here ever navigates away (decision 5).
 * `expanded`/`onToggle` are owned by `RecallOverlay`, keyed on `row.id`, so
 * at most one result is open at a time. */
function RecallRow({
  row,
  expanded,
  onToggle,
  projects,
  onTriage,
  lastTriage,
  vaultName,
  nowMs,
}: {
  row: RecallRowDTO;
  expanded: boolean;
  onToggle: () => void;
  projects: ProjectDTO[];
  onTriage?: (itemId: string, destination: "ready" | null, edits: TriageEdits) => void;
  lastTriage?: TaskTriageResult | null;
  vaultName?: string | null;
  nowMs: number;
}) {
  return (
    <Card
      padding="var(--space-5)"
      style={{
        display: "flex",
        flexDirection: "column",
        gap: "var(--space-4)",
        // Recede, never hide — the same "labelled, not hidden" reasoning
        // `LedgerScreen`'s row applies to an archived row.
        opacity: row.group === "archived" ? 0.72 : 1,
      }}
    >
      {/* The clickable summary is its own `role="button"` element, a sibling
          of the expanded content below rather than its ancestor. Unlike
          `FrontierColumns.tsx`'s `ItemCard`, which safely puts `role="button"`
          on the whole `Card` guarded by an `event.target === currentTarget`
          check (its one interactive descendant is a single checkmark
          button), the expanded block here holds a whole edit form's worth of
          buttons and inputs — a click landing on any of them must never
          bubble up and toggle the summary shut, and a target-equality guard
          alone would not stop that once the click's target IS one of those
          descendants. Two siblings instead of one ancestor makes it true for
          free, with no guard and no `stopPropagation` needed anywhere. */}
      <div
        role="button"
        tabIndex={0}
        aria-expanded={expanded}
        onClick={onToggle}
        onKeyDown={(event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            onToggle();
          }
        }}
        style={{
          display: "flex",
          alignItems: "center",
          gap: "var(--space-5)",
          flexWrap: "wrap",
          cursor: "pointer",
        }}
      >
        <StageBadge stage={row.stage} />
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
          {row.title}
        </span>
        <Badge mono tone="neutral">
          {GROUP_LABEL[row.group]}
        </Badge>
      </div>

      {expanded ? (
        <div style={{ borderTop: "1px solid var(--border-subtle)", paddingTop: "var(--space-4)" }}>
          {/* Timestamps: the one fact `ItemPanel` doesn't already state (it
              shows the HB handle and the description itself, in both its
              read-only and edit-mode renders). Read off `nowMs`, the same
              re-sampled clock `DoneScreen` uses for its own `relativeAge` —
              never a fresh `Date.now()` here, which a component render must
              stay pure of. */}
          <p className="hb-meta" style={{ marginBottom: "var(--space-4)" }}>
            created {relativeAge(Math.max(0, nowMs - row.createdAt))} · updated{" "}
            {relativeAge(Math.max(0, nowMs - row.updatedAt))}
          </p>
          {/* The same edit form the rest of the app uses (#479's acceptance),
              over the same `useItemDraft` state and the same `Core::triage`
              mutation — never a second copy. `onTriage` is only handed down
              for a `"live"` result: a Done or archived row gets no `onTriage`
              at all, which is exactly what makes `ItemPanel` render with no
              Edit affordance (its own "offers no Edit at all without an
              onTriage" rule) rather than a second read-only mode invented
              here. */}
          <ItemPanel
            key={row.id}
            mode="detail"
            item={row}
            projects={projects}
            onTriage={row.group === "live" ? onTriage : undefined}
            lastTriage={lastTriage}
            vaultName={vaultName}
            // This overlay has no steps wiring of its own — nothing here
            // ever called `requestSteps` for a result row — so the block
            // that would otherwise assert "No Steps yet." is not rendered
            // at all rather than stating a fact it never asked `Core` for.
            showSteps={false}
          />
        </div>
      ) : null}
    </Card>
  );
}

/** **Recall** (#478, CONTEXT.md): the lookup gesture over everything the
 * mirror has ever known — never a ranking or attention surface, never a
 * per-screen filter. Reachable from four triggers (#480): the header's
 * Search button, the `/` hotkey, the rail's magnifier and the phone More
 * sheet's entry, all landing on the same `open`/`onClose` this component
 * already took.
 *
 * Selecting a row opens it in place (#479): a live result's expansion
 * reaches `Core::triage` through `ItemPanel`, the same mutation path detail
 * mode everywhere else uses; a Done or archived result's expansion never
 * reaches `Core::act` or any other mutation, since it is handed no
 * `onTriage` at all.
 *
 * Built in the exact shape `CapturePopover` is (decision 4): a scrim, a
 * `role="dialog"` card hung off the control that opened it
 * (`RECALL_TRIGGER_ID`), the close button, a scrim click, and Escape
 * (`escape-claimants.ts`'s `search` claimant, wired in `App.tsx`) as the
 * ways out.
 *
 * Matching, ordering and the cap are entirely `Core::search`'s (via
 * `useRecallWiring.ts`) — this component only renders whatever `rows` and
 * `total` it is handed, plus the one client-only rule an empty query never
 * needs a round trip to state: blank `query` renders nothing to type
 * against, whatever `rows` last held.
 *
 * **#479's selection.** Which result is expanded is local, device-only UI
 * state — nothing about "which row is open" is a fact the mirror or any
 * other screen needs, so it is never lifted to `App.tsx` the way `query` is.
 * It resets to nothing whenever the overlay opens fresh, so a stale
 * expansion from a previous search session never survives the close/reopen
 * — the same "own it or reset it" rule `CapturePopover`'s focus restoration
 * follows. */
export function RecallOverlay({
  open,
  query,
  onQueryChange,
  onClose,
  rows,
  total,
  projects,
  onTriage,
  lastTriage,
  vaultName = null,
  nowMs,
}: RecallOverlayProps) {
  const restoreTo = useRef<Element | null>(null);
  const cardRef = useRef<HTMLDivElement>(null);
  const isPhone = useIsPhone();
  const trimmed = query.trim();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  // A fresh open starts fully collapsed (this component's own doc: the
  // expansion never survives a close/reopen). Derived from the previous
  // render's `open` rather than an effect — React's own "adjusting state
  // when a prop changes" pattern — so nothing here calls `setState`
  // synchronously inside an effect body.
  const [wasOpen, setWasOpen] = useState(open);
  if (open !== wasOpen) {
    setWasOpen(open);
    if (open) {
      setSelectedId(null);
    }
  }

  useLayoutEffect(() => {
    if (!open) {
      return;
    }
    function measure() {
      const card = cardRef.current;
      const trigger = document.getElementById(RECALL_TRIGGER_ID);
      if (!card || !trigger) {
        return;
      }
      const rect = trigger.getBoundingClientRect();
      const top = rect.bottom + ANCHOR_GAP;
      card.style.top = `${top}px`;
      if (isPhone) {
        card.style.left = "var(--gutter-page)";
      } else {
        card.style.left = "";
        card.style.right = `${window.innerWidth - rect.right}px`;
      }
      card.style.maxHeight = `${window.innerHeight - top - BOTTOM_ROOM}px`;
    }
    measure();
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, [open, isPhone]);

  // Focuses the query field on open and restores whatever had focus
  // beforehand on close — the identical round trip `CapturePopover` makes,
  // reached by id (`RECALL_QUERY_ID`) rather than a ref for the reason
  // documented on that constant.
  useLayoutEffect(() => {
    if (!open) {
      return;
    }
    restoreTo.current = document.activeElement;
    document.getElementById(RECALL_QUERY_ID)?.focus();
    return () => {
      const target = restoreTo.current;
      if (target instanceof HTMLElement && target.isConnected) {
        target.focus();
      }
    };
  }, [open]);

  if (!open) {
    return null;
  }

  const more = Math.max(0, total - (rows?.length ?? 0));

  return (
    <div
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) {
          onClose();
        }
      }}
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 40,
        background: "var(--surface-scrim)",
      }}
    >
      <div
        ref={cardRef}
        role="dialog"
        aria-modal="true"
        aria-label="Recall"
        style={{
          position: "fixed",
          top: "var(--space-10)",
          right: "var(--gutter-page)",
          width: "min(720px, calc(100vw - 2 * var(--gutter-page)))",
          maxHeight: "calc(100dvh - 2 * var(--space-10))",
          overflowY: "auto",
        }}
      >
        <Card
          elevation={3}
          padding="var(--space-6)"
          style={{ display: "flex", flexDirection: "column", gap: "var(--space-5)" }}
        >
          <div style={{ display: "flex", alignItems: "flex-start", gap: "var(--space-4)" }}>
            <Input
              id={RECALL_QUERY_ID}
              icon="search"
              placeholder="Search everything — title, notes, project, or hb-42"
              value={query}
              onChange={(event) => onQueryChange(event.target.value)}
              style={{ flex: 1 }}
            />
            <IconButton icon="x" label="Close" onClick={onClose} />
          </div>

          {trimmed.length === 0 ? (
            <EmptyState
              icon="search"
              compact
              headingLevel={3}
              title="Type to search"
              body="Live, Done and archived items — everything the mirror has ever known."
            />
          ) : rows === null ? (
            <span className="hb-meta">Searching…</span>
          ) : rows.length === 0 ? (
            <EmptyState
              icon="search"
              compact
              headingLevel={3}
              title="Nothing matched"
              body="Every word has to appear, or type a handle like hb-42."
            />
          ) : (
            <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
              {rows.map((row) => (
                <RecallRow
                  key={row.id}
                  row={row}
                  expanded={row.id === selectedId}
                  onToggle={() => setSelectedId((current) => (current === row.id ? null : row.id))}
                  projects={projects}
                  onTriage={onTriage}
                  lastTriage={lastTriage}
                  vaultName={vaultName}
                  nowMs={nowMs}
                />
              ))}
              {more > 0 ? (
                <span className="hb-meta">{more} more matched — narrow the words to see them</span>
              ) : null}
            </div>
          )}
        </Card>
      </div>
    </div>
  );
}
