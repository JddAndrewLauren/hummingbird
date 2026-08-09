// The nav rail's footer line: the core's state as a computed value, in the
// 11px mono meta style. Voice rule (design README): state what is true and
// stop. The failure text says the core failed and nothing reassuring; the
// error itself is too long for an 11px line and renders in Settings.

import type { CoreStatus } from "../store/store";

export function coreStatusLabel(status: CoreStatus, apiVersion: number | null): string {
  if (status === "ready") {
    return apiVersion === null ? "core ready" : `core ready · api v${apiVersion}`;
  }
  return status === "error" ? "core failed" : "starting core…";
}
