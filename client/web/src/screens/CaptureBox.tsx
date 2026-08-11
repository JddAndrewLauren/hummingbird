import { useEffect, useState } from "react";
import { Button } from "../components/core/Button";
import { Select } from "../components/forms/Select";
import { Slider } from "../components/forms/Slider";
import { Input } from "../components/forms/Input";
import { CAPTURE_INPUT_ID } from "../shell/capture-hotkey";
import { canSubmitCapture } from "./capture-validation";
import type { CaptureDestination } from "./capture-destination";

const CONTEXTS = ["@home", "@computer", "@phone", "@errands", "@garden", "@waiting"];

interface CaptureMeta {
  energy: number | null;
  size: number | null;
  context: string;
}

const EMPTY_META: CaptureMeta = { energy: null, size: null, context: "" };

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
   * `Core::capture` has no opinion of its own and would enqueue it. */
  onSubmit: (title: string, destination: CaptureDestination) => void;
  /** Demo mode only changes one line of copy: a real capture does not yet
   * store the optional metadata below, and saying otherwise would be the one
   * thing this product's voice refuses. */
  demo: boolean;
  /** Bumped to move focus into the field — the shell's global capture hotkey
   * and its "New" button both land here. Focus is taken on mount too (this
   * component mounts when the popover opens, which IS the request), so
   * unlike the screen-level version this needs no first-render guard. */
  focusRequestId: number;
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
export function CaptureBox({ onSubmit, demo, focusRequestId }: CaptureBoxProps) {
  const [draft, setDraft] = useState("");
  const [meta, setMeta] = useState<CaptureMeta>(EMPTY_META);
  const [last, setLast] = useState<LastSubmit | null>(null);

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

  function submit(destination: CaptureDestination) {
    if (!canSubmit) {
      return;
    }
    // The raw string, not a trimmed one: #110's "the raw string reaches the
    // mutation unmodified" — `canSubmitCapture` decides *whether* to submit,
    // never *what* is submitted.
    onSubmit(draft, destination);
    setLast({ destination, title: draft });
    setDraft("");
    setMeta(EMPTY_META);
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
          options={["low", "medium", "high"]}
          value={meta.energy}
          onChange={(energy) => setMeta({ ...meta, energy })}
        />
        <Slider
          label="Size"
          options={["quick", "normal", "deep"]}
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
      <span className="hb-meta">
        {demo
          ? "optional — stage, dates and everything else are decided at mint time"
          : "optional — stage, dates and everything else are decided at mint time (not yet stored on a real capture)"}
      </span>
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
