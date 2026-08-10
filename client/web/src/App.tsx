import { useEffect, useState } from "react";
import { contextTileProps } from "./calendar/tile-props";
import { demoData } from "./fixtures/demo";
import { AlertsScreen } from "./screens/AlertsScreen";
import { NowScreen } from "./screens/NowScreen";
import { RoutesScreen } from "./screens/RoutesScreen";
import { SettingsScreen } from "./screens/SettingsScreen";
import { TriageScreen } from "./screens/TriageScreen";
import { isCaptureHotkey } from "./shell/capture-hotkey";
import { Header } from "./shell/Header";
import { NavRail } from "./shell/NavRail";
import { SCREEN_TITLES, type Screen } from "./shell/screens";
import { coreStatusLabel } from "./shell/status-label";
import { syncStatusLabel } from "./shell/sync-status";
import { useCalendarWiring } from "./shell/useCalendarWiring";
import { useCaptureWiring } from "./shell/useCaptureWiring";
import { useFrontierWiring } from "./shell/useFrontierWiring";
import { useItemActions } from "./shell/useItemActions";
import { useItemDetailWiring } from "./shell/useItemDetailWiring";
import { useOnlineStatus } from "./shell/useOnlineStatus";
import { useSyncWiring } from "./shell/useSyncWiring";
import { useTaskTokenWiring } from "./shell/useTaskTokenWiring";
import { taskTokenUiState } from "./task/token-ui";
import { useStore } from "./store/useStore";
import type { WorkerLike } from "./store/worker-client";
import { toggledPreference } from "./theme/theme";
import { useTheme } from "./theme/useTheme";

function realWorker(): WorkerLike {
  // ADR-0010 (#126): one core per origin, in a `SharedWorker`. This
  // component talks to its `port`, not the `SharedWorker` object itself.
  const shared = new SharedWorker(new URL("./worker/core.worker.ts", import.meta.url), {
    type: "module",
  });
  return shared.port as unknown as WorkerLike;
}

interface AppProps {
  /** The port `App` talks to. Defaults to a lazily-constructed connection to
   * the real `SharedWorker`; overridable so this component could be driven
   * by a fake in a future DOM-environment test without touching production
   * wiring. */
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
  const task = useStore((state) => state.task);

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
  const {
    hasToken: hasTaskToken,
    enteredAtMs: taskTokenEnteredAtMs,
    handleSubmitToken: handleSubmitTaskToken,
    handleForgetToken: handleForgetTaskToken,
  } = useTaskTokenWiring(worker, status);
  const taskTokenState = taskTokenUiState(hasTaskToken, task.needsReconnect);

  const online = useOnlineStatus();
  const { nowMs: syncNowMs, handleDownloadMirror } = useSyncWiring(
    worker,
    status,
    task.syncOutcomeSeq,
  );
  useFrontierWiring(worker, status, task.syncOutcomeSeq);
  const {
    selectedItemId,
    openItem: handleOpenItem,
    closeItem: handleCloseItemDetail,
  } = useItemDetailWiring(worker, task.syncOutcomeSeq);
  const { submitCapture } = useCaptureWiring(worker, status, task.syncOutcomeSeq);

  // #110/S12's "always-present ... plus a global hotkey that focuses it"
  // (#98, restated on #110): a counter, not a boolean — `TriageScreen`'s own
  // effect keys off it changing, so a second hotkey press while already on
  // Triage re-focuses the input instead of being a no-op. Bumped by both the
  // hotkey below and the header's Capture button, since both are "take me to
  // the capture box" gestures.
  const [captureFocusRequestId, setCaptureFocusRequestId] = useState(0);
  const requestCaptureFocus = () => {
    setScreen("triage");
    setCaptureFocusRequestId((id) => id + 1);
  };

  // The global focus hotkey (#107's decision: shell level, not a leaf
  // component — `src/App.tsx` is that level). One `keydown` listener for the
  // whole app; the matching rule itself is `capture-hotkey.ts`'s pure
  // `isCaptureHotkey`, so this effect is only ever DOM plumbing.
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      const target = event.target;
      const targetIsEditable =
        target instanceof HTMLElement &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.tagName === "SELECT" ||
          target.isContentEditable);
      if (
        isCaptureHotkey({
          key: event.key,
          ctrlKey: event.ctrlKey,
          metaKey: event.metaKey,
          altKey: event.altKey,
          targetIsEditable,
          isComposing: event.isComposing,
        })
      ) {
        event.preventDefault();
        requestCaptureFocus();
      }
    }
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, []);
  const { act: handleAct } = useItemActions(worker);
  const syncLabel = syncStatusLabel({
    online,
    lastSyncOutcome: task.lastSyncOutcome,
    lastSyncAtMs: task.lastSyncAtMs,
    queueDepth: task.queueDepth,
    nowMs: syncNowMs,
  });

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
          // The demo badge stands in for a real cycle only in demo mode;
          // everywhere else this is now backed by one (S9) — see
          // `sync-status.ts`.
          syncLabel={demo?.syncBadge ?? (hasTaskToken ? syncLabel : undefined)}
          onRefresh={canRefresh ? handleRefreshClick : undefined}
          onCapture={requestCaptureFocus}
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
          {screen === "now" && (
            <NowScreen
              demo={demo}
              tile={tile}
              onScreen={setScreen}
              task={task}
              nowMs={nowMs}
              selectedItemId={selectedItemId}
              onOpenItem={handleOpenItem}
              onCloseItemDetail={handleCloseItemDetail}
              onAct={handleAct}
            />
          )}
          {screen === "triage" && (
            <TriageScreen
              demo={demo}
              task={task}
              onSubmitCapture={submitCapture}
              focusRequestId={captureFocusRequestId}
            />
          )}
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
              taskTokenState={taskTokenState}
              taskTokenEnteredAtMs={taskTokenEnteredAtMs}
              onSubmitTaskToken={handleSubmitTaskToken}
              onForgetTaskToken={() => void handleForgetTaskToken()}
              task={task}
              online={online}
              syncNowMs={syncNowMs}
              onDownloadMirror={handleDownloadMirror}
            />
          )}
        </div>
      </main>
    </div>
  );
}
