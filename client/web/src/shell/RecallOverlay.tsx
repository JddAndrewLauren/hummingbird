import { useLayoutEffect, useRef } from "react";
import { Badge } from "../components/core/Badge";
import { Card } from "../components/core/Card";
import { IconButton } from "../components/core/IconButton";
import { StageBadge } from "../components/domain/StageBadge";
import { EmptyState } from "../components/feedback/EmptyState";
import { Input } from "../components/forms/Input";
import type { RecallRowDTO } from "../store/protocol";
import { useIsPhone } from "./useIsPhone";

/** The DOM id the header's Search button carries, so this overlay can
 * measure what it hangs from — the identical trick `CapturePopover.tsx`
 * uses for `CAPTURE_TRIGGER_ID`, for the identical reason (`IconButton`
 * forwards no ref). */
export const RECALL_TRIGGER_ID = "shell-recall-trigger";

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
}

/** One label for a [`RecallRowDTO`]'s `group` — the same three buckets
 * `Core::search` orders by, restated for a reader rather than a sort. */
const GROUP_LABEL: Record<RecallRowDTO["group"], string> = {
  live: "live",
  done: "done",
  archived: "archived",
};

function RecallRow({ row }: { row: RecallRowDTO }) {
  return (
    <Card
      padding="var(--space-5)"
      style={{
        display: "flex",
        alignItems: "center",
        gap: "var(--space-5)",
        flexWrap: "wrap",
        // Recede, never hide — the same "labelled, not hidden" reasoning
        // `LedgerScreen`'s row applies to an archived row.
        opacity: row.group === "archived" ? 0.72 : 1,
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
    </Card>
  );
}

/** **Recall** (#478, CONTEXT.md): the lookup gesture over everything the
 * mirror has ever known — never a ranking or attention surface, never a
 * per-screen filter. Read-only in this slice: a result row states its
 * stage and nothing here reaches `Core::act` or any other mutation —
 * selecting a row is #479's slice. Every trigger (the header's Search
 * button, the `/` hotkey, the rail's magnifier, the phone More sheet's
 * entry) and the Escape wiring are #480's, all landing on the same
 * `open`/`onClose` this component already took.
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
 * against, whatever `rows` last held. */
export function RecallOverlay({ open, query, onQueryChange, onClose, rows, total }: RecallOverlayProps) {
  const restoreTo = useRef<Element | null>(null);
  const cardRef = useRef<HTMLDivElement>(null);
  const isPhone = useIsPhone();
  const trimmed = query.trim();

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
                <RecallRow key={row.id} row={row} />
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
