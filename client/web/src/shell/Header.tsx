import { useEffect, useRef } from "react";
import { Badge } from "../components/core/Badge";
import { Button } from "../components/core/Button";
import { IconButton } from "../components/core/IconButton";
import { CAPTURE_TRIGGER_ID } from "./CapturePopover";

export interface HeaderProps {
  title: string;
  /** The sync readout, pre-formatted. Rendered only when there is one: no
   * outbound queue exists yet, and a permanent "synced" pill would claim a
   * cycle that never ran. */
  syncLabel?: string;
  /** Search has no implementation yet; the affordance appears only where it
   * would work. */
  onSearch?: () => void;
  /** Refresh polls the worker, and `worker-client.ts` may only be called once
   * the core reports `ready`. The affordance appears only where it would
   * work: omit it and no button renders. Issue #194: this now runs the
   * ADR-0007 task sync cycle, the calendar context poll, or both, depending
   * on what `App.tsx`'s `refresh-gate.ts` found refreshable — never just the
   * calendar, even on a device with both. */
  onRefresh?: () => void;
  /** Opens the shell's capture popover (`CapturePopover`). Named for the
   * internal verb, labelled "New" in the UI. */
  onCapture: () => void;
}

export function Header({ title, syncLabel, onSearch, onRefresh, onCapture }: HeaderProps) {
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
    <header
      style={{
        display: "flex",
        alignItems: "center",
        gap: "var(--space-5)",
        flex: "0 0 auto",
        padding: "var(--space-7) var(--gutter-page) var(--space-6)",
      }}
    >
      <h1
        ref={headingRef}
        // Focusable only programmatically, by the effect above — never a tab
        // stop, so it is not a keyboard-operable component and shows no ring.
        tabIndex={-1}
        style={{
          flex: 1,
          minWidth: 0,
          font: "var(--type-h1)",
          letterSpacing: "var(--tracking-heading)",
          color: "var(--text-primary)",
          outline: "none",
        }}
      >
        {title}
      </h1>
      {syncLabel ? (
        <Badge mono tone="neutral">
          {syncLabel}
        </Badge>
      ) : null}
      {onSearch ? <IconButton icon="search" label="Search" onClick={onSearch} /> : null}
      {onRefresh ? (
        <IconButton icon="refresh-cw" label="Refresh" onClick={onRefresh} />
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
