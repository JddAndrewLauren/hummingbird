// The nav rail's footer line: the core's state as a computed value, in the
// 11px mono meta style. Voice rule (design README): state what is true and
// stop. The failure text says the core failed and nothing reassuring; the
// error itself is too long for an 11px line and renders in Settings.

import type { CoreStatus } from "../store/store";

export function coreStatusLabel(status: CoreStatus, apiVersion: number | null): string {
  if (status === "ready") {
    return apiVersion === null ? "core ready" : `api v${apiVersion} · core ready`;
  }
  return status === "error" ? "core failed" : "starting core…";
}

/** Issue #172's ADR-0010 diagnostic, in words: which core instance this view
 * is talking to, and which connect it was. Rendered in Settings' "Local
 * core" card — not the nav rail, whose 11px mono line already carries
 * `coreStatusLabel` and the build version, and where an abbreviated form
 * would be unreadable anyway.
 *
 * `null` when either half is unknown (no handshake yet), so the caller
 * renders nothing rather than a half-sentence naming an instance it cannot
 * identify. The instance id is the signal: two windows showing the SAME id
 * share one core, which is what ADR-0010 assumes; two different ids refute
 * it. The ordinal is what tells two windows apart at a glance, since the
 * whole point is reading the same id in two places. */
export function coreInstanceLabel(
  coreId: string | null,
  viewOrdinal: number | null,
): string | null {
  if (coreId === null || viewOrdinal === null) {
    return null;
  }
  return `Core instance ${coreId} · this view #${viewOrdinal}.`;
}
