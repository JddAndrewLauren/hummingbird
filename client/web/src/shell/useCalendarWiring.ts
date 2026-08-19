import { useEffect, useRef, useState } from "react";
import {
  connect,
  type ConnectionDeps,
  type ConnectionResult,
  handleCredentialNeeded,
  initConnection,
  msUntilRotation,
  shouldKeepExistingConnection,
} from "../calendar/connection";
import { watchConnectPendingRestore } from "../calendar/connect-pending";
import {
  readConnected,
  readSelectedCalendarIds,
  writeConnected,
  writeSelectedCalendarIds,
} from "../calendar/persistence";
import { resolveRedirectReturn } from "../calendar/redirect-return";
import {
  INITIAL_REMINT_HEALTH,
  recordInteractiveConnect,
  recordSilentRemint,
  type RemintHealth,
} from "../calendar/remint-health";
import { acceptSelectionChange, effectiveSelection } from "../calendar/selection";
import { createAuthorityTokenClient } from "../calendar/authority-token-client";
import type { TokenClient } from "../calendar/token-client";
import type { RedirectOutcome } from "../google/redirect-flow";
import { PHONE_QUERY } from "./breakpoints";
import { startOAuthRedirect, takeOAuthRedirect } from "./oauth-redirect";
import { isStandalone } from "./standalone";
import { coreStore, type CalendarState, type CoreStatus } from "../store/store";
import { createIndexedDbTaskTokenStore, type TaskTokenStoreLike } from "../task/token-store";
import {
  pollRefresh,
  pollStart,
  pollTimer,
  pushTokenToWorker,
  requestCalendarList,
  setCalendarSelectionsOnWorker,
  type WorkerLike,
} from "../store/worker-client";

// The web host's calendar opt-in wiring (issue #73): the token source, the
// proactive token rotation, the context poll cadence and the clock tick that
// keeps staleness honest. Extracted from App.tsx unchanged when the shell was
// decomposed (#107) — these effects must run for the app's whole lifetime
// regardless of which screen is mounted, so the shell calls this hook once
// and passes the handlers down.
//
// Everything here is intentionally thin: every decision (silent re-mint vs.
// re-connect, staleness, selection toggling, which errors mean a human is
// required, how to read a redirect fragment) is delegated to a unit-tested
// pure module under calendar/ or google/; this file only wires their results
// into the store and the worker.
//
// **#577/#583: the silent path no longer touches Google at all.** Every
// `TokenClient` call — the start-up re-mint, the proactive rotation below,
// and a core 401's credential-needed round-trip — now reaches
// `calendar/authority-token-client.ts`, a same-origin authenticated POST to
// `POST /api/google/calendar_token` (ADR-0028), usually answered from the
// authority's own cache. `google/gis.ts` and its popup are gone; **the
// hourly popup is gone with them.**
//
// The redirect flow (`google/redirect-flow.ts`, `shouldUseRedirect` below)
// is untouched by this slice and still navigates to Google directly on a
// standalone/phone-sized Connect click — #584 retires it once Settings'
// prerequisite copy (#585) no longer assumes a client-side consent step
// exists. On desktop, `connect()`'s `requestToken("consent")` now reaches
// the same authority-backed client the silent path uses, which ignores the
// prompt: a desktop Connect click no longer opens any popup either, because
// there is nothing left in the browser for it to open.

// The 15-minute context-poll foreground timer (#46, under ADR-0005). The
// host is responsible for only ticking while online and foregrounded —
// `document.hidden`/`navigator.onLine` gate every tick below.
const TIMER_INTERVAL_MS = 15 * 60 * 1000;

// This hook owns NO clock of its own beyond the 15-minute poll timer above.
// It used to also run a 30-second tick, purely to keep the context tile's
// "as of" label live and to re-ask for the current/next event as the moment
// moved; #245 replaced that tile with ADR-0015's ranked pane region, and
// `useSyncWiring.ts`'s existing unconditional 30-second `nowMs` is the one
// clock the Now screen gets.

