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
  recordInteractiveConnect,
  recordSilentRemint,
  type RemintHealth,
} from "../calendar/remint-health";
import { acceptSelectionChange, effectiveSelection } from "../calendar/selection";
import { createAuthorityTokenClient } from "../calendar/authority-token-client";
import type { TokenClient } from "../calendar/token-client";
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
// required) is delegated to a unit-tested pure module under `calendar/`;
// this file only wires their results into the store and the worker.
//
// **#577/#583/#584: there is one path, and it never opens a popup or leaves
// the page.** Every `TokenClient` call — the start-up re-mint, the proactive
// rotation below, a core 401's credential-needed round-trip, and an
// interactive Connect/Reconnect press — reaches
// `calendar/authority-token-client.ts`, a same-origin authenticated POST to
// `POST /api/google/calendar_token` (ADR-0028), usually answered from the
// authority's own cache. `google/gis.ts` (#583) and `google/redirect-flow.ts`
// (#584) are both gone, and with the redirect flow went the only reason this
// hook ever cared whether the view was standalone or phone-sized, or whether
// a click needed to survive a full page navigation. A Connect press now
// simply awaits `connect()`, the same way the silent paths await
// `initConnection`/`handleCredentialNeeded` — one shape, one file, no branch
// on how this view happens to be presented.

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

// Still gates the "Google calendar" section's rendering in Settings
// (`SettingsScreen.tsx` imports this) even though the authority-backed
// `TokenClient` below needs no client id at all — it authenticates with the
// device token, not this. What this gate should mean now that nothing left
// in the browser talks to Google directly is #585's question ("Settings
// tells the truth about what a calendar connection needs"), not this
// slice's.
export const GOOGLE_CLIENT_ID: string | undefined = import.meta.env.VITE_GOOGLE_CLIENT_ID;

let cachedTokenClient: TokenClient | null = null;
let cachedTaskTokenStore: TaskTokenStoreLike | null = null;
function tokenClient(): TokenClient {
  cachedTaskTokenStore ??= createIndexedDbTaskTokenStore();
  const taskTokenStore = cachedTaskTokenStore;
  cachedTokenClient ??= createAuthorityTokenClient({
    fetch: globalThis.fetch.bind(globalThis),
    readToken: async () => (await taskTokenStore.read())?.token ?? null,
  });
  return cachedTokenClient;
}

/** Builds the deps every connection call below needs. Cannot fail: the
 * authority-backed `TokenClient` authenticates with the stored device token
 * (or reports `no_device_token` itself, same as any other request failure),
 * not with anything this function could find absent. */
function connectionDeps(worker: WorkerLike): ConnectionDeps {
  return {
    tokenClient: tokenClient(),
    pushToken: (token) => pushTokenToWorker(worker, token),
  };
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
    publishRemintHealth(recordSilentRemint(remintHealthRef.current, result.error));
  }

  /** Folds an INTERACTIVE connect outcome in — the Connect/Reconnect button.
   * Kept distinct from `recordRemint`: before this existed a successful
   * Reconnect left `blocked` standing for the rest of the page's life. Both
   * effects that drive the silent path bail on that flag, so the calendar
   * went quietly stale immediately after the very gesture that fixed it,
   * with nothing on screen saying why. */
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
    const deps = connectionDeps(worker);
    let cancelled = false;
    void (async () => {
      const wasConnected = readConnected(localStorage);
      const selectedCalendarIds = readSelectedCalendarIds(localStorage);
      const result = await initConnection(deps, wasConnected);
      if (cancelled) {
        return;
      }
      if (wasConnected) {
        // Only then was this a silent re-mint. A never-opted-in device did
        // not attempt one.
        recordRemint(result);
      }
      writeConnected(localStorage, result.connected);
      coreStore.setCalendarState({
        connected: result.connected,
        needsReconnect: result.needsReconnect,
        selectedCalendarIds,
      });
      setExpiresAtMs(result.expiresAtMs);
      if (result.connected) {
        // Runs even when the silent re-mint failed — the offline-start
        // case, which is exactly when this matters most.
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
    const deps = connectionDeps(worker);
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
    const deps = connectionDeps(worker);
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
    const deps = connectionDeps(worker);
    const wasConnected = calendar.connected;
    coreStore.setCalendarState({ connectPending: true, connectError: null });
    const result = await connect(deps);
    // A successful connect is a success for the silent path's health too,
    // and lifts a block; a failed one is not evidence either way. See
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
