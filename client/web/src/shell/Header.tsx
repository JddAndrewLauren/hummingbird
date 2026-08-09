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
        style={{
          flex: 1,
          minWidth: 0,
          font: "var(--type-h1)",
          letterSpacing: "var(--tracking-heading)",
          color: "var(--text-primary)",
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
          global focus hotkey lands with S12 (#110). */}
      <Button iconLeft="feather" onClick={onCapture}>
        Capture
      </Button>
    </header>
  );
}
