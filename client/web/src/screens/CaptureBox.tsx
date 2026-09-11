import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Button } from "../components/core/Button";
import type { IconName } from "../components/core/Icon";
import { IconButton } from "../components/core/IconButton";
import { Combobox } from "../components/forms/Combobox";
import { DeadlineField } from "../components/forms/DeadlineField";
import { Select } from "../components/forms/Select";
import { Textarea } from "../components/forms/Textarea";
import { Slider } from "../components/forms/Slider";
import { Input } from "../components/forms/Input";
import { CAPTURE_INPUT_ID } from "../shell/capture-hotkey";
import {
  installDictationModel,
  isDictationApiPresent,
  probeDictationCapability,
  startLocalDictation,
  type DictationCapability,
  type DictationSession,
} from "../speech/local-dictation";
import { FILE_PATH_PROBLEM, isValidFilePath, normalizePastedPath } from "../dropbox/file-link";
import { derivePath, isValidVaultPath } from "../obsidian/vault-uri";
import { VAULT_PATH_PROBLEM } from "./triage-form";
import type { CaptureAttachments } from "../shell/useCaptureAttachments";
import type { ProjectDTO } from "../store/protocol";
import type { TaskCaptureResult } from "../store/store";
import type { CaptureFields } from "../store/worker-client";
import { freezeDraft, restoreDraft, spliceTranscript, type FrozenDraft } from "./capture-dictation";
import {
  CAPTURE_ENERGY_NAMES,
  CAPTURE_SIZE_NAMES,
  captureMetaProblems,
  EMPTY_CAPTURE_META,
  resolveCaptureFields,
  todayDeadline,
  type CaptureMeta,
} from "./capture-meta";
import { energyIcon, levelColor, sizeIcon } from "./size-energy";
import { canSubmitCapture } from "./capture-validation";
import { PRIORITY_OPTIONS } from "./priority";
import type { CaptureDestination } from "./capture-destination";

// The Energy/Size `Slider` stops are `capture-meta.ts`'s
// `CAPTURE_ENERGY_NAMES`/`CAPTURE_SIZE_NAMES` themselves, rendered directly.
// There used to be a second pair of arrays here holding display labels,
// because the middle size stop displayed "normal" while the wire said
// "short"; ADR-0024 made those the same word, so the display copy had
// nothing left to hold and went away, along with the length assertion in
// `capture-meta.test.ts` that was the only thing keeping the two aligned.
//
// The context list is NOT restated here, and is no longer even imported: the
// suggestions arrive as a prop, built by `field-vocabulary.ts`'s
// `contextSuggestions` from the operator's list plus the contexts live items actually
// carry, because the box has no store of its own to read them from. The list
// is still the one suggested copy this repo keeps standing, and the triage
// editor and the frontier's chip order still read it directly. It reaches a
// `Combobox` rather than a `Select` because context is an open vocabulary —
// that module's header carries the decision.

/** One of the three attachment disclosures. They are the same control three
 * times over — the word `Add` and a glyph, and the glyph is the noun — so
 * they are written once here rather than three times below.
 *
 * `carries` is the lit state the #782 link toggle already had as
 * `IconButton`'s `active`: a disclosure holding a value stays marked while
 * it is shut, or closing it would look like discarding it. `quiet` is the
 * design system's brand-tinted secondary, which is that treatment for a
 * `Button`.
 *
 * The label is the accessible name and the hover tooltip both, the same
 * contract the details chevron above carries — the glyph is unambiguous to
 * the eye and says nothing at all to a screen reader. */
function AttachToggle({
  icon,
  label,
  open,
  carries,
  onToggle,
}: {
  icon: IconName;
  label: string;
  open: boolean;
  carries: boolean;
  onToggle: () => void;
}) {
  return (
    <Button
      size="sm"
      variant={carries ? "quiet" : "secondary"}
      aria-expanded={open}
      aria-label={label}
      title={label}
      iconRight={icon}
      onClick={onToggle}
    >
      Add
    </Button>
  );
}

/** What the box did last, so the surface it sits on can say so. A popover
 * closes over whatever screen the person was on, so nothing else on screen
 * would show the capture landing — and reporting a fact ("Added to Triage")
 * is the honest alternative to a box that clears and says nothing. */
interface LastSubmit {
  destination: CaptureDestination;
  title: string;
}

export interface CaptureBoxProps {
  /** Enqueues one capture at `destination`'s stage. Never called with an
   * empty/whitespace-only draft: `canSubmitCapture` gates all three buttons here
   * first (#110's "an empty capture is refused client-side"), because
   * `Core::capture` has no opinion of its own and would enqueue it.
   * `fields` (#208) carries the Energy/Size/Context selections, already
   * resolved to the wire's vocabulary names by `capture-meta.ts`'s
   * `resolveCaptureFields` — never the slider's own indices or the select's
   * raw empty-string resting value. */
  onSubmit: (
    title: string,
    destination: CaptureDestination,
    fields: CaptureFields,
    /** The note and file the box collected, which are NOT capture fields and
     * cannot be — `useCaptureAttachments.ts` says why, and is what writes
     * them once the capture's own result names the minted id. Both `null`
     * for a capture that asked for neither, which is nearly all of them. */
    attachments: CaptureAttachments,
  ) => void;
  /** The Routes a capture can be filed under, for the Project select behind
   * "More details". `[]` on a device that has never synced — the select
   * still renders, offering "No project" alone, because an empty list is a
   * fact about the projects and not a reason to hide the control. */
  projects: ProjectDTO[];
  /** What the Context `Combobox` offers — `field-vocabulary.ts`'s
   * `contextSuggestions`, the suggested list unioned with the contexts live
   * items actually carry, so a context typed once here is offered the next
   * time instead of having to be retyped. A prop rather than a call in this
   * file because the items are the caller's state; the box has no store. */
  contextSuggestions: readonly string[];
  /** Bumped to move focus into the field — the shell's global capture hotkey
   * and its "New" button both land here. Focus is taken on mount too (this
   * component mounts when the popover opens, which IS the request), so
   * unlike the screen-level version this needs no first-render guard. */
  focusRequestId: number;
  /** The most recent capture result (`TaskState.lastCapture`) — what the
   * clear-on-ok rule below and the failure paragraph read. `null` until the
   * first capture resolves. */
  lastCapture: TaskCaptureResult | null;
  /** Dismisses whatever surface the box is mounted on, drawn in the field's
   * trailing slot beside the microphone. The close control lives here rather
   * than on the surface because a row holding nothing but an X cost more
   * vertical space above the field than the field itself uses, and the field
   * is the only thing the popover is for. Optional: a surface that cannot be
   * dismissed passes nothing and no control renders. */
  onClose?: () => void;
  /** Bumped by the shell (through `CapturePopover`) whenever an Escape while
   * dictating should cancel the session in place rather than close the
   * popover (#380) — the same "bumped counter" idiom `focusRequestId` already
   * uses, for the same reason: a plain boolean can't tell a second request
   * from a no-op. A bump while no session is live does nothing. */
  cancelDictationRequestId?: number;
  /** #771: the operator's Obsidian vault name, off the `obsidian-vault`
   * binding. `null` — unset, unread, or no binding at all — draws no note
   * disclosure whatsoever, the same rule `NoteLink` applies on the item
   * panel: nothing announces a vault that isn't there. */
  vaultName?: string | null;
  /** ADR-0036: present when there is file-link wiring behind this render,
   * carrying the one device-local fact a pasted path needs
   * (`normalizePastedPath`). Absent draws no file disclosure,
   * the same "a render with nothing to send it never offers what it cannot
   * do" contract every other write here carries. */
  fileLinks?: { localRoot: string | null };
  /** `useCaptureAttachments`'s report that a follow-up write did not land,
   * rendered beside `captureError`. The capture itself succeeded, so this is
   * never the capture's own failure and never reads as one. */
  attachmentFailure?: string | null;
  /** Reports every change in whether a dictation session is live, so the
   * shell's single Escape handler (`App.tsx`) can decide whether an Escape
   * means "cancel the dictation" or "close the popover" — see
   * `cancelDictationRequestId` above. */
  onDictatingChange?: (dictating: boolean) => void;
}

