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
import { createGisTokenClient, type TokenClient } from "../google/gis";
import { coreStore, type CalendarState, type CoreStatus } from "../store/store";
import {
  pollRefresh,
  pollStart,
  pollTimer,
  pushTokenToWorker,
  requestCalendarList,
  setCalendarIdsOnWorker,
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

export interface CalendarWiring {
  handleConnectClick: () => Promise<void>;
  handleCalendarSelectionChange: (selectedCalendarIds: string[]) => void;
  handleRefreshClick: () => void;
}

export function useCalendarWiring(
  worker: WorkerLike,
  status: CoreStatus,
  calendar: CalendarState,
): CalendarWiring {
  const [expiresAtMs, setExpiresAtMs] = useState<number | null>(null);

  // When the last credential-recovery retry went out (0 = never). See
  // `resumeAfterReconnect`.
  const lastRecoveryPollAtRef = useRef(0);

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
    setCalendarIdsOnWorker(worker, calendar.selectedCalendarIds);
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
      const result = await initConnection(deps, wasConnected);
      if (cancelled) {
        return;
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
        setCalendarIdsOnWorker(worker, selectedCalendarIds);
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
    if (!calendar.needsReconnect) {
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
  }, [calendar.needsReconnect]);

  // Proactive rotation: GIS issues no refresh token, so a fresh silent
  // re-mint ahead of the current token's expiry is what keeps a long-lived
  // session from ever needing the reactive credential-needed path above.
  useEffect(() => {
    if (!calendar.connected || calendar.needsReconnect || expiresAtMs === null) {
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
        coreStore.setCalendarState({
          connected: result.connected,
          needsReconnect: result.needsReconnect,
        });
        setExpiresAtMs(result.expiresAtMs);
      })();
    }, delayMs);
    return () => window.clearTimeout(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [calendar.connected, calendar.needsReconnect, expiresAtMs]);

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
    const result = await connect(deps);
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
      setCalendarIdsOnWorker(worker, calendar.selectedCalendarIds);
      pollStart(worker, Date.now());
      requestCalendarList(worker);
    }
  }

  function handleCalendarSelectionChange(selectedCalendarIds: string[]) {
    writeSelectedCalendarIds(localStorage, selectedCalendarIds);
    coreStore.setCalendarState({ selectedCalendarIds });
    setCalendarIdsOnWorker(worker, selectedCalendarIds);
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
    setCalendarIdsOnWorker(worker, calendar.selectedCalendarIds);
    pollRefresh(worker, Date.now());
  }

  return { handleConnectClick, handleCalendarSelectionChange, handleRefreshClick };
}
