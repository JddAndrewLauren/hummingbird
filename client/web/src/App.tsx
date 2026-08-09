import { useState } from "react";
import { contextTileProps } from "./calendar/tile-props";
import { demoData } from "./fixtures/demo";
import { AlertsScreen } from "./screens/AlertsScreen";
import { NowScreen } from "./screens/NowScreen";
import { RoutesScreen } from "./screens/RoutesScreen";
import { SettingsScreen } from "./screens/SettingsScreen";
import { TriageScreen } from "./screens/TriageScreen";
import { Header } from "./shell/Header";
import { NavRail } from "./shell/NavRail";
import { SCREEN_TITLES, type Screen } from "./shell/screens";
import { coreStatusLabel } from "./shell/status-label";
import { useCalendarWiring } from "./shell/useCalendarWiring";
import { useStore } from "./store/useStore";
import type { WorkerLike } from "./store/worker-client";
import { toggledPreference } from "./theme/theme";
import { useTheme } from "./theme/useTheme";

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

// The app shell (#107's decomposition): a fixed nav rail, a header, and one
// of five screens. Screens switch on local state rather than a router —
// there are no deep links to honour yet, and a router would be a dependency
// carrying no weight.
//
// Every decision is delegated: the calendar lifecycle to `useCalendarWiring`,
// the theme to `useTheme`, and each display decision to a unit-tested pure
// module. What is not backed by a real feature is not rendered — demo mode
// (dev-only) is the one place fixtures appear.
export function App({ worker: injectedWorker }: AppProps = {}) {
  const status = useStore((state) => state.status);
  const apiVersion = useStore((state) => state.apiVersion);
  const error = useStore((state) => state.error);
  const calendar = useStore((state) => state.calendar);

  // A lazy initializer rather than a ref: reading `ref.current` during render
  // is what React's rules forbid, and this needs to be constructed exactly
  // once per mount either way.
  const [worker] = useState<WorkerLike>(() => injectedWorker ?? realWorker());

  // Lazy initializer, not a ref: `demoData()` returns null in production, and
  // `ref.current ??= …` would re-run it on every render forever.
  const [demo] = useState(demoData);

  const [screen, setScreen] = useState<Screen>("now");
  const { preference, theme, setPreference } = useTheme();
  const {
    nowMs,
    handleConnectClick,
    handleCalendarSelectionChange,
    handleRefreshClick,
  } = useCalendarWiring(worker, status, calendar);

  const tile = contextTileProps(calendar, nowMs);

  // `worker-client.ts`'s postMessage wrappers may only be called once the core
  // reports `ready`, and a refresh on a device with no calendar opt-in would
  // poll an empty selection — the one thing the wiring hook works to prevent.
  // No usable refresh, no button.
  const canRefresh = status === "ready" && calendar.connected && !calendar.needsReconnect;

  return (
    <div
      style={{
        display: "flex",
        height: "100dvh",
        overflow: "hidden",
        background: "var(--surface-page)",
      }}
    >
      <NavRail
        screen={screen}
        onScreen={setScreen}
        counts={demo ? { triage: demo.triage.length, alerts: demo.alerts.length } : {}}
        statusLabel={coreStatusLabel(status, apiVersion)}
        theme={theme}
        onToggleTheme={() => setPreference(toggledPreference(theme))}
      />

      <main style={{ display: "flex", flex: 1, minWidth: 0, flexDirection: "column" }}>
        <Header
          title={SCREEN_TITLES[screen]}
          syncLabel={demo?.syncBadge}
          onRefresh={canRefresh ? handleRefreshClick : undefined}
          onCapture={() => setScreen("triage")}
        />

        {/* The one scroll container: the design README fixes the rail and
            the context panel, and lets only the centre column move. */}
        <div
          style={{
            flex: 1,
            minHeight: 0,
            overflowY: "auto",
            padding: "0 var(--gutter-page) var(--space-11)",
          }}
        >
          {screen === "now" && <NowScreen demo={demo} tile={tile} onScreen={setScreen} />}
          {screen === "triage" && <TriageScreen demo={demo} />}
          {screen === "routes" && <RoutesScreen demo={demo} />}
          {screen === "alerts" && <AlertsScreen demo={demo} />}
          {screen === "settings" && (
            <SettingsScreen
              demo={demo}
              status={status}
              apiVersion={apiVersion}
              error={error}
              calendar={calendar}
              themePreference={preference}
              onThemePreference={setPreference}
              onConnect={() => void handleConnectClick()}
              onSelectionChange={handleCalendarSelectionChange}
              onRefresh={handleRefreshClick}
            />
          )}
        </div>
      </main>
    </div>
  );
}
