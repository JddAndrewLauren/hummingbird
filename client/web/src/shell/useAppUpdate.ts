import { useEffect, useRef, useSyncExternalStore } from "react";
import { appUpdateSignal, type AppUpdateSignal } from "./app-update";
import { createUpdateChecker, UPDATE_CHECK_INTERVAL_MS } from "./update-check";

// The React side of the "a new version is waiting" strip: the signal read
// through `useSyncExternalStore` (`store/useStore.ts:7`'s shape), plus the
// two things that ask the browser to go looking for one — the hourly tick
// and window focus, both funnelled through `createUpdateChecker`'s gap rule.
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
 * exact input that used to reset both schedules. */
const DEFAULT_CHECK = (): void => void updateRegistration();

export function useAppUpdate(
  signal: AppUpdateSignal = appUpdateSignal,
  check: () => void = DEFAULT_CHECK,
): AppUpdate {
  const snapshot = useSyncExternalStore(signal.subscribe, signal.getSnapshot);

  // `check` is held in a ref and read at call time, so NOTHING below
  // depends on its identity. Both schedules here are longer than the
  // interval between renders — `useSyncWiring.ts`'s status clock rerenders
  // `App` every 30 seconds — so a checker or an interval rebuilt per render
  // never survives long enough to fire: the hourly tick restarts before it
  // reaches an hour, and `createUpdateChecker`'s gap forgets its last check
  // and lets every focus through. Hoisting the default alone would fix
  // today's caller; this makes any caller's inline arrow harmless too.
  const checkRef = useRef(check);
  useEffect(() => {
    checkRef.current = check;
  });

  // The checker is built HERE, inside the mount-once effect, rather than in
  // a `useMemo`: it has no reader outside this effect, and creating it in
  // render is what made its lifetime a render-identity question in the first
  // place. Empty deps, so one checker and one interval per mount.
  useEffect(() => {
    const checker = createUpdateChecker(() => checkRef.current(), Date.now);
    const timer = window.setInterval(() => checker.request(), UPDATE_CHECK_INTERVAL_MS);
    function onFocus() {
      checker.request();
    }
    window.addEventListener("focus", onFocus);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener("focus", onFocus);
    };
  }, []);

  return {
    ready: snapshot.ready,
    onReload: () => snapshot.apply?.(),
  };
}
