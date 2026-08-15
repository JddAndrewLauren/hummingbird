// #208's pure half: turns the capture box's `Slider`/`Select` state into
// the wire's `CaptureFields` shape (`store/worker-client.ts`), the same
// split `capture-validation.ts`/`triage-form.ts` already use for their own
// screen logic — no React, no worker, unit-testable on its own.
//
// **Split at M1-2 (ADR-0025, #141/#500).** The slider-index/`""`-sentinel
// mapping below (`resolveCaptureFields`, `CAPTURE_SIZE_NAMES`/
// `CAPTURE_ENERGY_NAMES`) is the form-adapter half and stays TS — it is a
// rendering concern (which stop a slider is resting on), not a decision two
// clients must answer identically. Two pieces of it WERE decisions and sank
// into `hummingbird_core::decisions::capture`: the `"0"` -> "not sent"
// priority rule (`priorityFromSelect`, called from `resolveCaptureFields`
// below) and `captureMetaProblems` itself, now a thin wrapper over the
// seam — see `decisions/seam.ts`.

import type { CaptureFields } from "../store/worker-client";
import {
  captureMetaProblems as captureMetaProblemsFromSeam,
  priorityFromSelect,
  type CaptureMetaProblems,
} from "../decisions/seam";

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
    // `"0"` -> "not sent" is a decision, not a slider-index lookup — it
    // sank into `hummingbird_core::decisions::capture::priority_from_select`
    // at M1-2. See that function's doc comment for why.
    priority: priorityFromSelect(meta.priority),
    deadline: meta.deadline === "" ? null : meta.deadline,
    scheduledDate: meta.scheduledDate === "" ? null : meta.scheduledDate,
  };
}

/** Whatever is wrong with the typed fields, keyed by field — the capture
 * box's counterpart to `triage-form.ts`'s `triageDraftProblems`, with the
 * same messages, because both now call the same
 * `hummingbird_core::decisions::capture::capture_meta_problems` through the
 * seam (M1-2, #500) — the two hand-copied strings this used to carry
 * (`capture-meta.ts:106,109` vs `triage-form.ts:104,107`, before the sink)
 * cannot drift apart because there is only the one function left to read.
 *
 * Only the free-text dates can be wrong: every other field is a `Select`
 * whose options are the vocabulary, and the title is
 * `capture-validation.ts`'s. */
export type { CaptureMetaProblems };

export function captureMetaProblems(meta: CaptureMeta): CaptureMetaProblems {
  return captureMetaProblemsFromSeam(meta.deadline, meta.scheduledDate);
}
