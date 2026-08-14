import { useEffect, useRef, useSyncExternalStore } from "react";
import { appUpdateSignal, type AppUpdateSignal } from "./app-update";
import { createUpdateChecker, UPDATE_CHECK_INTERVAL_MS } from "./update-check";

// The React side of the "a new version is waiting" strip: the signal read
// through `useSyncExternalStore` (`store/useStore.ts:7`'s shape), plus the
// five things that ask the browser to go looking for one — mount, `focus`,
// `visibilitychange`, `pageshow` and the periodic tick — all funnelled
// through the one `createUpdateChecker` so its gap rule stays the single
// rate limiter.
//
// Discovery is the whole bug this answers. `registerType: "prompt"` leaves
// a new worker waiting and `navigateFallback` answers navigations from the
// precache, so a view that never *asks* re-renders the old shell forever,
// pull-to-refresh included. An interval alone cannot ask: it is frozen
// while the view is backgrounded, which on a phone is almost always. So
// every way a view can come forward gets a signal:
//
//   - mount          — a view open across a deploy, or freshly opened.
//   - `focus`        — the desktop case; unreliable for a standalone resume.
//   - `visibility`   — what an installed iOS PWA actually fires on resume
//                      (`useSyncWiring.ts:64-75` is the house idiom).
//   - `pageshow`     — a bfcache restore, which fires NEITHER of the above.
//   - the tick       — a view left foregrounded and untouched, nothing else.
//
// This hook holds no decision of its own; both seams below are default
// parameters, the injection idiom `useTaskTokenWiring.ts` already uses, so
// a test drives it with neither a service worker nor a real clock.

export interface AppUpdate {
  /** A new version is waiting — render the strip. */
  ready: boolean;
  /** Applies the waiting worker and reloads. A no-op until one is waiting. */
  onReload: () => void;
}

/** Asks the browser to re-fetch the service worker script. A registration
 * that answers nothing (no service worker support, none registered yet) is
 * simply nothing to check — never an error surfaced to the reader. */
async function updateRegistration(): Promise<void> {
  if (!("serviceWorker" in navigator)) {
    return;
  }
  const registration = await navigator.serviceWorker.getRegistration();
  await registration?.update();
}

/** The default `check`, hoisted to module scope so it has ONE identity for
 * the life of the module. Written inline as a default parameter it was a
 * fresh function on every render, which — see `checkRef` below — is the
 * exact input that used to reset both schedules.
 *
 * The `.catch` is load-bearing, not defensive tidying: `registration.update()`
 * REJECTS when the script fetch fails, and a phone resuming offline — now
 * one of the commonest paths into here — is exactly that. Discarding the
 * promise bare made every such resume an unhandled rejection. Swallowing is
 * right: there is nothing to tell the reader, and the next signal retries. */
const DEFAULT_CHECK = (): void => {
  void updateRegistration().catch(() => {});
};

export function useAppUpdate(
  signal: AppUpdateSignal = appUpdateSignal,
  check: () => void = DEFAULT_CHECK,
): AppUpdate {
  const snapshot = useSyncExternalStore(signal.subscribe, signal.getSnapshot);

  // `check` is held in a ref and read at call time, so NOTHING below
  // depends on its identity. Both schedules here are longer than the
  // interval between renders — `useSyncWiring.ts`'s status clock rerenders
  // `App` every 30 seconds — so a checker or an interval rebuilt per render
  // never survives long enough to fire: the periodic tick restarts before
  // it comes round, and `createUpdateChecker`'s gap forgets its last check
  // and lets every signal through. Hoisting the default alone would fix
  // today's caller; this makes any caller's inline arrow harmless too.
  const checkRef = useRef(check);
  useEffect(() => {
    checkRef.current = check;
  });

  // The checker is built HERE, inside the mount-once effect, rather than in
  // a `useMemo`: it has no reader outside this effect, and creating it in
  // render is what made its lifetime a render-identity question in the first
  // place. Empty deps, so ONE checker, one interval and one set of listeners
  // per mount — and one checker is what makes the gap rule apply across all
  // five signals rather than per signal.
  useEffect(() => {
    const checker = createUpdateChecker(() => checkRef.current(), Date.now);

    // On mount, before any listener: a view opened across a deploy learns
    // about it now rather than waiting for a signal that may never come.
    checker.request();

    const timer = window.setInterval(() => checker.request(), UPDATE_CHECK_INTERVAL_MS);
    function onFocus() {
      checker.request();
    }
    function onVisibilityChange() {
      // Only the coming-forward edge. The hiding edge is a view leaving,
      // which is not a reason to ask the origin for anything.
      if (document.visibilityState === "visible") {
        checker.request();
      }
    }
    function onPageShow() {
      // Deliberately NOT filtered on `event.persisted`. Several real resume
      // paths report `false` for a restore that nonetheless skipped every
      // other signal, and a redundant request here costs nothing — the gap
      // rule already collapses it against the mount check beside it.
      checker.request();
    }
    window.addEventListener("focus", onFocus);
    document.addEventListener("visibilitychange", onVisibilityChange);
    window.addEventListener("pageshow", onPageShow);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener("focus", onFocus);
      document.removeEventListener("visibilitychange", onVisibilityChange);
      window.removeEventListener("pageshow", onPageShow);
    };
  }, []);

  return {
    ready: snapshot.ready,
    onReload: () => snapshot.apply?.(),
  };
}
