import { useEffect, useRef, useState } from "react";
import { CalendarPicker } from "./calendar/CalendarPicker";
import { type CalendarListEntry, listCalendars } from "./calendar/calendarList";
import {
  connect,
  type ConnectionDeps,
  handleCredentialNeeded,
  initConnection,
  msUntilRotation,
  shouldKeepExistingConnection,
} from "./calendar/connection";
import { ContextTile } from "./calendar/ContextTile";
import {
  readConnected,
  readSelectedCalendarIds,
  writeConnected,
  writeSelectedCalendarIds,
} from "./calendar/persistence";
import { createGisTokenClient, type TokenClient } from "./google/gis";
import { coreStore } from "./store/store";
import { useStore } from "./store/useStore";
import {
  pollRefresh,
  pollStart,
  pollTimer,
  pushTokenToWorker,
  requestCurrentNext,
  setCalendarIdsOnWorker,
  type WorkerLike,
} from "./store/worker-client";

// The 15-minute foreground timer (ADR-0007). The host is responsible for
// only ticking while online and foregrounded — `document.hidden`/
// `navigator.onLine` gate every tick below.
const TIMER_INTERVAL_MS = 15 * 60 * 1000;

// How often the "as of" label / stale styling re-samples the clock. This is
// independent of the poll timer above (which early-returns while
// `needsReconnect` is true): a credential hold must not freeze the
// staleness display, so this ticks any time a tile is showing at all,
// including during a credential hold. `formatAsOf`'s coarsest unit is a
// minute, so a much finer tick would just be wasted renders.
const CLOCK_TICK_MS = 30 * 1000;

const GOOGLE_CLIENT_ID: string | undefined = import.meta.env.VITE_GOOGLE_CLIENT_ID;

let cachedTokenClient: TokenClient | null = null;
function tokenClient(): TokenClient | null {
  if (!GOOGLE_CLIENT_ID) {
    return null;
  }
  cachedTokenClient ??= createGisTokenClient(GOOGLE_CLIENT_ID);
  return cachedTokenClient;
}

function realWorker(): WorkerLike {
  return new Worker(new URL("./worker/core.worker.ts", import.meta.url), {
    type: "module",
  }) as unknown as WorkerLike;
}

interface AppProps {
  /** The worker `App` talks to. Defaults to a lazily-constructed real Web
   * Worker; overridable so this component could be driven by a fake in a
   * future DOM-environment test without touching production wiring. */
  worker?: WorkerLike;
}

// The web host's calendar opt-in surface (issue #73): GIS consent, the
// calendar picker, and the current/next context tile. #69's placeholder
// shell now also carries this — see the Agent Brief for the acceptance
// criteria this satisfies. This component is intentionally thin: every
// decision (silent re-mint vs. re-connect, staleness, selection toggling)
// is delegated to a unit-tested pure module under calendar/ or google/; this
// file only wires their results into the store and the worker.
export function App({ worker: injectedWorker }: AppProps = {}) {
  const status = useStore((state) => state.status);
  const apiVersion = useStore((state) => state.apiVersion);
  const error = useStore((state) => state.error);
  const calendar = useStore((state) => state.calendar);

  const workerRef = useRef<WorkerLike | null>(null);
  workerRef.current ??= injectedWorker ?? realWorker();
  const worker = workerRef.current;

  // The most recently pushed access token, kept only in memory (never
  // persisted — same in-memory-only discipline as #72's core-side
  // `CredentialState`) so the calendar-list fetch below can reuse it
  // instead of minting a second token for the same consent.
  const lastAccessTokenRef = useRef<string | null>(null);
  const [calendars, setCalendars] = useState<CalendarListEntry[]>([]);
  const [expiresAtMs, setExpiresAtMs] = useState<number | null>(null);
  const [nowMs, setNowMs] = useState<number>(() => Date.now());

  function connectionDeps(): ConnectionDeps | null {
    const client = tokenClient();
    if (!client) {
      return null;
    }
    return {
      tokenClient: client,
      pushToken: (token) => {
        lastAccessTokenRef.current = token;
        pushTokenToWorker(worker, token);
      },
    };
  }

  async function refreshCalendarList() {
    const token = lastAccessTokenRef.current;
    if (!token) {
      return;
    }
    try {
      setCalendars(await listCalendars(token));
    } catch {
      // The picker's options are a UX nicety, not a poll dependency: a
      // failed list fetch never blocks polling itself.
    }
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
        await refreshCalendarList();
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [status]);

  // Answers a credential-needed round-trip from the core: silent re-mint
  // first, falling back to the re-connect affordance below.
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

  // The foreground 15-minute timer (ADR-0007).
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
  // only on `calendar.connected` (the tile's own render condition below) —
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
      await refreshCalendarList();
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

  return (
    <main className="flex min-h-screen flex-col items-center justify-center gap-4 bg-slate-950 p-8 text-slate-100">
      <h1 className="text-2xl font-semibold">hummingbird</h1>
      {status === "loading" && <p data-testid="status">Loading core…</p>}
      {status === "ready" && (
        <p data-testid="status">
          Core ready (api v{apiVersion}) — worker + wasm loaded.
        </p>
      )}
      {status === "error" && (
        <p data-testid="status" className="text-red-400">
          Core failed to load: {error}
        </p>
      )}

      {status === "ready" && GOOGLE_CLIENT_ID && (
        <div className="flex w-full max-w-sm flex-col gap-3">
          {!calendar.connected && (
            <button
              type="button"
              onClick={() => void handleConnectClick()}
              className="rounded-md bg-slate-800 px-3 py-2 text-sm font-medium"
            >
              Connect Google Calendar
            </button>
          )}
          {calendar.connected && calendar.needsReconnect && (
            <button
              type="button"
              onClick={() => void handleConnectClick()}
              className="rounded-md bg-amber-800 px-3 py-2 text-sm font-medium"
            >
              Reconnect Google Calendar
            </button>
          )}
          {calendar.connected && (
            <>
              <ContextTile calendar={calendar} nowMs={nowMs} />
              <button
                type="button"
                data-testid="refresh-calendar"
                onClick={handleRefreshClick}
                className="rounded-md border border-slate-800 px-3 py-2 text-sm font-medium text-slate-300"
              >
                Refresh calendar
              </button>
              <CalendarPicker
                calendars={calendars}
                selectedCalendarIds={calendar.selectedCalendarIds}
                onChange={handleCalendarSelectionChange}
              />
            </>
          )}
        </div>
      )}
    </main>
  );
}
