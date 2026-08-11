import type { CSSProperties } from "react";
import type { MouseEventHandler } from "react";
import { IconButton } from "../core/IconButton";

export interface MarkDoneButtonProps {
  /** The item's title — becomes the accessible name (`Mark "…" done`). */
  title: string;
  disabled?: boolean;
  onClick: MouseEventHandler<HTMLButtonElement>;
  style?: CSSProperties;
}

/** The one-click mark-done checkmark every row shares (`item-actions.ts`'s
 * `canMarkDone` decides who gets one; callers place it trailing — the
 * bottom-right of whatever pane the row wraps into). One component so the
 * glyph, the green and the label cannot drift between the five screens that
 * render it.
 *
 * The green is `--status-done-fg` — the same token the Done stage pill
 * carries, because this button *is* that status as a verb. A documented
 * exception to "icons never carry colour independently of their label"
 * (the design system's iconography rule): like the waste pane's bin glyphs,
 * the colour here encodes meaning, not decoration, and the accessible
 * label still says it in words.
 *
 * A local addition to the 16-component library, like `forms/Textarea` —
 * worth raising upstream rather than quietly keeping. */
export function MarkDoneButton({ title, disabled = false, onClick, style = {} }: MarkDoneButtonProps) {
  return (
    <IconButton
      icon="check"
      label={`Mark "${title}" done`}
      size="sm"
      disabled={disabled}
      style={{ flex: "0 0 auto", color: "var(--status-done-fg)", ...style }}
      onClick={onClick}
    />
  );
}
