import type { CalendarListEntry } from "./calendarList";
import { toggleCalendarId } from "./selection";

// The calendar picker (issue #73): its selection is what drives which
// calendars the core polls (`setCalendarIds`). Deliberately thin — every
// decision here is a call into selection.ts, which is what's unit-tested;
// this component itself only renders and forwards events (same split as
// App.tsx/store.ts throughout #69).

export interface CalendarPickerProps {
  calendars: CalendarListEntry[];
  selectedCalendarIds: string[];
  onChange: (selectedCalendarIds: string[]) => void;
}

export function CalendarPicker({
  calendars,
  selectedCalendarIds,
  onChange,
}: CalendarPickerProps) {
  if (calendars.length === 0) {
    return null;
  }

  return (
    <fieldset className="flex flex-col gap-2 rounded-lg border border-slate-800 p-4">
      <legend className="px-1 text-sm font-medium text-slate-300">
        Calendars to poll
      </legend>
      {calendars.map((calendar) => (
        <label
          key={calendar.id}
          className="flex items-center gap-2 text-sm text-slate-200"
        >
          <input
            type="checkbox"
            checked={selectedCalendarIds.includes(calendar.id)}
            onChange={() => onChange(toggleCalendarId(selectedCalendarIds, calendar.id))}
          />
          {calendar.summary}
        </label>
      ))}
    </fieldset>
  );
}
