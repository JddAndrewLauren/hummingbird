// Reloads this view when a new service worker takes control, whatever
// caused it — this view's own Reload click, another tab's, or an activation
// nothing in this app asked for.
//
// The gap it closes: `registerSW`'s `controlling` -> reload listener is
// attached inside its `showSkipWaitingPrompt`, so a view only ever gets one
// if it saw a `waiting` event for itself. A view that never ran an update
// check (backgrounded, its interval frozen) has no such listener, and so
// stays on the old shell after another view applies the update — running old
// JS under the new worker, which is exactly the two-builds-one-IndexedDB
// state `UpdateBanner.tsx` argues against. Here the listener is attached at
// wire time, unconditionally, which is what makes that banner's "reloading
// updates every open tab" true rather than aspirational.
//
// DOM-free by construction: the caller supplies the lifecycle, so this is
// provable in the node environment with no service worker and no `location`.
//
// Note on `main.tsx`'s `updateSW(false)`: in vite-plugin-pwa 0.21.2 that
// argument is named `_reloadPage` and never read, so it removes no listener
// and this module is not "the single reload owner". A view that DID show the
// banner keeps the plugin's listener too and may call `location.reload()`
// twice for one `controllerchange`; both land in the same task and collapse
// into one navigation. The `refreshing` guard below governs this module's
// listener only, and is not reaching for cross-owner coordination it cannot
// have.

export interface ServiceWorkerLifecycle {
  /** Whether a service worker currently controls this page. */
  hasController(): boolean;
  /** Subscribes to control changing hands. Returns an unsubscribe. */
  onControllerChange(listener: () => void): () => void;
}

/**
 * Wires `lifecycle` to `reload`. Returns a dispose that unsubscribes.
 *
 * Both pieces of state are closure-local rather than module-level: a module
 * flag would leak between tests in the same file and make the second one's
 * result depend on the first one's.
 */
export function watchForActivation(
  lifecycle: ServiceWorkerLifecycle,
  reload: () => void,
): () => void {
  // Sampled ONCE, at wire time. An uncontrolled page is a first-ever visit
  // (or one after a cache clear): the worker that claims it is installing
  // the shell this page is already running, so reloading would be a pointless
  // flash of the newest build over itself. Sampling later would read `true`
  // by the time the event arrives and lose the distinction.
  const wasControlled = lifecycle.hasController();

  // A reload does not stop this listener from firing again before the
  // navigation commits, and `reload()` is injected — a test's is not going
  // to unload anything.
  //
  // Under `registerType: "prompt"` a loop is not reachable even without this:
  // the reloaded page's own new worker *waits* rather than activating, so
  // there is no second `controllerchange` to answer. The guard defends the
  // day someone flips that config, where a self-claiming worker would
  // otherwise make this a reload loop.
  let refreshing = false;

  return lifecycle.onControllerChange(() => {
    if (!wasControlled || refreshing) {
      return;
    }
    refreshing = true;
    reload();
  });
}
