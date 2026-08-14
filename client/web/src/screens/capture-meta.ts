// #208's pure half: turns the capture box's `Slider`/`Select` state into
// the wire's `CaptureFields` shape (`store/worker-client.ts`), the same
// split `capture-validation.ts`/`triage-form.ts` already use for their own
// screen logic — no React, no worker, unit-testable on its own.

import type { CaptureFields } from "../store/worker-client";
import { isValidDeadline, isValidScheduledDate } from "./urgency";

/** The capture box's local field state (`screens/CaptureBox.tsx`).
 * `energy`/`size` are `Slider` stop indices (`null` = not set, the resting
 * state); everything else is its control's own string value (`""` = not set),
 * which is what keeps this shape a direct read of the form rather than a
 * parsed one — parsing happens once, in `resolveCaptureFields`. */
export interface CaptureMeta {
  energy: number | null;
  size: number | null;
  context: string;
  description: string;
  projectId: string;
  /** The `Select`'s own value: `"0".."4"`, where `"0"` is the priority
   * vocabulary's own "none" (`screens/priority.ts`) and is what the control
   * rests at. */
  priority: string;
  deadline: string;
  scheduledDate: string;
}

export const EMPTY_CAPTURE_META: CaptureMeta = {
  energy: null,
  size: null,
  context: "",
  description: "",
  projectId: "",
  priority: "0",
  deadline: "",
  scheduledDate: "",
};

/** `Slider` index -> `hummingbird_domain::Size`'s own wire name, in the
 * slider's own left-to-right stop order — and, since #446, also the stop
 * labels themselves: `CaptureBox.tsx` renders this array.
 *
 * It used to have a twin. The slider displayed "normal" at the middle stop
 * while the wire said `"short"`, so `CaptureBox.tsx` kept a parallel
 * `CAPTURE_SIZE_STOPS` of display labels, hand-aligned with this one and
 * guarded only by a length assertion in `capture-meta.test.ts` — nothing
 * mechanical connected them, and a stop added to one and not the other
 * resolved to `undefined`, which reads downstream as "not set": a silently
 * dropped selection with no error anywhere. ADR-0024 made the display word
 * the wire word, which leaves nothing for a second array to hold. The guard
 * went with it: there are no longer two things that can disagree.
 *
 * An index past the end still resolves to `undefined` and still reads as
 * "not set" — that is the resting state, not a failure. */
export const CAPTURE_SIZE_NAMES: ReadonlyArray<"quick" | "normal" | "deep"> = [
  "quick",
  "normal",
  "deep",
];

/** `Slider` index -> `hummingbird_domain::Energy`'s own wire name, and the
 * stop labels too — energy's display words always did match the domain
 * vocabulary, so this array never had the twin `CAPTURE_SIZE_NAMES` did. */
export const CAPTURE_ENERGY_NAMES: ReadonlyArray<"low" | "medium" | "high"> = [
  "low",
  "medium",
  "high",
];

/** Resolves one capture box's `CaptureMeta` to the wire's `CaptureFields`:
 * an unset slider or an empty context select maps to `null` ("not set"),
 * never a default index or an empty string reaching the mutation — the
 * "optional, decided at mint time" contract this issue must not break. */
export function resolveCaptureFields(meta: CaptureMeta): CaptureFields {
  return {
    size: meta.size === null ? null : CAPTURE_SIZE_NAMES[meta.size],
    energy: meta.energy === null ? null : CAPTURE_ENERGY_NAMES[meta.energy],
    context: meta.context === "" ? null : meta.context,
    description: meta.description.trim() === "" ? null : meta.description.trim(),
    projectId: meta.projectId === "" ? null : meta.projectId,
    // **`"0"` resolves to not-sent, not to a sent zero.** The server's own
    // default for `priority` is 0, so sending it would be this client
    // asserting a value the reader never chose — and priority would be the
    // one capture field that could not be left undecided.
    priority: meta.priority === "0" ? null : Number(meta.priority),
    deadline: meta.deadline === "" ? null : meta.deadline,
    scheduledDate: meta.scheduledDate === "" ? null : meta.scheduledDate,
  };
}

/** Whatever is wrong with the typed fields, keyed by field — the capture
 * box's counterpart to `triage-form.ts`'s `triageDraftProblems`, with the same
 * messages, because they validate the same values against the same rules
 * (`is_valid_deadline` at the seam, and the authority behind it).
 *
 * Only the free-text dates can be wrong: every other field is a `Select` whose
 * options are the vocabulary, and the title is `capture-validation.ts`'s. */
export interface CaptureMetaProblems {
  deadline?: string;
  scheduledDate?: string;
}

export function captureMetaProblems(meta: CaptureMeta): CaptureMetaProblems {
  const problems: CaptureMetaProblems = {};
  if (meta.deadline.length > 0 && !isValidDeadline(meta.deadline)) {
    problems.deadline = "Use YYYY-MM-DD or YYYY-MM-DDTHH:MM";
  }
  if (meta.scheduledDate.length > 0 && !isValidScheduledDate(meta.scheduledDate)) {
    problems.scheduledDate = "Use YYYY-MM-DD";
  }
  return problems;
}
