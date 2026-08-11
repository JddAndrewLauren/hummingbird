import { useEffect, useLayoutEffect, useRef } from "react";
import { Card } from "../components/core/Card";
import { IconButton } from "../components/core/IconButton";
import { CaptureBox } from "../screens/CaptureBox";
import type { CaptureDestination } from "../screens/capture-destination";

/** The DOM id the header's New button carries, so this popover can measure
 * what it hangs from. An id rather than a ref threaded through `Header`:
 * `Button` (the design-system component) forwards no ref, and the hotkey path
 * has no React handle on the trigger at all. */
export const CAPTURE_TRIGGER_ID = "shell-capture-trigger";

/** Gap between the button's bottom edge and the card, and the breathing room
 * left under the card before the window's bottom edge. */
const ANCHOR_GAP = 8;
const BOTTOM_ROOM = 24;

export interface CapturePopoverProps {
  open: boolean;
  /** Bumped by every "capture now" gesture — the header's New button and the
   * global hotkey. Threaded into `CaptureBox`, which takes focus on it, so a
   * second press while the popover is already open re-focuses the field
   * instead of being a no-op. */
  focusRequestId: number;
  onClose: () => void;
  onSubmit: (title: string, destination: CaptureDestination) => void;
  demo: boolean;
}

/** The shell's capture popover — the capture box under the header's New
 * button, over whatever screen is open, rather than a trip to Triage. Capture
 * is a shell-level gesture (#107): it is reachable from every screen, and
 * navigating away from what someone was reading in order to type one line is
 * the cost this replaces. It hangs off the trigger it was opened from
 * (`CAPTURE_TRIGGER_ID`), which is what makes it read as that button's own
 * surface rather than as a page-level interruption.
 *
 * It does not close on submit. Capturing several things in one sitting is the
 * normal case, `CaptureBox` reports what each submit did, and closing would
 * throw away the field focus that makes the next one one keystroke away.
 * Escape, the close button and a click on the scrim are the three ways out —
 * all three the same `onClose`.
 *
 * Everything decidable here is somebody else's: the draft rule is
 * `capture-validation.ts`, the destinations are `capture-destination.ts`, and
 * where a capture goes is `App.tsx`'s wiring. This component is the overlay
 * and its keyboard contract, nothing more. */
export function CapturePopover({ open, focusRequestId, onClose, onSubmit, demo }: CapturePopoverProps) {
  const restoreTo = useRef<Element | null>(null);
  const cardRef = useRef<HTMLDivElement>(null);

  // Measured, not hardcoded: the header's height is type- and token-dependent,
  // and the button's right edge moves with the sync badge and the refresh
  // control beside it. Written straight onto the node rather than held in
  // state — a measurement is not application state, and round-tripping it
  // through a render would paint the card at the fallback position first.
  // A layout effect, so the write lands before the browser paints.
  //
  // Re-measured on resize; nothing else moves the trigger, since the header is
  // fixed chrome and only the content column scrolls. If the trigger is not in
  // the DOM at all (the hotkey works on every screen, and a future screen may
  // not carry the button) the markup's own fallback stands: the top-right
  // corner, which is where the button sits when there is one.
  useLayoutEffect(() => {
    if (!open) {
      return;
    }
    function measure() {
      const card = cardRef.current;
      const trigger = document.getElementById(CAPTURE_TRIGGER_ID);
      if (!card || !trigger) {
        return;
      }
      const rect = trigger.getBoundingClientRect();
      const top = rect.bottom + ANCHOR_GAP;
      card.style.top = `${top}px`;
      card.style.right = `${window.innerWidth - rect.right}px`;
      card.style.maxHeight = `${window.innerHeight - top - BOTTOM_ROOM}px`;
    }
    measure();
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, [open]);

  // Focus goes into the field (CaptureBox's own effect) and has to come back
  // out again: whatever was focused when this opened — the header button, or
  // whatever the hotkey was pressed over — gets it back on close, so a
  // keyboard user is not dropped at the top of the document.
  useEffect(() => {
    if (!open) {
      return;
    }
    restoreTo.current = document.activeElement;
    // Escape on the document, not on the overlay's own `onKeyDown`: an
    // Escape must still close this if focus has left the card (tabbed past
    // its last control, say), and a handler bound to the markup only sees
    // what bubbles out of it.
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        onClose();
      }
    }
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("keydown", onKeyDown);
      const target = restoreTo.current;
      if (target instanceof HTMLElement && target.isConnected) {
        target.focus();
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  if (!open) {
    return null;
  }

  return (
    // The scrim. `onMouseDown` rather than `onClick`, and only when the press
    // landed on the scrim itself: a click that starts inside the card and ends
    // on the scrim (a drag that overshoots while selecting text) is not a
    // request to close.
    <div
      // `presentation`: the scrim is a backdrop, not a control — its press
      // handler is a convenience over the two real, focusable ways out
      // (Escape and the close button), so it must not be announced as
      // something to operate.
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) {
          onClose();
        }
      }}
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 40,
        background: "var(--surface-scrim)",
      }}
    >
      {/* Hung off the trigger rather than centred: the button is where the
          gesture started, and a card that opens under it reads as that
          button's own surface instead of as a page-level interruption.

          The positioned element carries the dialog role, not the `Card`
          inside it — the `Card` is the visual surface and forwards no ref, and
          the placement written by the effect above needs a node to write to.
          The values below are the pre-measurement fallback: top-right, the
          same 720 ceiling floored by the window so the card never reaches
          past the left gutter at 768. */}
      <div
        ref={cardRef}
        role="dialog"
        aria-modal="true"
        aria-label="New capture"
        style={{
          position: "fixed",
          top: "var(--space-10)",
          right: "var(--gutter-page)",
          width: "min(720px, calc(100vw - 2 * var(--gutter-page)))",
          maxHeight: "calc(100dvh - 2 * var(--space-10))",
          overflowY: "auto",
        }}
      >
        <Card
          elevation={3}
          padding="var(--space-6)"
          style={{ display: "flex", flexDirection: "column", gap: "var(--space-5)" }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: "var(--space-5)" }}>
            <h2 style={{ flex: 1, minWidth: 0, font: "var(--type-h3)", color: "var(--text-primary)" }}>
              New
            </h2>
            <IconButton icon="x" label="Close" onClick={onClose} />
          </div>
          <CaptureBox onSubmit={onSubmit} demo={demo} focusRequestId={focusRequestId} />
        </Card>
      </div>
    </div>
  );
}
