import type { TaskStageName } from "../store/protocol";

/** The two stages a *new* capture may be born into, and the whole closed
 * vocabulary the capture box offers.
 *
 * `"triage"` is capture's own default and what the box has always sent: a
 * capture is not an action yet, and deciding what it is is triage-time work
 * (CONTEXT.md's Stage — "Triage and Grilling are *pre-action* by
 * definition").
 *
 * `"ready"` is the skip: the glossary's **Mint** — "create the Action record
 * for a defined segment … landing in Ready" — for the case where the thing
 * being typed is already a concrete, startable action and a round-trip
 * through Triage would only be ceremony. It reaches the authority as one
 * ordinary `Core::capture` at that stage, NOT as a capture followed by a
 * triage: `Core::capture` already takes the stage the item is created at, so
 * there is one queued mutation, one id, and nothing to rebase between two
 * halves of one gesture.
 *
 * Deliberately NOT `TriageDestinationName` (`"grilling" | "ready"`), which is
 * a different question about a different item: that vocabulary is where an
 * *existing* capture may be promoted TO, and it has no `"triage"` member
 * because staying put is not a destination. This one has no `"grilling"`
 * member for the mirror-image reason — fog is found by looking at a capture
 * later, never declared at the moment of typing it. */
export type CaptureDestination = Extract<TaskStageName, "triage" | "ready">;