// Still gates every calendar effect below, even though the authority-backed
// `TokenClient` itself needs no client id at all — it authenticates with the
// device token, not this. Kept as the prerequisite for this slice: the
// redirect flow above still needs it, and rewriting what "connected" needs
// is #585's job ("Settings tells the truth about what a calendar connection
// needs"), not this one's.
export const GOOGLE_CLIENT_ID: string | undefined = import.meta.env.VITE_GOOGLE_CLIENT_ID;

let cachedTokenClient: TokenClient | null = null;
let cachedTaskTokenStore: TaskTokenStoreLike | null = null;
function tokenClient(): TokenClient | null {
  if (!GOOGLE_CLIENT_ID) {
    return null;
  }
  cachedTaskTokenStore ??= createIndexedDbTaskTokenStore();
  const taskTokenStore = cachedTaskTokenStore;
  cachedTokenClient ??= createAuthorityTokenClient({
    fetch: globalThis.fetch.bind(globalThis),
    readToken: async () => (await taskTokenStore.read())?.token ?? null,
  });
  return cachedTokenClient;
}

/** Turns a redirect return into the same `ConnectionResult` the popup path
 * produces, so everything downstream — the persisted flag, the store write,
 * the rotation timer, the poll — is one code path with one shape. Synchronous:
 * the token is already in hand, there is nothing to await. */
function applyRedirect(
  // `Exclude<…, "none">`: "this load is not a return from Google" is the
  // caller's branch, not a case here, and typing it out makes that structural
  // rather than a convention.
  outcome: Exclude<RedirectOutcome, { kind: "none" }>,
  deps: ConnectionDeps,
): ConnectionResult {
  if (outcome.kind === "error") {
    return { connected: false, needsReconnect: false, expiresAtMs: null, error: outcome.error };
  }
  deps.pushToken(outcome.accessToken);
  return {
    connected: true,
    needsReconnect: false,
    expiresAtMs: outcome.expiresAtMs,
    error: null,
  };
}

/** Whether this device should navigate to Google directly (`google/redirect-
 * flow.ts`) instead of taking the desktop `connect()` path, which — since
 * #583 deleted `google/gis.ts` — opens no popup of its own and simply asks
 * the authority for a token like every other path here.
 *
 * Standalone is the real criterion — an installed iOS web app is where a
 * popup escapes to Safari and loses its opener, and where Safari's own storage
 * container is no use because the app cannot see it. A phone-sized browser tab
 * is included because the same popup on a small screen is a full-screen
 * takeover with no visible relationship to the app it came from, and because a
 * phone tab is one "Add to Home Screen" away from being the standalone case
 * anyway. This function and the redirect flow it guards are #584's to
 * retire, not this slice's — see this file's header.
 *
 * **Measured with `matchMedia(PHONE_QUERY)`, not `window.innerWidth`.** Every
 * other consumer of this breakpoint — `useIsPhone`, and `responsive.css`'s
 * `@media` literals — asks the media query, and `innerWidth` is not the same
 * question: it counts a classic scrollbar's width and it tracks the VISUAL
 * (pinch-zoomed) viewport rather than the layout viewport the query resolves
 * against. So the two disagree in a narrow band, and they disagreed in the
 * harmful direction: a view already rendering the phone form could still be
 * handed the popup, which on that surface can never come back.
 *
 * The `matchMedia` guard is `useIsPhone`'s, and for the reason its header
 * spells out at length: this repo's jsdom does not implement `matchMedia` at
 * all, so an unguarded call throws. "No `matchMedia`" is read as "not a phone",
 * which is both the honest answer to a viewport question a runtime cannot
 * answer and the safe one — it selects the popup, and a test environment has
 * no popup to lose. */
function shouldUseRedirect(): boolean {
  if (isStandalone()) {
    return true;
  }
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
    return false;
  }
  return window.matchMedia(PHONE_QUERY).matches;
}

export interface CalendarWiring {
  handleConnectClick: () => Promise<void>;
  handleCalendarSelectionChange: (selectedCalendarIds: string[]) => void;
  handleRefreshClick: () => void;
}

