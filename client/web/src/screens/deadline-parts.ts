// Splitting a deadline into the two controls that edit it, and joining them
// back into the one string the wire carries.
//
// A deadline is `YYYY-MM-DD` or `YYYY-MM-DDTHH:MM` — one field, minute
// precision, no timezone (`hummingbird_domain::is_valid_deadline`). The form
// edits it with a date picker and an optional time picker, and this is the
// whole of the translation between the two shapes.
//
// **Not `rules/deadline-picker.ts`.** That module turns a picked date into a
// *duration* for a rule's lead time; this one is about the item's own deadline
// value, which is an absolute civil date-time. Same words, different wire.
//
// **Malformed values pass straight through.** An item captured before this
// existed — or by a skill, or by hand — may carry free text in `deadline`, and
// a picker that silently emptied the field on load would delete it the moment
// anything else on the form was saved. It comes back out of `joinDeadline`
// unchanged instead, and `triageDraftProblems` is what says it cannot be sent.

export interface DeadlineParts {
  /** `YYYY-MM-DD`, or the whole raw value when it is neither shape — which is
   * what keeps a legacy free-text deadline visible and intact. */
  date: string;
  /** `HH:MM`, or `null` when the deadline names a whole day. */
  time: string | null;
}

const DATE = /^\d{4}-\d{2}-\d{2}$/;
const DATE_TIME = /^(\d{4}-\d{2}-\d{2})T(\d{2}:\d{2})$/;

export function splitDeadline(value: string): DeadlineParts {
  const dateTime = DATE_TIME.exec(value);
  if (dateTime) {
    return { date: dateTime[1], time: dateTime[2] };
  }
  return { date: value, time: null };
}

/** The inverse, with two rules the form leans on: an empty date is an empty
 * deadline (clearing the date clears the whole value, time included — a time
 * with no day is not a deadline), and a shape this module does not recognise
 * is returned as it came. */
export function joinDeadline(date: string, time: string | null): string {
  if (date === "") {
    return "";
  }
  if (time === null || time === "") {
    return date;
  }
  return DATE.test(date) ? `${date}T${time}` : date;
}
