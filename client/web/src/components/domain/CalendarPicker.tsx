import type { CSSProperties, HTMLAttributes } from "react";
import { Checkbox } from "../forms/Checkbox";

export interface CalendarEntry { id: string; summary: string }

export interface CalendarPickerProps extends Omit<HTMLAttributes<HTMLFieldSetElement>, "style"> {
  calendars?: CalendarEntry[];
  selectedIds?: string[];
  /** Selected ids the provider no longer offers — kept visible, checked, amber. */
  unavailableIds?: string[];
  onToggle?: (id: string) => void;
  legend?: string;
  style?: CSSProperties;
}

// Mirrors client/web/src/calendar/CalendarPicker.tsx. A selected calendar the
// provider no longer offers stays listed, checked, and amber — unchecking it
// is the removal.
export function CalendarPicker({ calendars = [], selectedIds = [], unavailableIds = [], onToggle, legend = "Calendars to poll", style = {}, ...rest }: CalendarPickerProps) {
  return (
    <fieldset style={{ display: "flex", flexDirection: "column", gap: "var(--space-5)", margin: 0,
      padding: "var(--space-6)", background: "var(--surface-card)",
      border: "1px solid var(--border-subtle)", borderRadius: "var(--radius-card)", ...style }} {...rest}>
      <legend style={{ padding: "0 var(--space-3)", font: "var(--weight-semibold) var(--size-body-sm)/1 var(--font-sans)", color: "var(--text-secondary)" }}>{legend}</legend>
      {calendars.map((c) => (
        <Checkbox key={c.id} checked={selectedIds.includes(c.id)} onChange={() => onToggle && onToggle(c.id)} label={c.summary} />
      ))}
      {unavailableIds.map((id) => (
        <Checkbox key={id} checked tone="warn" onChange={() => onToggle && onToggle(id)}
          label={id} hint="Unavailable — uncheck to stop polling it" />
      ))}
    </fieldset>
  );
}
