import * as React from "react";
export interface CalendarEntry { id: string; summary: string }
export interface CalendarPickerProps extends Omit<React.HTMLAttributes<HTMLFieldSetElement>, "style"> {
  calendars?: CalendarEntry[];
  selectedIds?: string[];
  /** Selected ids the provider no longer offers — kept visible, checked, amber. */
  unavailableIds?: string[];
  onToggle?: (id: string) => void;
  legend?: string;
  style?: React.CSSProperties;
}
export declare function CalendarPicker(props: CalendarPickerProps): JSX.Element;