/** The resting capability, and the arm a browser without the API keeps
 * forever. A constant rather than `null`: "not yet probed" and "cannot" render
 * identically (no microphone), so a fourth state would be a distinction with
 * no reader. */
const NO_DICTATION: DictationCapability = {
  kind: "unsupported",
  reason: "This browser can't dictate on the device.",
};

/** #381's own state machine, held only while `dictation.kind ===
 * "setup-required"` — the arm #379 deliberately left rendering nothing. It
 * exists ONLY for the two-step gesture ADR-0022 Decision 5 made mandatory:
 * `closed` is the setup mic's resting state (nothing shown, nothing called);
 * the first tap moves to `explained` and calls nothing else; `installing`
 * covers the several seconds Decision 5 measured `install()` taking; and
 * `failed` carries the message for a rejected or `false` install, with the
 * download control left in place so the reader can try again. A successful
 * install never lands here at all — it re-probes and the branch that renders
 * this whole block (`dictation.kind === "setup-required"`) stops matching. */
type DictationSetupPhase =
  | { phase: "closed" }
  | { phase: "explained" }
  | { phase: "installing" }
  | { phase: "failed"; message: string };

const SETUP_CLOSED: DictationSetupPhase = { phase: "closed" };

/** The capture box — one input, three optional metadata controls, and the two
 * stages a capture may be born into (`capture-destination.ts`). Extracted
 * from `TriageScreen` when capture moved into the shell's popover
 * (`shell/CapturePopover.tsx`), which is now its only home: the box is
 * reachable from every screen, so pinning it to Triage bought nothing and
 * cost a second `<input>` carrying the same DOM id as the popover's.
 *
 * Owns the draft and the metadata, and nothing else — where a capture goes is
 * the caller's wiring, what a valid draft is stays in `capture-validation.ts`.
 *
 * ## Dictation (#379, ADR-0022)
 *
 * The microphone in the field's trailing slot is **pre-mutation UI state that
 * terminates at `setDraft(...)`**. Nothing about dictation crosses the wasm
 * seam: `client/core/`, `server/`, the worker protocol and the task model are
 * all untouched, and a dictated capture reaches `Core::capture` by exactly the
 * path a typed one does — the same `onSubmit`, the same `canSubmitCapture`
 * gate, the same raw string (#110). A later reader tempted to "fix" this by
 * moving transcript handling into `client/core` would be adding a modality the
 * domain has no use for. There is deliberately no modality flag on a capture.
 *
 * The recognizer itself is `speech/local-dictation.ts` and the splice is
 * `capture-dictation.ts`; what lives here is the session's lifecycle, the
 * caret, and the two rules that only exist because a field can be listening:
 *
 *  - **The field is `readOnly` while listening**, which makes a stale frozen
 *    draft *unrepresentable* rather than handled — the halves frozen at
 *    session start cannot go out of date if nothing else can edit the string.
 *    It still permits focus, caret movement and `keydown`, so the shell's
 *    hotkey contract is unaffected.
 *  - **Enter while listening stops the session and does not submit.** That is
 *    an explicit gate in the field's own `keydown` handler, NOT a consequence
 *    of `readOnly`, which does not suppress `keydown` at all. The last final
 *    result may not have arrived, and submitting half a sentence is the worst
 *    available outcome.
 *
 * **Backgrounding cancels — it does not finalize and commit** (#379's open
 * decision, taken here). Three reasons, in order of weight: a hidden page must
 * not hold a hot microphone, and `abort()` is how it is released; every other
 * ending here bumps the generation token *before* touching the recognizer, and
 * a session that must survive its own ending to deliver one trailing result
 * would be a second lifecycle shape for one edge case; and cancelling loses
 * far less than it sounds — the transcript already spliced into the field
 * STAYS there, so what is dropped is only the tail the recognizer had not yet
 * delivered, never the words the operator watched appear. Committing on
 * background would instead land text in a field nobody is looking at.
 * `cancelDictation` (Escape, #380) is the one session ending that rewrites
 * the draft on purpose, restoring it to what it said before the session
 * started rather than leaving the last splice in place — that is the whole
 * point of an explicit cancel, as opposed to backgrounding's silent one.
 *
 * A fifth ending, #367: a capture result landing `"ok"` while the NEXT
 * capture's dictation session is still live ends that session too, because
 * the render-phase clear-on-ok block already emptied the draft the session's
 * frozen halves were keyed to — a further transcript would splice onto those
 * stale halves and resurrect the capture just submitted. This one neither
 * finalizes nor restores: it just tears the session down (`endSession`,
 * same as backgrounding) and drops `frozenRef`, because the clear-on-ok
 * block's state is already correct.
 *
 * Two guards are load-bearing rather than hygiene. The generation token means
 * a callback from an ended session changes nothing. And the unmount cleanup
 * aborting the session is the only thing that releases the microphone when the
 * popover closes: `CapturePopover` returns `null` when closed, which unmounts
 * this box and would otherwise leave the recognizer running with nothing
 * receiving its results. */
