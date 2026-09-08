import { useEffect, useRef, useState } from "react";
import type { TaskCaptureResult, TaskFileLinkResult, TaskTriageResult } from "../store/store";
import type { FileLinksWiring } from "./useFileLinksWiring";
import type { TriageWiring } from "./useTriageWiring";

// The capture box's note and file attachments, written after the item they
// belong to exists.
//
// **Why they are not part of the capture.** A capture carries exactly the
// fields `CaptureFields` names (`store/worker-client.ts`), and neither of
// these is one of them:
//
//  - `items.vault_path` is a real column, but `Core::capture` hardcodes it
//    to `None` — "#771: a note is pointed at from the item panel, never at
//    capture — there is nothing to name a note about yet". The column is
//    reachable only through `Core::triage`, and the wasm seam's
//    `CaptureFields` is `deny_unknown_fields`, so a `vaultPath` smuggled
//    into a capture would fail the whole capture rather than be ignored.
//  - a file link is not on the item at all. It is a row in `file_links`
//    naming an `item_id`, and the authority answers 400 "unknown item_id"
//    for one that does not exist yet (`handlers/file_links.rs`).
//
// So the box may *collect* both, and this is what writes them: one capture,
// then — once its result names the minted id — the two doors that already
// exist. Nothing about the capture mutation changes, and #771's sentence in
// the core stays true as written.
//
// **Why here, and not in the core.** ADR-0025 sinks decisions two clients
// must answer identically. This is not one: it is the order in which one
// client chooses to make three writes it could equally have made by hand.
// The phone draws no note or file affordance at all, so there is no second
// answer for it to disagree with.
//
// **Why keyed by seed.** `lastCapture` is a broadcast to every connected
// view, not a reply to this one, so "the capture I sent" is only knowable
// by the seed this session minted. The entry is dropped as it fires, which
// is also what makes a replayed broadcast inert — a seed already attached
// is a seed no longer in the map. `lastTriage` and `lastFileLinkWrite` are
// shared the same way, and are recognised the same way.
//
// **What is deliberately dropped.** A capture that comes back `failed` or
// `busy` attached nothing, because there is no item to attach it to; the
// box has kept the typed values for the retry (#222), which is the honest
// end state. Nothing here retries on its own.

/** What one capture wants attached once it has an id. `null` is "not asked
 * for" on both halves; a capture with two `null`s never reaches this hook's
 * map at all. */
export interface CaptureAttachments {
  /** A vault-relative path, already shape-checked by the box against
   * `obsidian/vault-uri.ts`'s `isValidVaultPath`. */
  vaultPath: string | null;
  /** A Dropbox-relative path, already normalized and shape-checked by the
   * box against `dropbox/file-link.ts`. */
  filePath: string | null;
}

export interface CaptureAttachmentsWiring {
  /** Records what the capture carrying `seed` should have attached once it
   * lands. Called by the submit that mints the seed, never later. */
  remember: (seed: string, attachments: CaptureAttachments) => void;
  /** The most recent follow-up write that did NOT land, as a sentence for
   * the capture box to render, or `null`. Cleared by the next submit that
   * asks for an attachment. */
  failure: string | null;
}

/** Whether a capture has anything to attach at all — the box's gate before
 * it calls `remember`, so a plain capture never touches the map. */
export function hasAttachments({ vaultPath, filePath }: CaptureAttachments): boolean {
  return vaultPath !== null || filePath !== null;
}

/** Both halves say the same two things: the capture itself is safe, and the
 * one thing that rode with it is not there. Never "try again" — the item is
 * open in the app and both affordances are on its panel. */
export const NOTE_ATTACH_FAILURE = "The item was captured, but its note link didn't go through.";
export const FILE_ATTACH_FAILURE = "The item was captured, but its file link didn't go through.";
/** Both halves of one capture failed. A second failure must not overwrite the
 * first and leave the operator told about half of what went wrong. */
export const BOTH_ATTACH_FAILURE =
  "The item was captured, but neither its note link nor its file link went through.";

