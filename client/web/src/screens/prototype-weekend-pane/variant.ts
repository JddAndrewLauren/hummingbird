// PROTOTYPE — throwaway. Delete with the rest of this directory (#122).

import type { WeekendWindow } from "./weekend";

export type PaneSlot = "aside" | "banner";

export interface VariantProps {
  /** The merged window — already deduped, already grouped by day. Every
   * variant renders the SAME answer; they disagree only about its shape. */
  window: WeekendWindow;
  nowMs: number;
  /** `null` on a device that never connected a calendar: half the answer is
   * missing and the pane has to say so rather than read as a quiet weekend. */
  calendar: { asOf: string; stale: boolean } | null;
  /** The stub mutation. `null` clears the do-date. */
  onPlan: (itemId: string, dayKey: string | null) => void;
}