export function CaptureBox({
  onSubmit,
  projects,
  contextSuggestions,
  focusRequestId,
  lastCapture,
  onClose,
  vaultName = null,
  fileLinks,
  attachmentFailure = null,
  cancelDictationRequestId,
  onDictatingChange,
}: CaptureBoxProps) {
  const [draft, setDraft] = useState("");
  const [meta, setMeta] = useState(EMPTY_CAPTURE_META);
  const [last, setLast] = useState<LastSubmit | null>(null);
  // The submit whose result has not come back yet — promoted to `last` (and
  // the box cleared) only once that result actually reports `"ok"`, below.
  const [inFlight, setInFlight] = useState<LastSubmit | null>(null);

  // By id, not a ref: `Input` (the design-system component) forwards no ref,
  // and `CAPTURE_INPUT_ID` exists precisely so the shell's hotkey and the
  // field can never drift apart.
  function focusField(): void {
    document.getElementById(CAPTURE_INPUT_ID)?.focus();
  }

  useEffect(() => {
    focusField();
  }, [focusRequestId]);

  // The same "by id, not a ref" idiom as `focusField`, hardened: the id is a
  // document-wide lookup, so what comes back is only known to be an element.
  function captureField(): HTMLInputElement | null {
    const element = document.getElementById(CAPTURE_INPUT_ID);
    return element instanceof HTMLInputElement ? element : null;
  }

  // Synchronous, in the initializer, and never re-read: a browser does not
  // grow a speech API mid-session. False means the async probe below never
  // runs at all — no promise, no post-render `setState`, and so no React
  // `act()` warning in the existing `CapturePopover.test.tsx` cases, which
  // mount this box under a jsdom with no speech API.
  const [apiPresent] = useState(isDictationApiPresent);
  const [dictation, setDictation] = useState<DictationCapability>(NO_DICTATION);
  const [setupPhase, setSetupPhase] = useState<DictationSetupPhase>(SETUP_CLOSED);
  const [listening, setListening] = useState(false);
  const [dictationError, setDictationError] = useState<string | null>(null);
  const sessionRef = useRef<DictationSession | null>(null);
  // The halves frozen at session start, held here too (alongside the local
  // `frozen` `startDictation` closes over for its own splices) so a cancel
  // arriving from outside — the shell's Escape branch — can restore from the
  // very same halves. Not a second saved copy of the draft: it's the one
  // `spliceTranscript` already reads. Only two things clear it:
  // `cancelDictation`, which restores from it first, and the #367 clear-on-ok
  // effect below, which ends a still-live session out from under a capture
  // that just landed and clears it WITHOUT restoring, because the clear-on-ok
  // render-phase block already put the draft in its correct end state. Every
  // other way a session ends (stop, an error, backgrounding, unmount) leaves
  // whatever the recognizer last spliced in place, so there is nothing to
  // restore and nothing to clear.
  const frozenRef = useRef<FrozenDraft | null>(null);
  // Bumped by everything that ends a session, BEFORE the recognizer is
  // touched, so a result or an error still in flight is inert by the time it
  // lands. Teardown is keyed on session identity instead (see `endSession`),
  // because the bump would otherwise make a session's own `onEnd` inert and
  // nothing would ever clear `listening`.
  const generationRef = useRef(0);
  // A range, not a single caret: a splice always collapses to `{start: end}`,
  // but a cancel (#380) restores a selection, which needs both ends set in
  // the same paint or the browser would show a collapsed caret for one frame.
  const pendingSelectionRef = useRef<{ start: number; end: number } | null>(null);

  useEffect(() => {
    if (!apiPresent) {
      return;
    }
    let live = true;
    void probeDictationCapability().then((capability) => {
      if (live) {
        setDictation(capability);
      }
    });
    return () => {
      live = false;
    };
  }, [apiPresent]);

  // `ready` only. ADR-0022 forbids rendering anything at all for
  // `unsupported` — not a disabled microphone, not one with a warning.
  const canDictate = dictation.kind === "ready";
  // #381's arm: actionable, but through the explain-then-download control
  // below, never through the ordinary "Dictate" mic `canDictate` renders.
  const setupRequired = dictation.kind === "setup-required";

  // The download control's own click handler (#381). `installDictationModel()`
  // is called FIRST, synchronously, before any `setState` — see ADR-0022
  // Decision 5 and the module header of `speech/local-dictation.ts`: the
  // browser call has to be the first thing that happens on this call stack or
  // the still-live click gesture it depends on is gone. Nothing before this
  // function runs an `await`, an effect, or a timer, and nothing ever will —
  // that is the whole point of the acceptance criterion this satisfies.
  function beginInstall(): void {
    const install = installDictationModel();
    setSetupPhase({ phase: "installing" });
    install
      .then((ok) => {
        if (!ok) {
          setSetupPhase({
            phase: "failed",
            message: "The speech model couldn't be installed. Typing still works.",
          });
          return;
        }
        return probeDictationCapability().then((capability) => {
          setDictation(capability);
          setSetupPhase(SETUP_CLOSED);
        });
      })
      .catch(() => {
        setSetupPhase({
          phase: "failed",
          message: "The speech model couldn't be installed. Typing still works.",
        });
      });
  }

  function endSession(mode: "stop" | "abort"): void {
    const session = sessionRef.current;
    generationRef.current += 1;
    if (!session) {
      return;
    }
    if (mode === "stop") {
      session.stop();
    } else {
      session.abort();
    }
  }

  function startDictation(): void {
    const field = captureField();
    // The halves are frozen ONCE, here, and every transcript in this session
    // splices between these same two — see `capture-dictation.ts` on why that
    // is what makes an interim result replace rather than accumulate.
    const frozen: FrozenDraft = freezeDraft(
      draft,
      field?.selectionStart ?? null,
      field?.selectionEnd ?? null,
    );
    frozenRef.current = frozen;
    setDictationError(null);
    const generation = generationRef.current + 1;
    generationRef.current = generation;
    // A `let` assigned after the call, not the `const` it looks like it wants
    // to be: `onEnd` closes over it, and reading a `const` from a callback the
    // seam could fire before it returns would be a TDZ crash rather than a
    // stale read.
    let session: DictationSession | null = null;
    session = startLocalDictation({
      onTranscript: (transcript) => {
        if (generation !== generationRef.current) {
          return;
        }
        const spliced = spliceTranscript(frozen, transcript);
        setDraft(spliced.value);
        pendingSelectionRef.current = { start: spliced.caret, end: spliced.caret };
      },
      onError: (error) => {
        if (generation !== generationRef.current) {
          return;
        }
        // ADR-0022 Decision 1: an error ends the session and the reader is
        // told. It is never retried against anything else.
        setDictationError(error.message);
      },
      onEnd: () => {
        // Keyed on identity, not on the generation: this must still run for
        // the session whose own ending bumped the generation, and must not run
        // for one that has already been replaced or unmounted.
        if (sessionRef.current !== session) {
          return;
        }
        sessionRef.current = null;
        setListening(false);
      },
    });
    sessionRef.current = session;
    setListening(true);
    // Focus goes back to the field the microphone tap took it from: `readOnly`
    // still allows focus, and Enter has to reach the field's own handler to
    // stop the session.
    field?.focus();
  }

  // Escape while dictating (#380): abort the session — same `endSession`
  // path backgrounding takes, so the generation token already makes a
  // trailing callback inert — then put the draft back exactly as it stood
  // the instant dictation started, selection included. "Exactly" is not a
  // copy: `restoreDraft` reads the same frozen halves every splice does, PLUS
  // the `selected` text `freezeDraft` set aside rather than dropped, so a
  // cancel with words selected restores them and re-selects the same range —
  // never `spliceTranscript`'s empty-transcript case, which is built to
  // collapse a selection (a live session replaces it) and would delete those
  // words rather than give them back.
  //
  // Guarded on `sessionRef`, not the `listening` state, for the same reason
  // `endSession` itself reads only refs: a stray bump (the shell's second
  // Escape, once the popover has already closed) must be inert the instant
  // it runs, not once a render has caught up.
  function cancelDictation(): void {
    if (!sessionRef.current) {
      return;
    }
    const frozen = frozenRef.current;
    endSession("abort");
    if (frozen) {
      const restored = restoreDraft(frozen);
      setDraft(restored.value);
      pendingSelectionRef.current = {
        start: restored.selectionStart,
        end: restored.selectionEnd,
      };
    }
    frozenRef.current = null;
  }

  // The shell's ask to cancel in place, forwarded down through
  // `CapturePopover` as a bumped counter — see `cancelDictationRequestId`'s
  // own doc for why a counter rather than a boolean. Runs on mount too (the
  // id starts at a fixed value), which is exactly `cancelDictation`'s own
  // `sessionRef` guard's job: nothing is live yet, so it is inert. Deliberately
  // keyed on the id alone, the same idiom `focusRequestId`'s effect above
  // uses: `cancelDictation` is a new closure every render but reads nothing
  // that isn't already re-read from refs and props on each call, so
  // re-running it on every render would be churn with no reader.
  useEffect(() => {
    cancelDictation();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [cancelDictationRequestId]);

  // Tells the shell whether a session is live, so its single Escape handler
  // can branch between "cancel the dictation" and "close the popover"
  // (`App.tsx`). Fires on every `listening` transition, mount included.
  useEffect(() => {
    onDictatingChange?.(listening);
  }, [listening, onDictatingChange]);

  // Backgrounding cancels — see this component's header for why, and for what
  // that does and does not lose. Bound only while listening, so a page hidden
  // with no session pays nothing.
  useEffect(() => {
    if (!listening) {
      return;
    }
    function onVisibilityChange(): void {
      if (document.hidden) {
        endSession("abort");
      }
    }
    document.addEventListener("visibilitychange", onVisibilityChange);
    return () => document.removeEventListener("visibilitychange", onVisibilityChange);
    // `listening` alone: `endSession` reads nothing but refs, so re-binding
    // this on every render would be churn with no reader.
  }, [listening]);

  // Load-bearing, not hygiene: the popover unmounts this box when it closes,
  // and the microphone would otherwise stay hot. `sessionRef` is cleared
  // BEFORE the abort, so the session's `onEnd` finds no match and no state is
  // set on a component that is going away.
  useEffect(() => {
    return () => {
      const session = sessionRef.current;
      generationRef.current += 1;
      sessionRef.current = null;
      session?.abort();
    };
  }, []);

  // The caret React would otherwise park at the end: the field is controlled,
  // so every splice rewrites `value`. A layout effect, so the caret is right
  // before the browser paints and never visibly jumps; no dependency array and
  // an early return, so ordinary typing — which sets no pending caret — is
  // never disturbed.
  useLayoutEffect(() => {
    const selection = pendingSelectionRef.current;
    if (selection === null) {
      return;
    }
    pendingSelectionRef.current = null;
    captureField()?.setSelectionRange(selection.start, selection.end);
  });

  // Shut on arrival, and shut again after a capture lands: the box's whole
  // job is that typing one line and pressing Enter is the fastest thing on
  // the screen, and a form that reopens to seven fields taxes the next
  // capture for a decision the last one happened to make.
  const [detailsOpen, setDetailsOpen] = useState(false);
  // The three attachment disclosures, all shut until asked and all shut
  // again after each capture (with `detailsOpen`). #782's Link came first
  // and kept its shape; the note and the file joined it when the three
  // became one row.
  //
  // **The two paths are held here, beside `meta` rather than in it.**
  // `CaptureMeta`'s whole contract is that `resolveCaptureFields` turns it
  // into the wire's `CaptureFields`, and neither of these is a member of
  // that shape — the seam would refuse a capture carrying one
  // (`deny_unknown_fields`) rather than ignore it. They leave through
  // `onSubmit`'s fourth argument instead; `useCaptureAttachments.ts` has
  // the whole story.
  const [linkOpen, setLinkOpen] = useState(false);
  const [noteOpen, setNoteOpen] = useState(false);
  const [notePath, setNotePath] = useState("");
  // Whether `notePath` is something the reader typed, or still the path the
  // app proposed from the draft. A proposal is shown so it can be judged,
  // and is thrown away if the disclosure is closed without touching it —
  // otherwise looking at what a note WOULD be called silently attaches one.
  // Any edit promotes it, and from then on closing keeps it, which is the
  // rule the other two disclosures follow for their typed values.
  const [noteTouched, setNoteTouched] = useState(false);
  const [fileOpen, setFileOpen] = useState(false);
  const [filePath, setFilePath] = useState("");

  // Defence in depth rather than a live message: both date fields are native
  // pickers, which cannot hold a date that does not exist, so this finds
  // nothing today. It runs anyway because it is the same gate triage's form
  // has, against the same rules the seam refuses on — and the day one of
  // these becomes free text again, the gate is already here.
  const metaProblems = captureMetaProblems(meta);

  // The two attachment paths, judged by the same functions their item-panel
  // twins use — `obsidian/vault-uri.ts` and `dropbox/file-link.ts` — and
  // reported with the same two messages, so neither surface can invent its
  // own account of what a bad path is. A file path is normalized first,
  // because pasting one out of a file manager is how it usually arrives.
  // An untouched proposal is not an answer: it is only ever sent once the
  // reader has edited it.
  const typedNotePath = noteTouched ? notePath.trim() : "";
  const noteProblem =
    typedNotePath !== "" && !isValidVaultPath(typedNotePath) ? VAULT_PATH_PROBLEM : undefined;
  const typedFilePath = normalizePastedPath(filePath, fileLinks?.localRoot ?? null);
  const fileProblem =
    typedFilePath !== "" && !isValidFilePath(typedFilePath) ? FILE_PATH_PROBLEM : undefined;

  // **A problem may never hide behind the disclosure that holds it.** Both
  // paths block every submit below, and both live inside two nested
  // disclosures, so a path typed and then closed away would disable all three
  // buttons and silence Enter with nothing on screen saying why. Whatever is
  // wrong therefore forces its own field, and the details block around it,
  // back open — the reader can shut it again the moment it is valid.
  const attachmentProblem = noteProblem !== undefined || fileProblem !== undefined;
  const detailsShown = detailsOpen || attachmentProblem;
  const noteFieldShown = noteOpen || noteProblem !== undefined;
  const fileFieldShown = fileOpen || fileProblem !== undefined;

  // A bad path blocks the capture rather than being dropped from it. That is
  // this box's existing answer, not a new one: #782's "a link name needs a
  // URL" already blocks all three buttons through `metaProblems`. Submitting
  // and silently discarding what someone typed is the alternative, and it is
  // worse — they would find out by opening the item.
  const canSubmit =
    canSubmitCapture(draft) &&
    Object.keys(metaProblems).length === 0 &&
    noteProblem === undefined &&
    fileProblem === undefined;

  // Issue #222's rule, applied to capture (#208 tripled what a failed capture
  // would discard: the title PLUS size, energy and context). The draft and
  // the three meta selections survive until a result actually reports `"ok"`;
  // while the write is in flight (no result yet) and after a `"failed"` one,
  // everything the reader typed and chose is still here to retry or amend.
  // The render-phase "adjusting state when a prop changes" pattern, guarded
  // on the result's own `seed`, so a broadcast already observed can never
  // clear a draft twice and a replayed/stale seed clears nothing at all. A
  // capture carries no item id, so there is no per-item keying to do — the
  // seed IS the identity.
  //
  // **Context is the one carve-out from that clear.** Adding three things for
  // the same place is the normal sitting — the whole reason the popover does
  // not close on submit — and re-picking `@errands` for each of them taxes
  // every capture after the first for a decision already made. Size, energy,
  // dates and project still clear, because those are genuinely per-item;
  // where you are is not. The stickiness is bounded by the box's own life: the
  // popover returns `null` when shut (`CapturePopover.tsx`), so the box
  // unmounts and the preserved context dies with it — there is no teardown to
  // write, and nothing survives to the next time capture is opened.
  /** Everything the disclosures hold, back to shut and empty — one function
   * so a fourth disclosure is reset where the other three are and cannot be
   * forgotten (#638 removed its second caller, `submit`'s demo arm). It does
   * NOT touch `meta`: the context carve-out is the caller's. */
  function clearDisclosures(): void {
    setDetailsOpen(false);
    setLinkOpen(false);
    setNoteOpen(false);
    setNotePath("");
    setNoteTouched(false);
    setFileOpen(false);
    setFilePath("");
  }

  const [processedCaptureSeed, setProcessedCaptureSeed] = useState<string | null>(null);
  if (lastCapture && lastCapture.seed !== processedCaptureSeed) {
    setProcessedCaptureSeed(lastCapture.seed);
    if (lastCapture.kind === "ok") {
      setDraft("");
      setMeta({ ...EMPTY_CAPTURE_META, context: meta.context });
      clearDisclosures();
      // The dictation failure goes with the draft it happened to. Left
      // standing, a "Nothing was heard." would sit under a freshly emptied box
      // describing a session two captures ago — the same stale-report failure
      // `captureError`'s `kind !== "ok"` guards against.
      setDictationError(null);
      if (inFlight) {
        setLast(inFlight);
        setInFlight(null);
      }
    }
  }

  // #367: a successful result can land while a session for the NEXT capture
  // is still live — the clear-on-ok block above just rewrote `draft` out
  // from under the halves that session froze at its own start, so a further
  // transcript would splice onto stale halves and resurrect the capture just
  // submitted. An effect, not the render-phase block above: ending a session
  // touches refs, and the render phase may only set state. Ended
  // unconditionally on session presence, not the generation token — the
  // token defeats a late *callback*, and this session is still genuinely
  // live, just holding a snapshot that has quietly gone stale. `endSession`
  // tears the recognizer down the same way backgrounding does; there is no
  // restore here (unlike `cancelDictation`) because the clear-on-ok state
  // above is already the correct end state — a later `cancelDictation`
  // (guarded on `sessionRef`) finds nothing left to restore. Keyed on the
  // seed, same as the render-phase block, not `lastCapture` identity: a
  // replayed broadcast of a seed already handled must not kill a live
  // next-capture session that was started after it.
  const endedCaptureSeedRef = useRef<string | null>(null);
  useEffect(() => {
    if (
      lastCapture?.kind === "ok" &&
      lastCapture.seed !== endedCaptureSeedRef.current &&
      sessionRef.current
    ) {
      endedCaptureSeedRef.current = lastCapture.seed;
      endSession("abort");
      frozenRef.current = null;
    }
  }, [lastCapture]);

  // Reviewer finding on issue #222: `TaskState.lastCapture` was written on
  // every `captureResult` and read by nothing, so a failed capture left the
  // reader with no signal at all. A capture has no pre-existing item to key
  // the error against, so it renders near the box itself. `kind !== "ok"`
  // overwrites itself on the next capture result, so a stale failure never
  // survives a later success. (A `!demo` guard once kept it out of the
  // fixture-only demo view; #638 removed that prop chain, since #457 left
  // nothing that could pass `true`.)
  const captureError =
    lastCapture && lastCapture.kind !== "ok"
      ? (lastCapture.error ?? "That capture didn't go through.")
      : null;

  // `overrides` is what the "Mint for today" square stamps over the form's
  // own meta at the moment of submit — resolved into `fields` synchronously,
  // because a state write would land a render too late for the `onSubmit` on
  // this same call stack. It is ALSO persisted with `setMeta`: on ok the box
  // clears anyway, but a failed capture keeps the form for retry, and without
  // this the override (the user's today choice) would be gone from `meta` —
  // a later Enter or "Mint action" would then silently submit the stale
  // deadline instead.
  function submit(destination: CaptureDestination, overrides: Partial<CaptureMeta> = {}) {
    if (!canSubmit) {
      return;
    }
    const merged = { ...meta, ...overrides };
    const fields = resolveCaptureFields(merged);
    if (Object.keys(overrides).length > 0) {
      setMeta(merged);
    }
    // A submit ends any live session, and this is what actually keeps a frozen
    // draft from going stale: `readOnly` stops the *reader* editing the field,
    // but #222's clear-on-ok rewrites `draft` from a capture result, and a
    // transcript spliced onto halves frozen before that clear would resurrect
    // the capture that was just submitted. The two buttons are deliberately
    // NOT disabled while listening — an explicit click on "Add to Triage" is
    // an unambiguous "this is what I meant", unlike the Enter this component's
    // header gates.
    if (listening) {
      endSession("abort");
    }
    // Neither of these is a capture field, and neither can be sent with the
    // capture — `useCaptureAttachments.ts` carries the whole reason. The box
    // hands them over and stops there, exactly as it hands over `fields`.
    const attachments: CaptureAttachments = {
      vaultPath: typedNotePath === "" ? null : typedNotePath,
      filePath: typedFilePath === "" ? null : typedFilePath,
    };
    // The raw string, not a trimmed one: #110's "the raw string reaches the
    // mutation unmodified" — `canSubmitCapture` decides *whether* to submit,
    // never *what* is submitted.
    onSubmit(draft, destination, fields, attachments);
    setInFlight({ destination, title: draft });
    // Focus stays in the field on purpose: capturing three things in a row is
    // the normal case, and the popover deliberately does not close on submit.
    focusField();
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-6)" }}>
      <div style={{ display: "flex", alignItems: "flex-end", gap: "var(--space-5)", flexWrap: "wrap" }}>
        <Input
          id={CAPTURE_INPUT_ID}
          // `min()`: see `layout.tsx`'s `Column`.
          style={{ flex: 1, minWidth: "min(260px, 100%)" }}
          // The label is the accessible name only — the placeholder already
          // asks the question on screen, and a "Capture" caption over a single
          // field inside a capture popover names the container rather than
          // telling the reader anything. `aria-label` rather than a
          // visually-hidden `<label>`: `Input` renders no such variant, and
          // adding one to the component library for a single caller would be
          // the larger change.
          aria-label="Capture"
          icon="feather"
          placeholder="What's on your mind?"
          value={draft}
          // Makes a stale frozen draft unrepresentable rather than handled —
          // see the header. It does NOT deliver the Enter contract below.
          readOnly={listening}
          // Both controls ride inside the field's own box, pinned right, in
          // the order the hand reaches for them: dictate belongs to the draft,
          // close belongs to the surface. `sm` (28px) is the size that clears
          // the `md` field's 36px interior.
          trailing={
            <>
              {canDictate ? (
                <IconButton
                  size="sm"
                  icon="mic"
                  // The design system's own toggled-on treatment (ember tint,
                  // brand foreground) — and the label changes with it, so the
                  // state is not carried by colour alone.
                  active={listening}
                  label={listening ? "Stop dictating" : "Dictate"}
                  onClick={() => (listening ? endSession("stop") : startDictation())}
                />
              ) : setupRequired ? (
                // #381: the setup-state mic. The first tap only opens the
                // explanation below — nothing is downloaded and no network
                // request is made until the hint's own control is clicked.
                <IconButton
                  size="sm"
                  icon="mic"
                  active={setupPhase.phase !== "closed"}
                  label="Set up dictation"
                  onClick={() => {
                    // A tap while the hint is already open (installing,
                    // failed, or already explained) must not reset it — only
                    // the closed->explained transition does anything.
                    if (setupPhase.phase === "closed") {
                      setSetupPhase({ phase: "explained" });
                    }
                  }}
                />
              ) : null}
              {/* An X inside a text field usually means "clear the text", so
                  the name has to do the disambiguating that the glyph cannot:
                  `label` is both the accessible name and the hover tooltip. */}
              {onClose ? <IconButton size="sm" icon="x" label="Close" onClick={onClose} /> : null}
            </>
          }
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            // `isComposing` guards an IME composition commit (e.g. an Enter
            // that confirms a candidate while typing Japanese/Chinese/Korean)
            // from being read as "submit" — that Enter belongs to the
            // composition, not to this form. Enter is the default gesture,
            // and the default destination is Triage; minting is a deliberate
            // click, never something a keystroke does by accident.
            if (event.key === "Enter" && !event.nativeEvent.isComposing) {
              // The explicit session-state gate. `readOnly` does not suppress
              // `keydown`, so without this an Enter mid-dictation would submit
              // whatever half-sentence had arrived so far.
              if (listening) {
                endSession("stop");
                return;
              }
              submit("triage");
            }
          }}
        />
        <div style={{ display: "flex", gap: "var(--space-4)", flexWrap: "wrap" }}>
          {/* Three solid squares that say what they are by colour and glyph
              alone: the two destinations, then the mint-for-today. This one is the inbox
              (`inbox` is triage's own icon in the design system's vocabulary,
              and `info` is the blue triage wears in `StageBadge`)... */}
          {/* All three carry the same explicit 36: `md`'s own box is 34, and
              `IconButton` spreads `style` last over its own sizing, so this
              is what makes each square exactly the `md` field's height. At
              `lg` they would be 44 and stick 8px above the row. The row's
              `alignItems: "flex-end"` then lines all four up. */}
          <IconButton
            size="md"
            style={{ height: 36, width: 36 }}
            variant="solid"
            tone="info"
            icon="inbox"
            label="Triage"
            disabled={!canSubmit}
            onClick={() => submit("triage")}
          />
          {/* ...and this one is the skip — CONTEXT.md's Mint, "landing in
              Ready", for something already startable; the item never sits in
              Triage at all. Brand orange, because minting is the gesture the
              hand learns and then stops reading; with both glyphs now bare,
              each `label` is the only place its gesture is named. */}
          <IconButton
            size="md"
            style={{ height: 36, width: 36 }}
            variant="solid"
            tone="accent"
            icon="plus"
            label="Mint action"
            disabled={!canSubmit}
            onClick={() => submit("ready")}
          />
          {/* The third square is the mint again, with today's date stamped
              as the deadline. Ember-700 — the accent's own family with more
              heat — because it is the same gesture plus a claim about the
              date, and `calendar-check` because that is what it adds (the
              Wear capture handoff of 2026-09-10 settled both, for the web,
              the phone and the watch alike; `flag` stays the deadline glyph
              in item metadata, `calendar` the scheduled date's). It
              overrides a deadline picked under "More details" rather than
              yielding to it: the button's name is a promise about the date,
              and a click that silently kept some other day would break it.
              Date-only, per `todayDeadline`. */}
          <IconButton
            size="md"
            style={{ height: 36, width: 36 }}
            variant="solid"
            tone="ember-deep"
            icon="calendar-check"
            label="Mint for today"
            disabled={!canSubmit}
            onClick={() => submit("ready", { deadline: todayDeadline(Date.now()) })}
          />
        </div>
      </div>
      {/* The three optional fields and the disclosure share one row. The
          toggle used to sit on a row of its own underneath, which spent a
          whole line of vertical space on one short label; pinned to the right
          of Context it costs none. */}
      <div style={{ display: "flex", alignItems: "flex-start", gap: "var(--space-5)" }}>
        <div
          style={{
            // `min-width: 0` so the grid may shrink inside the flex row rather
            // than pushing the toggle off the card's right edge.
            flex: 1,
            minWidth: 0,
            display: "grid",
            // `repeat(3, 1fr)` forced three columns at every width, so on a
            // phone each held ~110px and its contents spilled sideways.
            // `auto-fit` drops to two and then one as the room runs out; the
            // inner `min()` keeps the 160px track from being a floor of its own.
            gridTemplateColumns: "repeat(auto-fit, minmax(min(160px, 100%), 1fr))",
            gap: "var(--space-7)",
            alignItems: "start",
          }}
        >
          {/* The stop lists are the wire vocabularies themselves, and the
              glyphs and colours are derived from them by index — nothing here
              is a second copy that could disagree with the level it draws.
              Both start unset, which is what the ghost variants are for: the
              slider shows no thumb and no ramp colour until someone chooses,
              because deciding is mint-time work, not capture-time work. */}
          <Slider
            label="Energy"
            options={CAPTURE_ENERGY_NAMES}
            optionIcons={CAPTURE_ENERGY_NAMES.map(energyIcon)}
            optionColors={CAPTURE_ENERGY_NAMES.map(levelColor)}
            value={meta.energy}
            onChange={(energy) => setMeta({ ...meta, energy })}
          />
          <Slider
            label="Size"
            options={CAPTURE_SIZE_NAMES}
            optionIcons={CAPTURE_SIZE_NAMES.map(sizeIcon)}
            optionColors={CAPTURE_SIZE_NAMES.map(levelColor)}
            value={meta.size}
            onChange={(size) => setMeta({ ...meta, size })}
          />
          <Combobox
            label="Context"
            value={meta.context}
            onChange={(context) => setMeta({ ...meta, context })}
            suggestions={contextSuggestions}
            placeholder="Not set"
          />
        </div>
        {/* Everything a mint would ask, behind one control. The sentence that
            used to sit here said dates were "decided at mint time"; they can
            be decided here now, and the disclosure says so better than a
            caption could. Shut by default and shut again after each capture —
            see `detailsOpen`.

            The words "More details" are gone from the screen and the glyph
            carries them alone; `label` is both the accessible name and the
            hover tooltip, so nothing about what this opens is unreachable.
            The direction is the state, and it stays a rotated `chevron-down`
            rather than a `chevron-right` — one more glyph in `ICON_MAP` is an
            extension of the brand's named vocabulary, and this branch already
            declined to make one of those unasked. */}
        {/* A class, not an inline object: the padding that holds this level
            with the Context select has to be undone at 390px, where the three
            fields stack, and nothing in a stylesheet outranks a `style`
            attribute. Both forms live in `shell/responsive.css`. */}
        <div className="hb-capture-details-toggle">
          {/* The rotation rides on this span, not on `IconButton`'s `style`:
              that prop is spread last over the button's own `transform`, so
              writing one here would silently cost the press scale. Rotating
              the whole square box is indistinguishable from rotating the
              glyph inside it, and the press animation survives. */}
          <span
            style={{
              display: "inline-flex",
              transform: detailsShown ? "none" : "rotate(-90deg)",
              transition: "transform var(--dur-fast) var(--ease-flit)",
            }}
          >
            <IconButton
              icon="chevron-down"
              label="More details"
              aria-expanded={detailsShown}
              onClick={() => setDetailsOpen(!detailsShown)}
            />
          </span>
        </div>
      </div>
      {detailsShown ? (
        <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-5)" }}>
          <Textarea
            label="Description"
            rows={3}
            value={meta.description}
            placeholder="The only free-prose field — never a checklist"
            onChange={(event) => setMeta({ ...meta, description: event.target.value })}
          />
          <div
            style={{
              display: "grid",
              gridTemplateColumns: "repeat(auto-fit, minmax(min(160px, 100%), 1fr))",
              gap: "var(--space-5)",
              alignItems: "start",
            }}
          >
            <Select
              label="Project"
              size="sm"
              value={meta.projectId}
              onChange={(event) => setMeta({ ...meta, projectId: event.target.value })}
              options={[
                { value: "", label: "No project" },
                ...projects.map((project) => ({ value: project.id, label: project.name })),
              ]}
            />
            <Select
              label="Priority"
              size="sm"
              value={meta.priority}
              onChange={(event) => setMeta({ ...meta, priority: event.target.value })}
              options={PRIORITY_OPTIONS}
            />
            <DeadlineField
              value={meta.deadline}
              error={metaProblems.deadline}
              onChange={(deadline) => setMeta({ ...meta, deadline })}
            />
            <Input
              label="Scheduled date"
              size="sm"
              type="date"
              value={meta.scheduledDate}
              error={metaProblems.scheduledDate}
              onChange={(event) => setMeta({ ...meta, scheduledDate: event.target.value })}
            />
          </div>

          {/* The three things an item can point at, as one row of three
              identical controls — the same row, the same order and the same
              glyphs the item panel draws (`ItemPanel.tsx`), so the gesture is
              learned once and found twice.

              They live behind "More details" rather than beside the field:
              the box's whole job is that typing one line and pressing Enter
              is the fastest thing on screen, and three more controls in the
              resting state tax every capture for a decision almost none of
              them make. #782's Link toggle used to sit on its own always-
              visible row and came in here with the other two.

              Each is drawn only when it could be written: no vault name, no
              note (nothing announces a vault that isn't there); no file-link
              wiring, no file. The link needs neither, because it is a column
              on the item itself. */}
          <div style={{ display: "flex", gap: "var(--space-4)", flexWrap: "wrap" }}>
            <AttachToggle
              icon="link"
              label="Add a link"
              open={linkOpen}
              carries={meta.linkUrl.length > 0}
              onToggle={() => setLinkOpen(!linkOpen)}
            />
            {vaultName !== null ? (
              <AttachToggle
                icon="notebook-text"
                label="Add a note"
                open={noteFieldShown}
                carries={typedNotePath.length > 0}
                onToggle={() => {
                  // Opening proposes a path from what has been typed so far,
                  // exactly as `NoteLink`'s editor proposes one from the
                  // item's title — the same `derivePath`, and the same
                  // contract: it is a proposal, editable before it is ever
                  // stored. An empty or unproposable draft opens empty.
                  // Closing an untouched proposal throws it away, and
                  // re-opening re-derives it from whatever the draft says by
                  // then, so a retitled capture never carries a note named
                  // for the title it had a moment ago.
                  if (!noteOpen) {
                    if (!noteTouched) {
                      setNotePath(derivePath(draft) ?? "");
                    }
                  } else if (!noteTouched) {
                    setNotePath("");
                  }
                  setNoteOpen(!noteOpen);
                }}
              />
            ) : null}
            {fileLinks ? (
              <AttachToggle
                icon="dropbox"
                label="Add a file"
                open={fileFieldShown}
                carries={typedFilePath.length > 0}
                onToggle={() => setFileOpen(!fileOpen)}
              />
            ) : null}
          </div>

          {/* #782's Link pair. Emptying the URL clears the name with it, in
              the form and not only in the patch, so what is left cannot read
              as "a name beside no URL". */}
          {linkOpen ? (
            <div
              style={{
                display: "grid",
                gridTemplateColumns: "repeat(auto-fit, minmax(min(200px, 100%), 1fr))",
                gap: "var(--space-5)",
                alignItems: "start",
              }}
            >
              <Input
                label="URL"
                size="sm"
                type="url"
                inputMode="url"
                value={meta.linkUrl}
                placeholder="https://"
                onChange={(event) => setMeta({ ...meta, linkUrl: event.target.value })}
              />
              <Input
                label="Link name"
                size="sm"
                value={meta.linkLabel}
                error={metaProblems.linkLabel}
                placeholder="Shown as the host when empty"
                onChange={(event) => setMeta({ ...meta, linkLabel: event.target.value })}
              />
            </div>
          ) : null}

          {noteFieldShown ? (
            <Input
              label="Vault path"
              size="sm"
              icon="notebook-text"
              value={notePath}
              error={noteProblem}
              placeholder="Hummingbird/Knee rehab.md"
              onChange={(event) => {
                setNotePath(event.target.value);
                setNoteTouched(true);
              }}
            />
          ) : null}

          {fileFieldShown ? (
            <Input
              label="File path"
              size="sm"
              icon="dropbox"
              value={filePath}
              error={fileProblem}
              placeholder="Finance/2026/receipt.pdf"
              onChange={(event) => setFilePath(event.target.value)}
            />
          ) : null}
        </div>
      ) : null}
      {setupRequired && setupPhase.phase !== "closed" ? (
        // #381: the explanation, and its own download control — the ONLY
        // thing that calls `installDictationModel` (`beginInstall`, above).
        // A `div` of its own rather than sharing `dictationError`'s `<p>`:
        // this state carries a control, not just a sentence.
        <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
          {setupPhase.phase === "failed" ? (
            // `role="alert"` + the danger token, matching `dictationError`/
            // `captureError` below: this is a failure report like theirs, not
            // the explanation/progress text the other phases render.
            <p
              role="alert"
              style={{ font: "var(--type-body-sm)", color: "var(--status-danger-fg)", margin: 0 }}
            >
              {setupPhase.message}
            </p>
          ) : (
            <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)", margin: 0 }}>
              {setupPhase.phase === "installing"
                ? "Downloading the on-device speech model…"
                : "Local speech recognition needs a one-time download before dictation works."}
            </p>
          )}
          {setupPhase.phase !== "installing" ? (
            <Button size="sm" variant="secondary" onClick={beginInstall}>
              Download speech model
            </Button>
          ) : null}
        </div>
      ) : null}
      {dictationError ? (
        // ADR-0022 Decision 1's "the session ends and the user is told", in
        // its own slot rather than sharing the capture failure's: they are
        // separate results, and a dictation that failed must not read as a
        // capture that did. `role="alert"` for the same reason as below —
        // nothing else on the page changes when it appears.
        <p
          role="alert"
          style={{ font: "var(--type-body-sm)", color: "var(--status-danger-fg)", margin: 0 }}
        >
          {dictationError}
        </p>
      ) : null}
      {captureError ? (
        // `role="alert"`: this paragraph renders only once a write has
        // already failed, so it appears with no other change on the page —
        // colour alone would never reach a screen reader.
        <p
          role="alert"
          style={{ font: "var(--type-body-sm)", color: "var(--status-danger-fg)", margin: 0 }}
        >
          {captureError}
        </p>
      ) : null}
      {attachmentFailure ? (
        // Its own paragraph, never folded into `captureError` above: that one
        // says the capture did not happen, and this one says it did. A reader
        // who conflates them would go looking for an item that is already
        // there. `role="alert"` for the same reason as the others — it
        // appears with no other change on the page.
        <p
          role="alert"
          style={{ font: "var(--type-body-sm)", color: "var(--status-danger-fg)", margin: 0 }}
        >
          {attachmentFailure}
        </p>
      ) : null}
      {last ? (
        <p
          // `aria-live`: the box clears and stays open, so this line is the
          // only report that anything happened — a screen reader has to hear
          // it without moving focus off the field.
          aria-live="polite"
          style={{
            font: "var(--type-body-sm)",
            color: "var(--text-secondary)",
            margin: 0,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {last.destination === "ready" ? "Minted into Ready" : "Added to Triage"} — {last.title}
        </p>
      ) : null}
    </div>
  );
}
