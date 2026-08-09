import * as React from "react";
export interface CalendarEntry { id: string; summary: string }
// `onToggle` shadows the DOM's own toggle handler on the fieldset, so it is
// omitted here: this one takes a calendar id, not a ToggleEvent.
export interface CalendarPickerProps extends Omit<React.HTMLAttributes<HTMLFieldSetElement>, "style" | "onToggle"> {
  calendars?: CalendarEntry[];
  selectedIds?: string[];
  /** Selected ids the provider no longer offers — kept visible, checked, amber. */
  unavailableIds?: string[];
  onToggle?: (id: string) => void;
  legend?: string;
  style?: React.CSSProperties;
}
export declare function CalendarPicker(props: CalendarPickerProps): JSX.Element;
