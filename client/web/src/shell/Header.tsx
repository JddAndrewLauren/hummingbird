import { useEffect, useRef } from "react";
import { Badge } from "../components/core/Badge";
import { Button } from "../components/core/Button";
import { IconButton } from "../components/core/IconButton";

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
   * the core reports `ready`. The affordance appears only where it would work:
   * omit it and no button renders. */
  onRefresh?: () => void;
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
        <IconButton icon="refresh-cw" label="Refresh calendar context" onClick={onRefresh} />
      ) : null}
      {/* The shell owns capture (#107): the box itself lives on Triage, and
          this is the always-present way to reach it from any screen. The
          global focus hotkey (#110/S12) is `App.tsx`'s `capture-hotkey.ts`
          listener — this button fires the identical "focus the box"
          request. */}
      <Button iconLeft="feather" onClick={onCapture}>
        Capture
      </Button>
    </header>
  );
}
