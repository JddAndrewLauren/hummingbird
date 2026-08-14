import { useState } from "react";
import type { CSSProperties, HTMLAttributes } from "react";
import { Icon } from "../core/Icon";
import { MarkDoneButton } from "./MarkDoneButton";
import { hasPriority, priorityLabel } from "../../screens/priority";
import { energyIcon, energyTitle, levelColor, sizeIcon, sizeTitle } from "../../screens/size-energy";
import type { TaskItemDTO } from "../../store/protocol";
import { StageBadge } from "./StageBadge";
import type { Stage } from "./StageBadge";

export interface ItemRowProps extends Omit<HTMLAttributes<HTMLDivElement>, "style"> {
  title: string;
  stage?: Stage;
  /** Derived at read time, never stored — see CONTEXT.md "Urgency". */
  urgency?: "calm" | "soon" | "now" | "overdue";
  /** Deadline the world imposes. Rendered with a flag. */
  deadline?: string;
  /** Do-date the human chose. Rendered with a calendar glyph, always muted. */
  scheduled?: string;
  /** The item's own `size` (`quick` / `normal` / `deep`), rendered as a
   * depth-ring glyph and its uppercase word, both in the level's ramp
   * colour (`screens/size-energy.ts`). Absent means the caller has nothing
   * to say — this row omits it entirely rather than drawing the unset
   * ghost, the same contract `priority`, `steps` and `blockedBy` follow.
   * The full-item surface that *does* draw absence is `ItemDetailPanel`. */
  size?: TaskItemDTO["size"];
  /** The item's own `energy` (`low` / `medium` / `high`), drawn beside size
   * in the same treatment (#446). Rows carried no energy at all before
   * that — it rendered in exactly one place, the detail panel — which is
   * what made the two dimensions feel like different kinds of fact when
   * they are one kind. */
  energy?: TaskItemDTO["energy"];
  /** The owned schema's raw `items.priority` wire value (0..4, ADR-0009) —
   * rendered by its label (`priorityLabel`), never the raw number, which is
   * inverted and holed (issue #108). Omitted entirely at "No priority"
   * (0), the same "nothing to say" contract every other optional meta chip
   * on this row already follows. */
  priority?: number;
  /** Count or key of the actions this one is blocked by. */
  blockedBy?: string;
  /** Microtask progress, e.g. "2/5". */
  steps?: string;
  /** Set once an unconfirmed capture/mutation is overlaid on this item
   * (`TaskState.pending`, `Core::is_pending`) — a pending item must be
   * marked as such (issue #108) rather than rendered indistinguishably
   * from confirmed server truth. */
  pending?: boolean;
  selected?: boolean;
  /** The one-click "mark done" checkmark (`item-actions.ts`'s
   * `canMarkDone` decides who gets one — callers pass this only for items
   * it allows). Rendered trailing — the row's bottom-right — and disabled
   * while `pending` so a second act can never race the queued one. */
  onComplete?: () => void;
  style?: CSSProperties;
}

const URGENCY: Record<"calm" | "soon" | "now" | "overdue", string> = { calm: "var(--urgency-calm)", soon: "var(--urgency-soon)", now: "var(--urgency-now)", overdue: "var(--urgency-overdue)" };
// The dot carries urgency in colour alone, so its tooltip says it in words —
// the stored enum is not what a reader wants hovering a coloured dot.
const URGENCY_LABEL: Record<"calm" | "soon" | "now" | "overdue", string> = { calm: "Calm", soon: "Due soon", now: "Due now", overdue: "Overdue" };

