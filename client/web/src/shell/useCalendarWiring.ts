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
  requestCurrentNext,
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

// How often the "as of" label / stale styling re-samples the clock. This is
// independent of the poll timer above (which early-returns while
// `needsReconnect` is true): a credential hold must not freeze the
// staleness display, so this ticks any time a tile is showing at all,
// including during a credential hold. `formatAsOf`'s coarsest unit is a
// minute, so a much finer tick would just be wasted renders.
const CLOCK_TICK_MS = 30 * 1000;

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
  /** Re-sampled every 30s so the tile's "as of" label stays live. */
  nowMs: number;
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
  const [nowMs, setNowMs] = useState<number>(() => Date.now());

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
    requestCurrentNext(worker, now);
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
        // The selection, because the worker was constructed with an empty
        // one: leaving it empty means a later Reconnect polls zero
        // calendars, and a zero-calendar poll SUCCEEDS with an empty
        // snapshot, overwriting the last good one (`fetch_calendar_snapshot`
        // simply iterates no ids). The last-good snapshot is a
        // previously-connected device's only offline context; a reconnect
        // must not be able to destroy it.
        //
        // The current/next request, because nothing else asks for one until
        // a poll completes — and a held credential means no poll completes.
        // Without this the persisted IndexedDB snapshot is never read and
        // the tile claims "no current or upcoming event" on a device that
        // has one, which is worse than showing it honestly stale.
        setCalendarIdsOnWorker(worker, selectedCalendarIds);
        requestCurrentNext(worker, Date.now());
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

  // Keeps the context tile's "as of" label / stale styling live. Gated
  // only on `calendar.connected` (the tile's own render condition) —
  // deliberately NOT also gated on `!calendar.needsReconnect` like the poll
  // timer effect above, so staleness keeps advancing (and correctly turns
  // "stale") exactly during a credential hold, instead of freezing at
  // whatever `Date.now()` happened to be sampled at the last render before
  // the hold began.
  useEffect(() => {
    if (!calendar.connected) {
      return;
    }
    const id = window.setInterval(() => {
      const now = Date.now();
      setNowMs(now);
      // Re-query current/next, not just the clock. "Now" and "Next" are
      // answers about a moment, and the moment moves: without this the tile
      // keeps saying "Next" after an event has started and "Now" after it
      // has ended, until the 15-minute network poll happens to correct it.
      // This read is local (the persisted snapshot, no network), so it is
      // also correct to run during a credential hold or offline.
      requestCurrentNext(worker, now);
    }, CLOCK_TICK_MS);
    return () => window.clearInterval(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [calendar.connected]);

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
    requestCurrentNext(worker, Date.now());
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
    requestCurrentNext(worker, Date.now());
  }

  return { nowMs, handleConnectClick, handleCalendarSelectionChange, handleRefreshClick };
}
