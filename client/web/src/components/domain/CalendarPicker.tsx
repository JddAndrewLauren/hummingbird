import type { CSSProperties, HTMLAttributes, ReactNode } from "react";
import { Checkbox } from "../forms/Checkbox";

export interface CalendarEntry { id: string; summary: string }

// `onToggle` shadows the DOM's own toggle handler on the fieldset, so it is
// omitted here: this one takes a calendar id, not a ToggleEvent.
export interface CalendarPickerProps extends Omit<HTMLAttributes<HTMLFieldSetElement>, "style" | "onToggle"> {
  calendars?: CalendarEntry[];
  selectedIds?: string[];
  /** Selected ids the provider no longer offers — kept visible, checked, amber. */
  unavailableIds?: string[];
  /** Ids this device polls because a standing question's binding says so
   * (#121) — rendered checked and **locked**, with `lockedHint` saying why.
   * A calendar fetched with nothing on screen to explain it is the consent
   * surprise ADR-0005 guarded against, so the lock has to be visible. */
  lockedIds?: string[];
  lockedHint?: ReactNode;
  onToggle?: (id: string) => void;
  legend?: string;
  style?: CSSProperties;
}

// Mirrors client/web/src/calendar/CalendarPicker.tsx. A selected calendar the
// provider no longer offers stays listed, checked, and amber — unchecking it
// is the removal.
export function CalendarPicker({ calendars = [], selectedIds = [], unavailableIds = [], lockedIds = [], lockedHint, onToggle, legend = "Calendars to poll", style = {}, ...rest }: CalendarPickerProps) {
  return (
    <fieldset style={{ display: "flex", flexDirection: "column", gap: "var(--space-5)", margin: 0,
      padding: "var(--space-6)", background: "var(--surface-card)",
      border: "1px solid var(--border-subtle)", borderRadius: "var(--radius-card)", ...style }} {...rest}>
      <legend style={{ padding: "0 var(--space-3)", font: "var(--weight-semibold) var(--size-body-sm)/1 var(--font-sans)", color: "var(--text-secondary)" }}>{legend}</legend>
      {calendars.map((c) => (
        lockedIds.includes(c.id)
          // Locked, not merely re-ticked: `onChange` is absent so the row
          // cannot be unticked at all, which is the honest rendering of a
          // selection this device does not get to make on its own.
          ? <Checkbox key={c.id} checked disabled label={c.summary} hint={lockedHint} />
          : <Checkbox key={c.id} checked={selectedIds.includes(c.id)} onChange={() => onToggle && onToggle(c.id)} label={c.summary} />
      ))}
      {lockedIds.filter((id) => !calendars.some((c) => c.id === id)).map((id) => (
        // Bound to a calendar this credential's listing does not offer (a
        // stale binding, a listing that has not landed): still polled, so
        // still shown.
        <Checkbox key={id} checked disabled label={id} hint={lockedHint} />
      ))}
      {unavailableIds.map((id) => (
        <Checkbox key={id} checked tone="warn" onChange={() => onToggle && onToggle(id)}
          label={id} hint="Unavailable — uncheck to stop polling it" />
      ))}
    </fieldset>
  );
}