export function useCalendarWiring(
  worker: WorkerLike,
  status: CoreStatus,
  calendar: CalendarState,
  /** #121: the designated Trips calendar, read off the synced `settings`
   * table (`calendar/selection.ts`'s `tripsCalendarId`). It contributes to
   * the polled set at every push site below — derived, never persisted. */
  tripsCalendarId: string | null,
): CalendarWiring {
  const [expiresAtMs, setExpiresAtMs] = useState<number | null>(null);

  // When the last credential-recovery retry went out (0 = never). See
  // `resumeAfterReconnect`.
  const lastRecoveryPollAtRef = useRef(0);

  // The silent re-mint's running health (`calendar/remint-health.ts`). A ref,
  // not state: nothing renders from the counter itself — only from the
  // `blocked` flag, which is mirrored into the store below — and making it
  // state would re-run every effect keyed on this hook for a number no view
  // reads. Mirrored rather than derived because the effects that must bail
  // out read `calendar.silentRemintBlocked`, and a ref read inside an effect
  // would not re-run it when the value changed.
  const remintHealthRef = useRef<RemintHealth>(INITIAL_REMINT_HEALTH);

  // Whether a redirect attempt was started from THIS document and has not
  // been answered. Only the restore watcher below reads it; see
  // `calendar/connect-pending.ts` for why the reset is predicated on it rather
  // than fired at every resume.
  const redirectInFlightRef = useRef(false);

  /** Folds one silent-re-mint outcome in, and publishes `blocked` when it
   * moves. Every silent path calls this — start, credential-needed and the
   * rotation timer — so there is no third place a failure can be forgotten. */
  function recordRemint(result: ConnectionResult) {
    publishRemintHealth(recordSilentRemint(remintHealthRef.current, result.error));
  }

  /** Folds an INTERACTIVE connect outcome in — the button, by either route.
   * Both interactive paths call this, and they must: `recordRemint` above is
   * reached only from the three silent paths, so before this existed a
   * successful Reconnect left `blocked` standing for the rest of the page's
   * life. Both effects that drive the silent path bail on that flag, so the
   * calendar went quietly stale immediately after the very gesture that fixed
   * it, with nothing on screen saying why. */
  function recordInteractive(result: ConnectionResult) {
    publishRemintHealth(recordInteractiveConnect(remintHealthRef.current, result.error));
  }

  /** The one place `blocked` reaches the store, so the mirror cannot be
   * updated by one recorder and forgotten by the other. */
  function publishRemintHealth(next: RemintHealth) {
    const before = remintHealthRef.current.blocked;
    remintHealthRef.current = next;
    if (next.blocked !== before) {
      coreStore.setCalendarState({ silentRemintBlocked: next.blocked });
    }
  }

  // Every push of the selection goes through here (#121), so the derived
  // union — ticked calendars ∪ the bound Trips calendar, each with its
  // horizon — cannot be applied at four call sites and forgotten at the
  // fifth.
  function pushSelection(storedIds: string[]) {
    setCalendarSelectionsOnWorker(worker, effectiveSelection(storedIds, tripsCalendarId));
  }

  function connectionDeps(): ConnectionDeps | null {
    const client = tokenClient();
    if (!client) {
      return null;
    }
    return {
      tokenClient: client,
      pushToken: (token) => pushTokenToWorker(worker, token),
    };
  }

  // Re-polls and re-reads the tile after a credential *recovery* — the
  // credential-needed round-trip below, not the proactive rotation timer
  // (which re-mints while polling is healthy and has no abandoned poll to
  // retry).
  function resumeAfterReconnect(result: ConnectionResult) {
    if (!result.connected || result.needsReconnect) {
      return;
    }
    // At most one recovery poll per timer interval. The retry closes a loop
    // — its own 401 records another credential event, which flips
    // `needsReconnect` back on and re-enters this path — so a token the
    // authority keeps minting and Google keeps rejecting (a revoked scope,
    // say) would otherwise spin re-mint/poll pairs as fast as the network
    // allows. The cooldown makes the pathological case degrade to exactly
    // the cadence this recovery replaced, and never worse.
    const now = Date.now();
    if (now - lastRecoveryPollAtRef.current < TIMER_INTERVAL_MS) {
      return;
    }
    lastRecoveryPollAtRef.current = now;
    // Re-assert the selection first, for the same reason every other poll
    // trigger here does: a worker restarted by the browser holds an empty
    // one, and a zero-calendar poll succeeds with an empty snapshot that
    // would replace the last good one.
    pushSelection(calendar.selectedCalendarIds);
    pollRefresh(worker, now);
    requestCalendarList(worker);
  }

  // Core-start wiring: attempt a silent re-mint for a previously-connected
  // device, or stay disconnected (Agent Brief: "a never-connected device
  // has no tile and unconstrained ranking").
  useEffect(() => {
    if (status !== "ready") {
      return;
    }
    const deps = connectionDeps();
    if (!deps) {
      return;
    }
    let cancelled = false;
    void (async () => {
      const wasConnected = readConnected(localStorage);
      const selectedCalendarIds = readSelectedCalendarIds(localStorage);
      // A return from the redirect flow lands here, not in a click handler:
      // the round-trip is a full page load, so the component that started it
      // no longer exists. `takeOAuthRedirect` is one-shot, which is what keeps
      // StrictMode's double-invoke from applying the same token twice.
      const redirect = takeOAuthRedirect();
      const attempt =
        redirect.kind === "none"
          ? await initConnection(deps, wasConnected)
          : applyRedirect(redirect, deps);
      if (cancelled) {
        return;
      }
      // What the attempt DID is `attempt`; what this load should apply is
      // `result`, and for a redirect return the two differ. A failed return —
      // a cancelled consent, or merely a crafted `#error=…` link somebody
      // opened — must not un-connect a device that was connected before it
      // left, exactly as the popup path's `shouldKeepExistingConnection` check
      // must not. `calendar/redirect-return.ts` holds that decision and its
      // header explains why the remedy here has to be a positive write rather
      // than the popup's "return and touch nothing": on this path the store is
      // still at its initial disconnected default, so touching nothing is
      // itself the wipe.
      const result =
        redirect.kind === "none" ? attempt : resolveRedirectReturn(wasConnected, attempt);
      if (redirect.kind === "none") {
        if (wasConnected) {
          // Only then was this a silent re-mint. A never-opted-in device did
          // not attempt one.
          recordRemint(attempt);
        }
      } else {
        // A redirect return is the interactive path, and a successful one is
        // as good a reason to lift a silent block as a silent success is.
        recordInteractive(attempt);
        // The redirect path is the one that can report a failure at start-up:
        // an ordinary open has nothing to say, and a silent re-mint's failure
        // is `needsReconnect`, not a message. Written unconditionally so a
        // successful return also clears whatever the last attempt left — and
        // the error is `attempt`'s, not `result`'s, because keeping the
        // connection is about `connected`/`needsReconnect` only: a reconnect
        // that failed is precisely when the reader needs telling.
        coreStore.setCalendarState({ connectPending: false, connectError: attempt.error });
      }
      writeConnected(localStorage, result.connected);
      coreStore.setCalendarState({
        connected: result.connected,
        needsReconnect: result.needsReconnect,
        selectedCalendarIds,
      });
      setExpiresAtMs(result.expiresAtMs);
      if (result.connected) {
        // Both of these run even when the silent re-mint failed — the
        // offline-start case, which is exactly when they matter most.
        //
        // Because the worker was constructed with an empty selection:
        // leaving it empty means a later Reconnect polls zero calendars, and
        // a zero-calendar poll SUCCEEDS with an empty snapshot, overwriting
        // the last good one (`fetch_calendar_snapshot` simply iterates no
        // ids). The last-good snapshot is a previously-connected device's
        // only offline context; a reconnect must not be able to destroy it.
        pushSelection(selectedCalendarIds);
      }
      if (result.connected && !result.needsReconnect) {
        pollStart(worker, Date.now());
        requestCalendarList(worker);
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [status]);

  // Answers a credential-needed round-trip from the core: silent re-mint
  // first, falling back to the re-connect affordance in Settings.
  useEffect(() => {
    // `silentRemintBlocked` bails out here rather than in `handleCredentialNeeded`:
    // `needsReconnect` stays standing, so the Reconnect affordance and the
    // last-good snapshot both survive and the reader sees stale-but-honest
    // context instead of an app that has gone dark. What stops is only the
    // hourly attempt that cannot succeed.
    if (!calendar.needsReconnect || calendar.silentRemintBlocked) {
      return;
    }
    const deps = connectionDeps();
    if (!deps) {
      return;
    }
    let cancelled = false;
    void (async () => {
      const result = await handleCredentialNeeded(deps);
      if (cancelled) {
        return;
      }
      recordRemint(result);
      coreStore.setCalendarState({
        connected: result.connected,
        needsReconnect: result.needsReconnect,
      });
      setExpiresAtMs(result.expiresAtMs);
      // The poll that provoked this round-trip was abandoned mid-flight and
      // nothing else retries it: the foreground timer restarts from zero, so
      // without this the tile stays stale for up to another 15 minutes after
      // a recovery the user never saw fail.
      resumeAfterReconnect(result);
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [calendar.needsReconnect, calendar.silentRemintBlocked]);

  // Proactive rotation. This timer is what fired the hourly popup before
  // #583 — the popup is gone because `google/gis.ts` is gone, not because
  // this timer went with it. The access token the authority hands back is
  // still ~1 hour long (ADR-0028: the authority itself holds the refresh
  // token now, but does not push one down), so a device that only re-minted
  // reactively would still take a live 401 on every calendar poll that
  // landed after expiry. Re-minting ahead of it, here, is what keeps a
  // long-lived session from ever needing the reactive credential-needed
  // path above — and it is now one same-origin POST, not a popup, so
  // nothing about running it every ~55 minutes is disruptive any more.
  useEffect(() => {
    if (
      !calendar.connected ||
      calendar.needsReconnect ||
      calendar.silentRemintBlocked ||
      expiresAtMs === null
    ) {
      return;
    }
    const deps = connectionDeps();
    if (!deps) {
      return;
    }
    const delayMs = msUntilRotation(expiresAtMs, Date.now());
    const id = window.setTimeout(() => {
      void (async () => {
        const result = await handleCredentialNeeded(deps);
        recordRemint(result);
        coreStore.setCalendarState({
          connected: result.connected,
          needsReconnect: result.needsReconnect,
        });
        setExpiresAtMs(result.expiresAtMs);
      })();
    }, delayMs);
    return () => window.clearTimeout(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [calendar.connected, calendar.needsReconnect, calendar.silentRemintBlocked, expiresAtMs]);

  // #121: a `trips-calendar` binding edited on ANY device reaches this one
  // through the ordinary delta pull, and the polled set is derived from it —
  // so the moment it moves, the worker's copy of the selection is stale and
  // the newly-designated calendar has never been fetched. Re-push and poll,
  // which is exactly what the picker does for a tick.
  useEffect(() => {
    if (status !== "ready" || !calendar.connected || calendar.needsReconnect) {
      return;
    }
    pushSelection(calendar.selectedCalendarIds);
    pollRefresh(worker, Date.now());
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [status, calendar.connected, calendar.needsReconnect, tripsCalendarId]);

  // Un-sticks a Connect button left spinning by a redirect that was never
  // answered — the Back button from Google's consent screen, where the page
  // usually comes back from bfcache and no module scope, no effect and no
  // start-up path re-runs at all. Mount-once: the listeners are the document's
  // and the decision behind them is in `calendar/connect-pending.ts`.
  useEffect(() => {
    return watchConnectPendingRestore({
      isRedirectInFlight: () => redirectInFlightRef.current,
      onRestore: () => {
        // The marker is cleared with the flag, so a later resume does not keep
        // firing against an attempt that is over.
        redirectInFlightRef.current = false;
        // `connectError` is deliberately left alone: nothing failed. The
        // reader chose to come back, and inventing an error for a gesture they
        // abandoned would be the app telling them something untrue.
        coreStore.setCalendarState({ connectPending: false });
      },
    });
  }, []);

  // The foreground 15-minute context-poll timer (#46, under ADR-0005).
  useEffect(() => {
    if (!calendar.connected || calendar.needsReconnect) {
      return;
    }
    const id = window.setInterval(() => {
      if (document.hidden || !navigator.onLine) {
        return;
      }
      pollTimer(worker, Date.now());
    }, TIMER_INTERVAL_MS);
    return () => window.clearInterval(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [calendar.connected, calendar.needsReconnect]);

  async function handleConnectClick() {
    const deps = connectionDeps();
    if (!deps) {
      return;
    }
    const wasConnected = calendar.connected;
    coreStore.setCalendarState({ connectPending: true, connectError: null });
    if (GOOGLE_CLIENT_ID && shouldUseRedirect()) {
      // The document is about to be replaced, so nothing after this runs and
      // there is nothing to await. The result comes back through the start
      // effect above on the next load. `connectPending` stays set, which is
      // correct: the attempt genuinely is in flight, and if the navigation
      // fails to happen at all the button stays visibly busy rather than
      // silently idle.
      //
      // Marked in-flight FIRST, before the navigation: this ref is what the
      // restore watcher above consults to tell "the reader pressed Back out of
      // Google" from "the reader switched tabs while a popup is open", and it
      // survives the round trip only in the bfcache case — which is precisely
      // the case that has no other way back.
      redirectInFlightRef.current = true;
      startOAuthRedirect(GOOGLE_CLIENT_ID);
      return;
    }
    const result = await connect(deps);
    // A successful popup connect is a success for the silent path's health
    // too, and lifts a block; a failed one is not evidence either way. See
    // `recordInteractive`.
    recordInteractive(result);
    // The error is written FIRST, above the early return below — not folded
    // into it. A failed *reconnect* takes that return, and before this the
    // handler left without touching any state at all: the press produced
    // nothing on screen, which is the reported bug in its purest form. A
    // reconnect failing is exactly when the reader needs telling.
    // `connection.ts`'s `shouldKeepExistingConnection` doc records this
    // ordering too; both say it because moving it back is a one-line edit
    // that looks like tidying.
    coreStore.setCalendarState({ connectPending: false, connectError: result.error });
    if (shouldKeepExistingConnection(wasConnected, result)) {
      // A cancelled or failed *Reconnect*: keep the opt-in, the last-good
      // tile and the Reconnect button exactly as they were, so the user can
      // simply try again. (`needsReconnect` is untouched, so the rotation
      // timer stays parked and the stale `expiresAtMs` below is never used.)
      return;
    }
    writeConnected(localStorage, result.connected);
    coreStore.setCalendarState({
      connected: result.connected,
      needsReconnect: result.needsReconnect,
    });
    setExpiresAtMs(result.expiresAtMs);
    if (result.connected) {
      // Re-assert the selection before polling. This click is also the
      // Reconnect button, and the worker's copy of the selection can be
      // empty here (a startup whose silent re-mint failed, a worker
      // restarted by the browser) — polling with an empty selection would
      // quietly replace the last good snapshot with an empty one.
      pushSelection(calendar.selectedCalendarIds);
      pollStart(worker, Date.now());
      requestCalendarList(worker);
    }
  }

  function handleCalendarSelectionChange(requestedCalendarIds: string[]) {
    // #121: the bound Trips calendar's row is locked in the picker, and the
    // refusal is repeated here rather than trusted to it — a handler that
    // accepted the untick and let `effectiveSelection` silently re-add the
    // calendar would leave the picker showing a control that springs back.
    const selectedCalendarIds = acceptSelectionChange(requestedCalendarIds, tripsCalendarId);
    if (selectedCalendarIds === null) {
      return;
    }
    writeSelectedCalendarIds(localStorage, selectedCalendarIds);
    coreStore.setCalendarState({ selectedCalendarIds });
    pushSelection(selectedCalendarIds);
    pollRefresh(worker, Date.now());
  }

  // The user-facing manual refresh (#46/#72). Without it the only way to
  // retry a transient poll failure is to change the calendar selection,
  // reload the app, or wait out the 15-minute timer — and the tile shows a
  // stale "as of" precisely when someone wants a retry.
  function handleRefreshClick() {
    // Re-assert the selection first, for the same reason the connect path
    // does: a worker restarted by the browser holds an empty one, and a
    // zero-calendar poll succeeds with an empty snapshot that would replace
    // the last good one.
    pushSelection(calendar.selectedCalendarIds);
    pollRefresh(worker, Date.now());
  }

  return { handleConnectClick, handleCalendarSelectionChange, handleRefreshClick };
}
