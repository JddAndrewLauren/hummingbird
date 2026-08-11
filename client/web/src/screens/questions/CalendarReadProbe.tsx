import type { CalendarReadDTO } from "../../store/protocol";
import type { QuestionInputs } from "./contract";

// Issue #267's seam-proof: `QuestionInputs.calendarReads` needs a real
// reader before it can be called done — "an exported, unit-tested,
// never-wired seam is this repo's signature defect" (the Agent Brief's own
// words; see `client/web/src/screens/waste-pane/` for the pane-read arm's
// version of the same proof at #245). This is deliberately NOT a standing
// question: it is not in `registry.ts`'s `StandingQuestion` union, it is not
// mounted by `RankedRegion`, and it renders nothing beyond one plain line of
// text. #122 (the weekend-plans pane) is the real, registered consumer this
// arm exists for; this component's only job is to be a genuine caller of it
// today, proven by `CalendarReadProbe.test.tsx` and by
// `NowScreen.test.tsx`'s "the calendar-reads arm (#267)" suite.
//
// DELETE THIS FILE, and the two test mounts above, once #122 registers the
// real calendar-lane question — the same fold-in the `prototype-*-pane/`
// directories' own headers mark for themselves. Its only job is to keep the
// seam demonstrably wired until then.

/** The collapsed-to-one-line description of one calendar-events read —
 * pure, so the three states (`"not requested yet"` / `"not_read"` /
 * `"read"`, empty or not) are each independently testable without mounting
 * anything. */
export function describeCalendarRead(read: CalendarReadDTO | undefined): string {
  if (read === undefined) {
    // Covers both "nobody has asked yet" and a dropped "busy" answer — the
    // worker never delivers the latter (see `protocol.ts`'s `calendarEvents`
    // doc), so this side cannot and need not tell them apart.
    return "Not requested yet";
  }
  if (read.state === "not_read") {
    return "This device has never synced its calendar";
  }
  if (read.events.length === 0) {
    return "No events in range";
  }
  return read.events.map((event) => event.title).join(", ");
}

export function CalendarReadProbe({
  requestKey,
  inputs,
}: {
  /** Which entry of `inputs.calendarReads` to read — the caller-chosen
   * request key `useCalendarEventsWiring.ts`'s requests carry. */
  requestKey: string;
  inputs: QuestionInputs;
}) {
  return <p data-testid="calendar-read-probe">{describeCalendarRead(inputs.calendarReads[requestKey])}</p>;
}
