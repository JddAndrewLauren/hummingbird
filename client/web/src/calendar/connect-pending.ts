// The one thing that can un-stick a Connect button left spinning by the
// redirect flow.
//
// **The trap.** `handleConnectClick`'s redirect branch sets
// `connectPending: true` and then navigates the document away. Nothing in this
// app clears it: the clear lives in the start effect, which runs when the
// browser comes back with a *fragment* to apply. Press Back from Google's
// consent screen instead and there is no fragment — and, commonly, no load
// either, because the page is restored from bfcache. Module scope does not
// re-run, React state comes back exactly as it was, and the reader is looking
// at a disabled, spinning button with no error, no explanation and no way out
// short of reloading.
//
// **Why both events, and why `pageshow` is not filtered on `persisted`.**
// `shell/useAppUpdate.ts` establishes this repo's idiom and its reasoning
// applies unchanged: a bfcache restore fires `pageshow` and NOT
// `visibilitychange`, several real resume paths report `persisted: false` for a
// restore that nonetheless skipped every other signal, and a redundant call
// costs nothing here because clearing an already-clear flag is a no-op. Both
// listeners, no `persisted` filter.
//
// **Why the in-flight predicate rather than clearing unconditionally.** The
// popup path is still alive on desktop, and there `connectPending` covers a
// genuinely in-flight `await connect(deps)` that will clear the flag itself
// when it settles. A reader who switches tabs and comes back mid-consent fires
// `visibilitychange`, and clearing there would re-enable the button under a
// request that is still running — inviting a second concurrent consent. Only
// the redirect branch arms this, and only the redirect branch is unrecoverable
// without it.

export interface ConnectPendingRestoreDeps {
  /** Whether a redirect attempt was started from this document and has not
   * been answered. The caller holds this across the navigation in a ref, which
   * survives a bfcache restore for the same reason the bug does: the JS heap
   * is preserved. */
  isRedirectInFlight: () => boolean;
  /** Clears the pending flag — and the caller's own in-flight marker with it,
   * so a later resume does not keep re-firing against an attempt that is over. */
  onRestore: () => void;
}

/** Wires the two "this view came back" signals. Returns the teardown, in the
 * shape a `useEffect` can return directly. */
export function watchConnectPendingRestore(deps: ConnectPendingRestoreDeps): () => void {
  function restore() {
    if (!deps.isRedirectInFlight()) {
      return;
    }
    deps.onRestore();
  }
  function onPageShow() {
    restore();
  }
  function onVisibilityChange() {
    // Only the coming-forward edge. The hiding edge is the navigation to
    // Google itself, which is the moment the flag is most legitimately set.
    if (document.visibilityState === "visible") {
      restore();
    }
  }
  window.addEventListener("pageshow", onPageShow);
  document.addEventListener("visibilitychange", onVisibilityChange);
  return () => {
    window.removeEventListener("pageshow", onPageShow);
    document.removeEventListener("visibilitychange", onVisibilityChange);
  };
}
