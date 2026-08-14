import type { CSSProperties } from "react";
import { APP_VERSION } from "./build-version";

export interface ShellMetaProps {
  /** The core's state, already formatted (see `status-label.ts`). */
  statusLabel: string;
  style?: CSSProperties;
}

/** The shell's two meta lines: the core's state (which carries the api
 * version) over the build version.
 *
 * Stacked, not side by side — at 11px in the mono meta style they read as one
 * run-on string on a single line; stacked, each is its own fact. The build
 * version is deliberately its own span rather than folded into
 * `coreStatusLabel`: it is known even when the core failed or is still
 * starting, whereas that function's other two branches say nothing at all.
 *
 * Extracted from `NavRail`'s footer (#107) because the phone form has no rail
 * to put it in, and a build number nobody can read is the bug this whole
 * change set started from. `NavRail` still renders it in its expanded footer
 * exactly as before; `NavBar` renders it at the foot of the More sheet. It
 * reads `APP_VERSION` itself rather than taking it as a prop, so neither
 * caller can render a version it made up. */
export function ShellMeta({ statusLabel, style }: ShellMetaProps) {
  return (
    <div
      style={{
        minWidth: 0,
        display: "flex",
        flexDirection: "column",
        gap: "var(--space-1)",
        ...style,
      }}
    >
      <span className="hb-meta">{statusLabel}</span>
      <span className="hb-meta">{`v${APP_VERSION}`}</span>
    </div>
  );
}
