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
import {
  readConnected,
  readSelectedCalendarIds,
  writeConnected,
  writeSelectedCalendarIds,
} from "../calendar/persistence";
import {
  INITIAL_REMINT_HEALTH,
  recordSilentRemint,
  type RemintHealth,
} from "../calendar/remint-health";
import { acceptSelectionChange, effectiveSelection } from "../calendar/selection";
import { createGisTokenClient, type TokenClient } from "../google/gis";
import type { RedirectOutcome } from "../google/redirect-flow";
import { PHONE_MAX_WIDTH_PX } from "./breakpoints";
import { startOAuthRedirect, takeOAuthRedirect } from "./oauth-redirect";
import { isStandalone } from "./standalone";
import { coreStore, type CalendarState, type CoreStatus } from "../store/store";
import {
  pollRefresh,
  pollStart,
  pollTimer,
  pushTokenToWorker,
  requestCalendarList,
  setCalendarSelectionsOnWorker,
  type WorkerLike,
} from "../store/worker-client";

// The web host's calendar opt-in wiring (issue #73): GIS consent, the
// proactive token rotation, the context poll cadence and the clock tick that
// keeps staleness honest. Extracted from App.tsx unchanged when the shell was
// decomposed (#107) — these effects must run for the app's whole lifetime
// regardless of which screen is mounted, so the shell calls this hook once
// and passes the handlers down.
//
// Everything here is intentionally thin: every decision (silent re-mint vs.
// re-connect, staleness, selection toggling) is delegated to a unit-tested
// pure module under calendar/ or google/; this file only wires their results
// into the store and the worker.

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

export const GOOGLE_CLIENT_ID: string | undefined = import.meta.env.VITE_GOOGLE_CLIENT_ID;

let cachedTokenClient: TokenClient | null = null;
function tokenClient(): TokenClient | null {
  if (!GOOGLE_CLIENT_ID) {
    return null;
  }
  cachedTokenClient ??= createGisTokenClient(GOOGLE_CLIENT_ID);
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

/** Whether this device should use the redirect flow instead of GIS's popup.
 *
 * Standalone is the real criterion — an installed iOS web app is where the
 * popup escapes to Safari and loses its opener, and where Safari's own storage
 * container is no use because the app cannot see it. A phone-sized browser tab
 * is included because the same popup on a small screen is a full-screen
 * takeover with no visible relationship to the app it came from, and because a
 * phone tab is one "Add to Home Screen" away from being the standalone case
 * anyway. A desktop keeps GIS: it works there, and it is the less disruptive
 * of the two. */
function shouldUseRedirect(): boolean {
  return isStandalone() || (typeof window !== "undefined" && window.innerWidth <= PHONE_MAX_WIDTH_PX);
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

  /** Folds one silent-re-mint outcome in, and publishes `blocked` when it
   * moves. Every silent path calls this — start, credential-needed and the
   * rotation timer — so there is no third place a failure can be forgotten. */
  function recordRemint(result: ConnectionResult) {
    const before = remintHealthRef.current.blocked;
    remintHealthRef.current = recordSilentRemint(remintHealthRef.current, result.error);
    if (remintHealthRef.current.blocked !== before) {
      coreStore.setCalendarState({ silentRemintBlocked: remintHealthRef.current.blocked });
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
    // `needsReconnect` back on and re-enters this path — so a token GIS
    // keeps minting and Google keeps rejecting (a revoked scope, say) would
    // otherwise spin re-mint/poll pairs as fast as the network allows. The
    // cooldown makes the pathological case degrade to exactly the cadence
    // this recovery replaced, and never worse.
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
      const result =
        redirect.kind === "none"
          ? await initConnection(deps, wasConnected)
          : applyRedirect(redirect, deps);
      if (cancelled) {
        return;
      }
      if (redirect.kind === "none" && wasConnected) {
        // Only then was this a silent re-mint. A never-opted-in device did
        // not attempt one, and a redirect return is the interactive path.
        recordRemint(result);
      }
      // The redirect path is the one that can report a failure at start-up:
      // an ordinary open has nothing to say, and a silent re-mint's failure is
      // `needsReconnect`, not a message. Written unconditionally so a
      // successful return also clears whatever the last attempt left.
      if (redirect.kind !== "none") {
        coreStore.setCalendarState({ connectPending: false, connectError: result.error });
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

  // Proactive rotation: GIS issues no refresh token, so a fresh silent
  // re-mint ahead of the current token's expiry is what keeps a long-lived
  // session from ever needing the reactive credential-needed path above.
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
      startOAuthRedirect(GOOGLE_CLIENT_ID);
      return;
    }
    const result = await connect(deps);
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
