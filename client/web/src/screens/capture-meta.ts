// #208's pure half: turns the capture box's `Slider`/`Select` state into
// the wire's `CaptureFields` shape (`store/worker-client.ts`), the same
// split `capture-validation.ts`/`triage-form.ts` already use for their own
// screen logic — no React, no worker, unit-testable on its own.

import type { CaptureFields } from "../store/worker-client";

/** The capture box's local Energy/Size/Context state
 * (`screens/CaptureBox.tsx`). `energy`/`size` are `Slider` stop indices
 * (`null` = not set, the resting state); `context` is the `Select`'s own
 * value (`""` = not set). */
export interface CaptureMeta {
  energy: number | null;
  size: number | null;
  context: string;
}

export const EMPTY_CAPTURE_META: CaptureMeta = { energy: null, size: null, context: "" };

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
  };
}
