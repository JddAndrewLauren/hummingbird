import { useEffect, useRef } from "react";
import { Badge } from "../components/core/Badge";
import { Button } from "../components/core/Button";
import { IconButton } from "../components/core/IconButton";
import { CAPTURE_TRIGGER_ID, RECALL_TRIGGER_ID } from "./trigger-ids";

export interface HeaderProps {
  title: string;
  /** The sync readout, pre-formatted. Rendered only when there is one: no
   * outbound queue exists yet, and a permanent "synced" pill would claim a
   * cycle that never ran. */
  syncLabel?: string;
  /** Opens the shell's Recall overlay (#478) over whatever screen is
   * showing — the same "the affordance appears only where it would work"
   * rule every other optional control here follows. `App.tsx` is what
   * decides where this renders (every screen, same as capture). */
  onSearch?: () => void;
  /** Refresh polls the worker, and `worker-client.ts` may only be called once
   * the core reports `ready`. The affordance appears only where it would
   * work: omit it and no button renders. Issue #194: this now runs the
   * ADR-0007 task sync cycle, the calendar context poll, or both, depending
   * on what `App.tsx`'s `refresh-gate.ts` found refreshable — never just the
   * calendar, even on a device with both. */
  onRefresh?: () => void;
  /** Toggles Now's standing-questions aside. Both directions live here, not
   * one here and one in the panel's own corner: a control that moves when you
   * press it is a control you have to find twice. Supplied by `App.tsx` only
   * on Now, on the same rule as `onSearch`/`onRefresh` — the affordance
   * appears where it would work. */
  onToggleQuestions?: () => void;
  /** Which way `onToggleQuestions` currently goes; picks the glyph. */
  questionsCollapsed?: boolean;
  /** Opens the shell's capture popover (`CapturePopover`). Named for the
   * internal verb, labelled "New" in the UI. */
  onCapture: () => void;
}

export function Header({
  title,
  syncLabel,
  onSearch,
  onRefresh,
  onToggleQuestions,
  questionsCollapsed = false,
  onCapture,
}: HeaderProps) {
  const headingRef = useRef<HTMLHeadingElement>(null);
  const mounted = useRef(false);

  // Switching screens swaps this heading and replaces everything below it,
  // but nothing else tells assistive tech the view changed — focus would
  // stay parked in the nav rail. Moving focus to the new heading both
  // announces it and puts the reading position at the top of the new
  // content. Skipped on mount: stealing focus on load is disorienting.
  useEffect(() => {
    if (!mounted.current) {
      mounted.current = true;
      return;
    }
    headingRef.current?.focus();
  }, [title]);

  return (
    // `hb-header` / `hb-header-title` rather than style objects: on a phone the
    // title takes its own line and drops a size, and at equal importance a
    // stylesheet rule loses to an element's own `style` attribute. `!important`
    // would have won — `font: var(--type-h2) !important` beats an inline `font`
    // shorthand just as it beats a longhand — but only by putting `!important`
    // on every phone declaration and on anything that ever needs to override
    // one. Deleting the h1's object instead leaves one source of truth per
    // element. `shell/responsive.css` carries the argument.
    <header className="hb-header">
      <h1
        ref={headingRef}
        className="hb-header-title"
        // Focusable only programmatically, by the effect above — never a tab
        // stop, so it is not a keyboard-operable component and shows no ring.
        tabIndex={-1}
      >
        {title}
      </h1>
      {syncLabel ? (
        <Badge mono tone="neutral">
          {syncLabel}
        </Badge>
      ) : null}
      {/* `id` is what `RecallOverlay` measures to hang itself under this
          button — the identical trick the New button's `CAPTURE_TRIGGER_ID`
          plays for `CapturePopover`.

          "Search everything" is the one name all three triggers wear — this
          button, the rail's magnifier and the phone More sheet's entry.
          Recall is the domain and the dialog's name (CONTEXT.md); what a
          trigger says is what it does, and it said "Search" here only
          because a header has less room than a menu row. */}
      {onSearch ? (
        <IconButton
          id={RECALL_TRIGGER_ID}
          icon="search"
          label="Search everything"
          onClick={onSearch}
        />
      ) : null}
      {onRefresh ? (
        <IconButton icon="refresh-cw" label="Refresh" onClick={onRefresh} />
      ) : null}
      {/* Between Refresh and New on purpose: it belongs with the chrome that
          acts on the screen you are looking at, not with the one gesture that
          creates something. It stays in this one slot through both states, so
          shutting the panel never moves the control that reopens it.

          Shut, `?` names what is missing; open, the chevron points the way the
          panel goes — right, out of the reader's way. Both glyphs are already
          in the design system's named vocabulary.

          `label` and nothing else: `IconButton` writes `aria-label` AND
          `title` from it after spreading `rest`, so a `title` passed here
          would be silently dropped. `aria-expanded` carries the direction. */}
      {onToggleQuestions ? (
        // The rotation rides on a wrapper, not on `IconButton`'s `style`:
        // that prop is spread last over the button's own `transform`, so
        // writing one here would cost the press scale. Same reason as the
        // capture box's disclosure (`screens/CaptureBox.tsx`).
        <span
          style={{
            display: "inline-flex",
            transform: questionsCollapsed ? "none" : "rotate(-90deg)",
            transition: "transform var(--dur-fast) var(--ease-flit)",
          }}
        >
          <IconButton
            icon={questionsCollapsed ? "help-circle" : "chevron-down"}
            label="Standing questions"
            aria-expanded={!questionsCollapsed}
            onClick={onToggleQuestions}
          />
        </span>
      ) : null}
      {/* The shell owns capture (#107): this opens `CapturePopover` over
          whatever screen is showing, rather than navigating to Triage. The
          global hotkey (#110/S12) is `App.tsx`'s `capture-hotkey.ts`
          listener and fires the identical request.

          Labelled "New" — what the person is doing, not the internal verb.
          Capture is still the verb everywhere it belongs: the field's own
          label, the `feather` icon (the brand's capture glyph), the wire
          message, `Core::capture`. */}
      {/* The id is what `CapturePopover` measures to hang itself under this
          button; see `CAPTURE_TRIGGER_ID`. */}
      <Button id={CAPTURE_TRIGGER_ID} iconLeft="feather" onClick={onCapture}>
        New
      </Button>
    </header>
  );
}
