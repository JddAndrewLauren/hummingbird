import { useEffect, useState } from "react";
import { Button } from "../components/core/Button";
import { Select } from "../components/forms/Select";
import { Slider } from "../components/forms/Slider";
import { Input } from "../components/forms/Input";
import { CAPTURE_INPUT_ID } from "../shell/capture-hotkey";
import type { TaskCaptureResult } from "../store/store";
import type { CaptureFields } from "../store/worker-client";
import { EMPTY_CAPTURE_META, resolveCaptureFields } from "./capture-meta";
import { canSubmitCapture } from "./capture-validation";
import type { CaptureDestination } from "./capture-destination";

const CONTEXTS = ["@home", "@computer", "@phone", "@errands", "@garden", "@waiting"];

/** The capture box's Energy/Size `Slider` stops, left to right — the display
 * labels, which are NOT always the domain vocabulary ("normal" is the middle
 * size stop; `hummingbird_domain::Size` calls it `short`).
 *
 * Exported only so `capture-meta.test.ts` can pin them against that module's
 * own `CAPTURE_SIZE_NAMES`/`CAPTURE_ENERGY_NAMES`, which are indexed by the
 * raw slider index and hand-aligned with these. Nothing mechanical connects
 * the two sides: a fourth stop added here and not there resolves to
 * `undefined`, which reads downstream as "not set" — a dropped selection
 * with no error anywhere. The length assertion is that missing mechanism. */
export const CAPTURE_ENERGY_STOPS: string[] = ["low", "medium", "high"];
export const CAPTURE_SIZE_STOPS: string[] = ["quick", "normal", "deep"];

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
}

/** The capture box — one input, three optional metadata controls, and the two
 * stages a capture may be born into (`capture-destination.ts`). Extracted
 * from `TriageScreen` when capture moved into the shell's popover
 * (`shell/CapturePopover.tsx`), which is now its only home: the box is
 * reachable from every screen, so pinning it to Triage bought nothing and
 * cost a second `<input>` carrying the same DOM id as the popover's.
 *
 * Owns the draft and the metadata, and nothing else — where a capture goes is
 * the caller's wiring, what a valid draft is stays in `capture-validation.ts`. */
export function CaptureBox({ onSubmit, demo, focusRequestId, lastCapture }: CaptureBoxProps) {
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

  const canSubmit = canSubmitCapture(draft);

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
      if (inFlight) {
        setLast(inFlight);
        setInFlight(null);
      }
    }
  }

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
    if (demo) {
      // No `captureResult` is coming — the caller's fixture queue IS the
      // acknowledgement, so the demo arm clears and reports right away.
      onSubmit(draft, destination, resolveCaptureFields(meta));
      setLast({ destination, title: draft });
      setDraft("");
      setMeta(EMPTY_CAPTURE_META);
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
          style={{ flex: 1, minWidth: 260 }}
          label="Capture"
          icon="feather"
          placeholder="What's on your mind?"
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            // `isComposing` guards an IME composition commit (e.g. an Enter
            // that confirms a candidate while typing Japanese/Chinese/Korean)
            // from being read as "submit" — that Enter belongs to the
            // composition, not to this form. Enter is the default gesture,
            // and the default destination is Triage; minting is a deliberate
            // click, never something a keystroke does by accident.
            if (event.key === "Enter" && !event.nativeEvent.isComposing) {
              submit("triage");
            }
          }}
        />
        <div style={{ display: "flex", gap: "var(--space-4)", flexWrap: "wrap" }}>
          <Button size="md" iconLeft="plus" disabled={!canSubmit} onClick={() => submit("triage")}>
            Add to Triage
          </Button>
          {/* The skip. CONTEXT.md's Mint — "landing in Ready" — for something
              already startable; the item never sits in Triage at all. */}
          <Button
            size="md"
            variant="secondary"
            iconLeft="sparkles"
            disabled={!canSubmit}
            onClick={() => submit("ready")}
          >
            Mint action
          </Button>
        </div>
      </div>
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(3, 1fr)",
          gap: "var(--space-7)",
          alignItems: "start",
        }}
      >
        <Slider
          label="Energy"
          options={CAPTURE_ENERGY_STOPS}
          value={meta.energy}
          onChange={(energy) => setMeta({ ...meta, energy })}
        />
        <Slider
          label="Size"
          options={CAPTURE_SIZE_STOPS}
          value={meta.size}
          onChange={(size) => setMeta({ ...meta, size })}
        />
        <Select
          label="Context"
          value={meta.context}
          onChange={(event) => setMeta({ ...meta, context: event.target.value })}
          options={[
            { value: "", label: "Not set" },
            ...CONTEXTS.map((context) => ({ value: context, label: context })),
          ]}
        />
      </div>
      {/* One string, not a ternary: #208 made the capture box's
          Energy/Size/Context genuinely persist onto `CreateItem`, so the real
          arm's old "(not yet stored on a real capture)" suffix went from true
          to false — and it sat on the arm that now DOES store them, while
          demo mode got the clean sentence. With the suffix gone the two arms
          said the same thing, so there is nothing left to branch on.
          `CapturePopover.test.tsx` pins the text so it cannot silently rot
          back. */}
      <span className="hb-meta">
        optional — stage, dates and everything else are decided at mint time
      </span>
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