export function useCaptureAttachments(
  triage: TriageWiring["triage"],
  fileLinks: FileLinksWiring,
  lastCapture: TaskCaptureResult | null,
  lastTriage: TaskTriageResult | null,
  lastFileLinkWrite: TaskFileLinkResult | null,
): CaptureAttachmentsWiring {
  // Refs, not state: writing either of these must not re-render, and both
  // are read only from the effects below on the tick a result arrives.
  const pending = useRef(new Map<string, CaptureAttachments>());
  // Each issued write remembers the capture it rode with, so a failure can be
  // shown only while that capture is still the most recent one — see the
  // derivation at the bottom of this hook.
  const issued = useRef(new Map<string, { captureSeed: string; message: string }>());
  const [failed, setFailed] = useState<{ captureSeed: string; message: string } | null>(null);

  useEffect(() => {
    if (lastCapture === null) {
      return;
    }
    const attachments = pending.current.get(lastCapture.seed);
    // Retired on ANY result, not just the one that attaches. A `failed` or
    // `busy` capture is the end of that seed — the box keeps what was typed
    // (#222) but a retry mints a fresh seed, so an entry left here is one
    // nothing will ever collect. Dropping it before either write below is
    // also what makes a replayed broadcast inert: a seed already attached is
    // a seed no longer in the map.
    if (attachments !== undefined) {
      pending.current.delete(lastCapture.seed);
    }
    if (lastCapture.kind !== "ok" || lastCapture.id === null || attachments === undefined) {
      return;
    }
    const itemId = lastCapture.id;
    const captureSeed = lastCapture.seed;
    if (attachments.vaultPath !== null) {
      // `destination: null` (#122) — pointing an item at a note moves it
      // through no stage, exactly as `NoteLink`'s own save does.
      issued.current.set(triage(itemId, null, { vaultPath: attachments.vaultPath }), {
        captureSeed,
        message: NOTE_ATTACH_FAILURE,
      });
    }
    if (attachments.filePath !== null) {
      issued.current.set(fileLinks.createFileLink(itemId, attachments.filePath), {
        captureSeed,
        message: FILE_ATTACH_FAILURE,
      });
    }
    // `triage`/`createFileLink` are stable for the life of the worker, and
    // re-running this on their identity would risk attaching twice.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [lastCapture]);

  // A result whose seed this hook issued is this hook's to report; every
  // other result on those two shared slots belongs to a panel or a screen
  // and is already rendered there. Written out twice rather than through a
  // hook defined in this body: two plain effects are the shape every other
  // `use*Wiring` here has, and the duplication is four lines.
  function report(result: { seed: string; kind: string } | null): void {
    if (result === null) {
      return;
    }
    const entry = issued.current.get(result.seed);
    if (entry === undefined) {
      return;
    }
    issued.current.delete(result.seed);
    if (result.kind !== "ok") {
      // A second failure from the SAME capture widens the sentence rather
      // than replacing it — otherwise the operator is told the file link
      // failed and never that the note did.
      setFailed((standing) =>
        standing !== null && standing.captureSeed === entry.captureSeed
          ? { captureSeed: entry.captureSeed, message: BOTH_ATTACH_FAILURE }
          : entry,
      );
    }
  }

  useEffect(() => {
    report(lastTriage);
  }, [lastTriage]);

  useEffect(() => {
    report(lastFileLinkWrite);
  }, [lastFileLinkWrite]);

  return {
    remember: (seed, attachments) => {
      // The previous capture's failure belongs to the previous capture, and
      // a submit is the moment it stops being the most recent thing that
      // happened. In an event handler, so it is an ordinary state write.
      setFailed(null);
      pending.current.set(seed, attachments);
    },
    // **Derived, not cleared.** A failure is shown only while the capture it
    // belongs to is still the most recent one, so a later capture retires it
    // by arriving — no effect has to reach out and null it, which is both a
    // cascading render and the thing that made the old sentence outlive its
    // item and greet an unrelated capture under a freshly emptied box.
    failure:
      failed !== null && lastCapture !== null && lastCapture.seed === failed.captureSeed
        ? failed.message
        : null,
  };
}
