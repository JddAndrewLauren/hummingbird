// Tracks each connected view's own reported page-visibility state, for the
// shared core's single sync timer to consult (`core.worker.ts`) — round-1
// review of #107/PR#181 caught a per-tab `setInterval` that multiplied
// cycles with open-tab count, which ADR-0010's amendment to ADR-0007 rules
// out ("a second tab is a view, not a second cycle"). A `SharedWorker`'s
// global scope has no `document` of its own, so page visibility can only
// ever reach it as a fact each view reports (`protocol.ts`'s
// `setViewVisibility`).
//
// "Hidden" from the shared core's perspective means every known view
// reports hidden — one visible tab keeps the cycle running even while N-1
// siblings are backgrounded; this is a single, origin-wide decision, not
// one made per tab.
//
// Never explicitly pruned on disconnect — `ports.ts`'s own `PortRegistry`
// documents the identical tradeoff for its port `Set` (a `connect` event
// has no matching, script-observable "disconnect"), so a tab that closes
// without ever reporting hidden leaves a stale "visible" entry behind. This
// can only ever fail OPEN (the shared timer keeps ticking a little longer
// than strictly needed by one already-closed tab's last report), never
// silently mutes a still-open, genuinely visible tab's own status — tracked
// as issue #183 rather than left silent.
export class VisibilityTracker<PortId> {
  private readonly hiddenByPort = new Map<PortId, boolean>();

  setHidden(port: PortId, hidden: boolean): void {
    this.hiddenByPort.set(port, hidden);
  }

  /** True only if every view that has ever reported in currently reports
   * hidden — including the vacuous case of no reports at all, which cannot
   * happen while this `SharedWorker`'s global scope is alive (the browser
   * tears it down once the last port disconnects) but is still the safe,
   * paused-by-default answer if it somehow did. */
  isHidden(): boolean {
    if (this.hiddenByPort.size === 0) {
      return true;
    }
    for (const hidden of this.hiddenByPort.values()) {
      if (!hidden) {
        return false;
      }
    }
    return true;
  }
}
