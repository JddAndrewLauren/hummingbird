import { useRef } from "react";
import { CalendarPicker } from "./calendar/CalendarPicker";
import { ContextTile } from "./calendar/ContextTile";
import { GOOGLE_CLIENT_ID, useCalendarWiring } from "./shell/useCalendarWiring";
import { useStore } from "./store/useStore";
import type { WorkerLike } from "./store/worker-client";

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

// The app shell. Every decision it renders is delegated: the calendar
// lifecycle to `useCalendarWiring`, and each display decision to a
// unit-tested pure module.
export function App({ worker: injectedWorker }: AppProps = {}) {
  const status = useStore((state) => state.status);
  const apiVersion = useStore((state) => state.apiVersion);
  const error = useStore((state) => state.error);
  const calendar = useStore((state) => state.calendar);

  const workerRef = useRef<WorkerLike | null>(null);
  workerRef.current ??= injectedWorker ?? realWorker();
  const worker = workerRef.current;

  const {
    nowMs,
    handleConnectClick,
    handleCalendarSelectionChange,
    handleRefreshClick,
  } = useCalendarWiring(worker, status, calendar);

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
                calendars={calendar.availableCalendars}
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