export function ItemRow({ title, stage = "ready", urgency = "calm", deadline, scheduled, size, energy, priority, blockedBy, steps, pending = false, selected = false, onComplete, onClick, onKeyDown, onMouseEnter, onMouseLeave, style = {}, ...rest }: ItemRowProps) {
  const [hover, setHover] = useState(false);
  // No onClick, no affordance: a row that does nothing must not take focus,
  // announce itself as a button, or claim a pointer.
  const activatable = Boolean(onClick);
  return (
    // The role, the tab stop and Enter/Space activation all arrive together
    // with `onClick`; without it the row is inert text with hover paint.
    // (jsx-a11y reads this ternary role only because the config enables
    // `allowExpressionValues` for exactly this case.)
    <div role={activatable ? "button" : undefined} tabIndex={activatable ? 0 : undefined} onClick={onClick}
      onKeyDown={(event) => {
        // onClick on a div never fires from the keyboard, so Enter and Space
        // are wired by hand; Space is prevented first or it scrolls the list.
        // Guarded to the row itself: a keypress on the checkmark button
        // bubbles up here, and its own native activation must not also open
        // the row.
        if (activatable && event.target === event.currentTarget && (event.key === "Enter" || event.key === " ")) {
          event.preventDefault();
          event.currentTarget.click();
        }
        onKeyDown?.(event);
      }}
      onMouseEnter={(event) => { setHover(true); onMouseEnter?.(event); }}
      onMouseLeave={(event) => { setHover(false); onMouseLeave?.(event); }}
      // `hb-item-row` carries only what the phone form has to change — the
      // wrap. Everything below is unconditional or caller-dependent and stays
      // inline. See `shell/responsive.css` for why the split is by kind.
      className="hb-item-row"
      style={{
        display: "flex", alignItems: "center", gap: "var(--space-5)",
        minHeight: "var(--row-height)", padding: "var(--space-4) var(--space-5)",
        background: selected ? "var(--accent-quiet)" : hover ? "var(--surface-quiet)" : "transparent",
        borderLeft: `2px solid ${selected ? "var(--accent)" : "transparent"}`,
        borderRadius: "var(--radius-sm)", cursor: activatable ? "pointer" : undefined,
        transition: "background var(--dur-fast) var(--ease-flit)", ...style,
      }} {...rest}>
      <span title={URGENCY_LABEL[urgency] || URGENCY_LABEL.calm} style={{ width: 6, height: 6, borderRadius: "50%", flex: "0 0 auto", background: URGENCY[urgency] || URGENCY.calm }} />
      {/* No `opacity` change here for `pending`: the chip below is the one
          pending indicator, deliberately, so a caller that also dims this
          row for an unrelated reason (e.g. NowScreen's "Blocked" section)
          never compounds two opacities into an over-muted row
          (PR #200 review). */}
      {/* The flex/clip half of this span's styling is `hb-item-row-title` in
          `shell/responsive.css`, not inline: on a phone the title takes the
          whole first line and wraps instead of clipping, and at equal
          importance a stylesheet rule loses to an element's own `style`
          attribute. Those properties moved out rather than being fought with
          an `!important` apiece. Only what depends on `stage` stays here —
          the media query never touches it, so it costs the class nothing. */}
      <span className="hb-item-row-title" style={{ color: stage === "done" ? "var(--text-muted)" : "var(--text-primary)",
        textDecoration: stage === "done" ? "line-through" : "none" }}>{title}</span>
      {pending ? (
        <span title="Not yet confirmed by the server" style={{ display: "inline-flex", alignItems: "center", gap: "var(--space-2)", font: "var(--type-meta)", letterSpacing: "var(--tracking-meta)", textTransform: "uppercase", color: "var(--text-muted)" }}>
          <Icon name="loader-circle" size={13} />Pending
        </span>
      ) : null}
      {steps ? (
        <span style={{ display: "inline-flex", alignItems: "center", gap: "var(--space-2)", font: "var(--type-meta)", color: "var(--text-muted)" }}>
          <Icon name="list-checks" size={13} />{steps}
        </span>
      ) : null}
      {blockedBy ? (
        <span style={{ display: "inline-flex", alignItems: "center", gap: "var(--space-2)", font: "var(--type-meta)", color: "var(--status-danger-fg)" }}>
          <Icon name="link" size={13} />{blockedBy}
        </span>
      ) : null}
      {/* Glyph only, no word (#446): the row annotates a title, and two
          spelled-out dimensions per line competed with it. `title` is what
          keeps the mark answerable — the same pattern `pending` above uses,
          and the reason `sizeLabel` is not called here. */}
      {size ? (
        <span title={sizeTitle(size)} style={{ display: "inline-flex", alignItems: "center", color: levelColor(size) }}>
          <Icon name={sizeIcon(size)} size={13} />
        </span>
      ) : null}
      {energy ? (
        <span title={energyTitle(energy)} style={{ display: "inline-flex", alignItems: "center", color: levelColor(energy) }}>
          <Icon name={energyIcon(energy)} size={13} />
        </span>
      ) : null}
      {priority !== undefined && hasPriority(priority) ? (
        <span style={{ font: "var(--type-meta)", letterSpacing: "var(--tracking-meta)", textTransform: "uppercase", color: "var(--text-brand)" }}>{priorityLabel(priority)}</span>
      ) : null}
      {scheduled ? (
        <span style={{ display: "inline-flex", alignItems: "center", gap: "var(--space-2)", font: "var(--type-meta)", color: "var(--text-muted)" }}>
          <Icon name="calendar" size={13} />{scheduled}
        </span>
      ) : null}
      {deadline ? (
        <span style={{ display: "inline-flex", alignItems: "center", gap: "var(--space-2)", font: "var(--type-meta)",
          color: urgency === "overdue" ? "var(--urgency-overdue)" : urgency === "now" ? "var(--urgency-now)" : "var(--text-secondary)" }}>
          <Icon name="flag" size={13} />{deadline}
        </span>
      ) : null}
      <StageBadge stage={stage} />
      {onComplete ? (
        <MarkDoneButton
          title={title}
          disabled={pending}
          onClick={(event) => {
            // The row's own click opens item detail — finishing something
            // must never also open it.
            event.stopPropagation();
            onComplete();
          }}
        />
      ) : null}
    </div>
  );
}
