import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Button } from "../components/core/Button";
import { IconButton } from "../components/core/IconButton";
import { Combobox } from "../components/forms/Combobox";
import { DeadlineField } from "../components/forms/DeadlineField";
import { Select } from "../components/forms/Select";
import { Textarea } from "../components/forms/Textarea";
import { Slider } from "../components/forms/Slider";
import { Input } from "../components/forms/Input";
import { CAPTURE_INPUT_ID } from "../shell/capture-hotkey";
import {
  isDictationApiPresent,
  probeDictationCapability,
  startLocalDictation,
  type DictationCapability,
  type DictationSession,
} from "../speech/local-dictation";
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
} from "./capture-meta";
import { energyIcon, levelColor, sizeIcon } from "./size-energy";
import { canSubmitCapture } from "./capture-validation";
import { CONTEXTS } from "./field-vocabulary";
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
// The context list is NOT restated here. `screens/field-vocabulary.ts`'s
// `CONTEXTS` is the one copy this branch left standing, and capture, the
// triage editor and the frontier's chip order all read it. It reaches a
// `Combobox` rather than a `Select` because context is an open vocabulary —
// that module's header carries the decision.

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
   * empty/whitespace-only draft: `canSubmitCapture` gates both buttons here
   * first (#110's "an empty capture is refused client-side"), because
   * `Core::capture` has no opinion of its own and would enqueue it.
   * `fields` (#208) carries the Energy/Size/Context selections, already
   * resolved to the wire's vocabulary names by `capture-meta.ts`'s
   * `resolveCaptureFields` — never the slider's own indices or the select's
   * raw empty-string resting value. */
  onSubmit: (title: string, destination: CaptureDestination, fields: CaptureFields) => void;
  /** The Routes a capture can be filed under, for the Project select behind
   * "More details". `[]` in demo mode and on a device that has never synced —
   * the select still renders, offering "No project" alone, because an empty
   * list is a fact about the projects and not a reason to hide the control. */
  projects: ProjectDTO[];
  /** Demo mode has no worker behind it, so no `captureResult` will ever
   * arrive: the demo arm clears on submit (the fixture queue IS the
   * acknowledgement) and must never wear a stale failure from a previous
   * real session. */
  demo: boolean;
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
 * Two guards are load-bearing rather than hygiene. The generation token means
 * a callback from an ended session changes nothing. And the unmount cleanup
 * aborting the session is the only thing that releases the microphone when the
 * popover closes: `CapturePopover` returns `null` when closed, which unmounts
 * this box and would otherwise leave the recognizer running with nothing
 * receiving its results. */
export function CaptureBox({
  onSubmit,
  projects,
  demo,
  focusRequestId,
  lastCapture,
  onClose,
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
  const [listening, setListening] = useState(false);
  const [dictationError, setDictationError] = useState<string | null>(null);
  const sessionRef = useRef<DictationSession | null>(null);
  // The halves frozen at session start, held here too (alongside the local
  // `frozen` `startDictation` closes over for its own splices) so a cancel
  // arriving from outside — the shell's Escape branch — can restore from the
  // very same halves. Not a second saved copy of the draft: it's the one
  // `spliceTranscript` already reads. Only `cancelDictation` clears it — every
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

  // `ready` only. `setup-required` is actionable but its control is #381's, and
  // ADR-0022 forbids rendering anything at all for `unsupported` — not a
  // disabled microphone, not one with a warning.
  const canDictate = dictation.kind === "ready";

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

  // Defence in depth rather than a live message: both date fields are native
  // pickers, which cannot hold a date that does not exist, so this finds
  // nothing today. It runs anyway because it is the same gate triage's form
  // has, against the same rules the seam refuses on — and the day one of
  // these becomes free text again, the gate is already here.
  const metaProblems = captureMetaProblems(meta);
  const canSubmit = canSubmitCapture(draft) && Object.keys(metaProblems).length === 0;

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
  const [processedCaptureSeed, setProcessedCaptureSeed] = useState<string | null>(null);
  if (lastCapture && lastCapture.seed !== processedCaptureSeed) {
    setProcessedCaptureSeed(lastCapture.seed);
    if (lastCapture.kind === "ok") {
      setDraft("");
      setMeta(EMPTY_CAPTURE_META);
      setDetailsOpen(false);
      // The dictation failure goes with the draft it happened to. Left
      // standing, a "Nothing was heard." would sit under a freshly emptied box
      // describing a session two captures ago — the same stale-report failure
      // `!demo` guards `captureError` against.
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
  // (guarded on `sessionRef`) finds nothing left to restore.
  useEffect(() => {
    if (lastCapture?.kind === "ok" && sessionRef.current) {
      endSession("abort");
      frozenRef.current = null;
    }
  }, [lastCapture]);

  // Reviewer finding on issue #222: `TaskState.lastCapture` was written on
  // every `captureResult` and read by nothing, so a failed capture left the
  // reader with no signal at all. A capture has no pre-existing item to key
  // the error against, so it renders near the box itself; `!demo` keeps it
  // out of the fixture-only demo view, which never issues a real capture and
  // so must never wear a stale one from a previous real session.
  // `kind !== "ok"` overwrites itself on the next capture result, so a stale
  // failure never survives a later success.
  const captureError =
    !demo && lastCapture && lastCapture.kind !== "ok"
      ? (lastCapture.error ?? "That capture didn't go through.")
      : null;

  function submit(destination: CaptureDestination) {
    if (!canSubmit) {
      return;
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
    if (demo) {
      // No `captureResult` is coming — the caller's fixture queue IS the
      // acknowledgement, so the demo arm clears and reports right away.
      onSubmit(draft, destination, resolveCaptureFields(meta));
      setLast({ destination, title: draft });
      setDraft("");
      setMeta(EMPTY_CAPTURE_META);
      setDetailsOpen(false);
      focusField();
      return;
    }
    // The raw string, not a trimmed one: #110's "the raw string reaches the
    // mutation unmodified" — `canSubmitCapture` decides *whether* to submit,
    // never *what* is submitted.
    onSubmit(draft, destination, resolveCaptureFields(meta));
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
          {/* The two destinations, drawn as what they are rather than as two
              equal sentences: this one is the inbox (`inbox` is triage's own
              icon in the design system's vocabulary)... */}
          <Button
            size="md"
            variant="secondary"
            iconLeft="inbox"
            disabled={!canSubmit}
            onClick={() => submit("triage")}
          >
            Triage
          </Button>
          {/* ...and this one is the skip — CONTEXT.md's Mint, "landing in
              Ready", for something already startable; the item never sits in
              Triage at all. A bare `+` because minting is the gesture the
              hand learns and then stops reading, and the accessible name
              still says the whole thing. */}
          <IconButton
            size="lg"
            variant="solid"
            icon="plus"
            label="Mint action"
            disabled={!canSubmit}
            onClick={() => submit("ready")}
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
            suggestions={CONTEXTS}
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
              transform: detailsOpen ? "none" : "rotate(-90deg)",
              transition: "transform var(--dur-fast) var(--ease-flit)",
            }}
          >
            <IconButton
              icon="chevron-down"
              label="More details"
              aria-expanded={detailsOpen}
              onClick={() => setDetailsOpen(!detailsOpen)}
            />
          </span>
        </div>
      </div>
      {detailsOpen ? (
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
