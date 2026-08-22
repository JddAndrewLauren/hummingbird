import { EmptyState } from "../../components/feedback/EmptyState";

// One pane's "there is no answer, and here is why" rendering, in the two
// forms its two hosts need.
//
// Now's aside draws the whole thing: an `EmptyState` whose heading is the
// gap sentence and whose body is the reason. The Status board's expanded
// tile has already drawn that same sentence in its own header — it is the
// pane's `collapsedHeadline` — so repeating it as a heading two lines below
// says one thing twice, and on an unpolled device that duplication is the
// entire board rather than an edge case. There, only the reason is new, so
// only the reason is drawn.

export interface PaneGapProps {
  /** Whether this host still needs the gap's own headline. */
  headline: boolean;
  /** The gap sentence — the same words the pane's `collapsedHeadline` uses. */
  title: string;
  /** Why there is no answer. The only line the board's tile has not said. */
  body: string;
}

export function PaneGap({ headline, title, body }: PaneGapProps) {
  if (!headline) {
    return (
      <p
        style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}
      >
        {body}
      </p>
    );
  }
  return (
    <EmptyState
      compact
      icon="cloud-fog"
      headingLevel={3}
      title={title}
      body={body}
    />
  );
}
