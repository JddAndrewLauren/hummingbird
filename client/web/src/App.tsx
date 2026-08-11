import { useEffect, useState } from "react";
import { demoData } from "./fixtures/demo";
import { AlertsScreen } from "./screens/AlertsScreen";
import { DoneScreen } from "./screens/DoneScreen";
import { LedgerScreen } from "./screens/LedgerScreen";
import { NowScreen } from "./screens/NowScreen";
import { RoutesScreen } from "./screens/RoutesScreen";
import { RulesScreen } from "./screens/RulesScreen";
import { SettingsScreen } from "./screens/SettingsScreen";
import { TriageScreen } from "./screens/TriageScreen";
import type { CaptureDestination } from "./screens/capture-destination";
import { isCaptureHotkey } from "./shell/capture-hotkey";
import { CapturePopover } from "./shell/CapturePopover";
import { Header } from "./shell/Header";
import { NavRail } from "./shell/NavRail";
import { readRailCollapsed, writeRailCollapsed } from "./shell/rail-collapse";
import { canRefresh } from "./shell/refresh-gate";
import { SCREEN_TITLES, type Screen } from "./shell/screens";
import { coreStatusLabel } from "./shell/status-label";
import { syncStatusLabel } from "./shell/sync-status";
import { useCalendarEventsWiring } from "./shell/useCalendarEventsWiring";
import { useCalendarWiring } from "./shell/useCalendarWiring";
import { useCaptureWiring } from "./shell/useCaptureWiring";
import { useFrontierWiring } from "./shell/useFrontierWiring";
import { useItemActions } from "./shell/useItemActions";
import { useTriageWiring } from "./shell/useTriageWiring";
import { useBindingsWiring } from "./shell/useBindingsWiring";
import { useItemDetailWiring } from "./shell/useItemDetailWiring";
import { useLedgerWiring } from "./shell/useLedgerWiring";
import { useOnlineStatus } from "./shell/useOnlineStatus";
import { UpdateBanner } from "./shell/UpdateBanner";
import { useAppUpdate } from "./shell/useAppUpdate";
import { usePaneReadsWiring } from "./shell/usePaneReadsWiring";
import { useRulesWiring } from "./shell/useRulesWiring";
import { useSyncWiring } from "./shell/useSyncWiring";
import { useTaskTokenWiring } from "./shell/useTaskTokenWiring";
import { taskTokenUiState } from "./task/token-ui";
import { useStore } from "./store/useStore";
import type { CaptureFields, WorkerLike } from "./store/worker-client";
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
  // Device-local view preference, same storage guard `NowScreen`'s ranked
  // region uses — absent storage means the preference lasts the session.
  const [railCollapsed, setRailCollapsed] = useState(() =>
    readRailCollapsed(typeof localStorage === "undefined" ? undefined : localStorage),
  );
  const handleToggleRailCollapsed = () => {
    const next = !railCollapsed;
    setRailCollapsed(next);
    writeRailCollapsed(typeof localStorage === "undefined" ? undefined : localStorage, next);
  };
  const { preference, theme, setPreference } = useTheme();
  const {
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
  const { ready: updateReady, onReload: handleReload } = useAppUpdate();
  const { nowMs: syncNowMs, handleDownloadMirror, handleManualSync } = useSyncWiring(worker, status);
  useFrontierWiring(worker, status, task.syncOutcomeSeq);
  const {
    selectedItemId,
    openItem: handleOpenItem,
    closeItem: handleCloseItemDetail,
  } = useItemDetailWiring(worker, task.syncOutcomeSeq);
  const { submitCapture } = useCaptureWiring(worker, status, task.syncOutcomeSeq);
  const { setBinding: handleSetBinding } = useBindingsWiring(worker, status, task.syncOutcomeSeq);
  const { createRule: handleCreateRule, patchRule: handlePatchRule } = useRulesWiring(
    worker,
    status,
    task.syncOutcomeSeq,
  );
  // #245: every source the registered standing questions need, refreshed on
  // the same per-cycle signal as the bindings they depend on.
  usePaneReadsWiring(worker, status, task.syncOutcomeSeq);
  // #267: the calendar arm's exact twin — every interval the registered
  // standing questions need from the calendar mirror. #122's the first real
  // caller, which is what makes `getCalendarEvents` reachable at all rather
  // than an exported, unit-tested, never-wired hook.
  useCalendarEventsWiring(worker, status, task.syncOutcomeSeq);
  // The Ledger/Done reads, refreshed per cycle AND per mutation result — the
  // hook's own doc says why the mutation-result refresh lives there rather
  // than in worker-client.ts.
  useLedgerWiring(worker, status, task.syncOutcomeSeq, task.lastCapture, task.lastAct, task.lastTriage);

  // #110/S12's "always-present ... plus a global hotkey that focuses it"
  // (#98, restated on #110), now over a popover instead of a screen switch:
  // capture opens `CapturePopover` over whatever is showing, so asking for
  // the box no longer costs the person the screen they were reading.
  //
  // The counter beside the flag is not redundant. `CaptureBox` focuses its
  // field on every bump, so a second gesture while the popover is ALREADY
  // open re-focuses the field rather than being a no-op — the same reason the
  // Triage-screen version was a counter and not a boolean.
  const [captureOpen, setCaptureOpen] = useState(false);
  const [captureFocusRequestId, setCaptureFocusRequestId] = useState(0);
  const requestCapture = () => {
    setCaptureOpen(true);
    setCaptureFocusRequestId((id) => id + 1);
  };

  // Demo mode's unsorted list. Held here, not in `TriageScreen`, because the
  // capture box is in the shell now: a fixture capture typed in the popover
  // has to land in the list the Triage screen renders. Dev-only either way —
  // `demoData()` is null in production.
  const [demoCaptures, setDemoCaptures] = useState(() => demo?.triage ?? []);

  function handleCapture(title: string, destination: CaptureDestination, fields: CaptureFields) {
    if (demo) {
      // Fixtures, so `destination` is not honoured — and neither is `fields`:
      // the demo frontier is a hand-authored world, and a minted fixture
      // appearing on it would be a second, divergent source of truth for what
      // the demo shows.
      setDemoCaptures((current) => [
        { id: `CAP-${current.length + 8}`, title, source: "Typed here", age: "just now" },
        ...current,
      ]);
      return;
    }
    submitCapture(title, destination, Date.now(), fields);
  }

  function dropDemoCapture(id: string) {
    setDemoCaptures((current) => current.filter((capture) => capture.id !== id));
  }

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
        requestCapture();
      }
    }
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, []);
  const { act: handleAct } = useItemActions(worker);
  const { triage: handleTriage } = useTriageWiring(worker);
  // #122's do-date write: the same triage mutation entry point every other
  // triage edit uses, with `destination: null` (leave `stage` untouched —
  // `useTriageWiring.ts`'s own doc) and only `scheduledDate` set. Not its
  // own hook: it is one more call shape of the wiring already threaded
  // through this component, not a second mutation entry point.
  function handleSetScheduledDate(itemId: string, date: string | null): void {
    handleTriage(itemId, null, { scheduledDate: date });
  }
  const syncLabel = syncStatusLabel({
    online,
    lastSyncOutcome: task.lastSyncOutcome,
    lastSyncAtMs: task.lastSyncAtMs,
    queueDepth: task.queueDepth,
    nowMs: syncNowMs,
  });

  // Issue #194: the header refresh control's gate is the union of what is
  // actually refreshable — a task token, a healthy calendar connection, or
  // both — not the calendar alone. See `refresh-gate.ts`'s own doc for why
  // the calendar-only gate was a bug: it hid the button entirely on a
  // task-only device, the default and the state the owned-stack path
  // actually cares about.
  const refreshEnabled = canRefresh(status, hasTaskToken, calendar.connected, calendar.needsReconnect);

  // Fires whichever legs are actually usable. Keeps the calendar refresh
  // exactly as it was (`handleRefreshClick`) — this adds a second leg
  // (`handleManualSync`, routed through the shared cadence), it does not
  // replace one; a device with both refreshes both from one press.
  function handleRefresh() {
    if (calendar.connected && !calendar.needsReconnect) {
      handleRefreshClick();
    }
    if (hasTaskToken) {
      handleManualSync();
    }
  }

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
        collapsed={railCollapsed}
        onToggleCollapsed={handleToggleRailCollapsed}
      />

      <main style={{ display: "flex", flex: 1, minWidth: 0, flexDirection: "column" }}>
        <Header
          title={SCREEN_TITLES[screen]}
          // The demo badge stands in for a real cycle only in demo mode;
          // everywhere else this is now backed by one (S9) — see
          // `sync-status.ts`.
          syncLabel={demo?.syncBadge ?? (hasTaskToken ? syncLabel : undefined)}
          onRefresh={refreshEnabled ? handleRefresh : undefined}
          onCapture={requestCapture}
        />

        {/* A waiting service worker, said out loud. `<main>` is a column
            whose header is `flex: 0 0 auto` and whose scroll container is
            `flex: 1; minHeight: 0`, so a `0 0 auto` sibling here is always
            visible, never scrolls and never overlaps. */}
        {updateReady ? <UpdateBanner onReload={handleReload} /> : null}

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
              onScreen={setScreen}
              task={task}
              nowMs={syncNowMs}
              selectedItemId={selectedItemId}
              onOpenItem={handleOpenItem}
              onCloseItemDetail={handleCloseItemDetail}
              onAct={handleAct}
              calendarReads={calendar.eventReads}
              calendarConnected={calendar.connected}
              onSetScheduledDate={demo ? undefined : handleSetScheduledDate}
            />
          )}
          {screen === "triage" && (
            <TriageScreen
              demo={demo}
              task={task}
              demoCaptures={demo ? demoCaptures : undefined}
              onDropDemoCapture={demo ? dropDemoCapture : undefined}
              onTriage={demo ? undefined : handleTriage}
              onComplete={demo ? undefined : (itemId) => handleAct(itemId, "complete")}
              nowMs={syncNowMs}
            />
          )}
          {screen === "routes" && <RoutesScreen demo={demo} />}
          {screen === "alerts" && <AlertsScreen demo={demo} />}
          {screen === "rules" && (
            <RulesScreen
              rules={demo ? demo.ruleDetails : task.rules}
              kindRegistry={demo ? demo.ruleKindRegistry : task.kindRegistry}
              frontier={demo ? demo.ruleBacktestItems : task.frontier}
              lastRuleWrite={demo ? null : task.lastRuleWrite}
              syncOutcomeSeq={task.syncOutcomeSeq}
              onCreateRule={demo ? () => {} : handleCreateRule}
              onPatchRule={demo ? () => {} : handlePatchRule}
            />
          )}
          {screen === "done" && <DoneScreen task={task} nowMs={syncNowMs} />}
          {screen === "ledger" && (
            <LedgerScreen
              task={task}
              nowMs={syncNowMs}
              onComplete={(itemId) => handleAct(itemId, "complete")}
            />
          )}
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
              onSetBinding={handleSetBinding}
              online={online}
              syncNowMs={syncNowMs}
              onDownloadMirror={handleDownloadMirror}
            />
          )}
        </div>
      </main>

      {/* The shell's capture box (#107): over the current screen, never
          instead of it. Rendered outside `<main>` — it is `position: fixed`
          chrome for the whole window, not content in the scroll column. */}
      <CapturePopover
        open={captureOpen}
        focusRequestId={captureFocusRequestId}
        onClose={() => setCaptureOpen(false)}
        onSubmit={handleCapture}
        demo={demo !== null}
        lastCapture={demo ? null : task.lastCapture}
      />
    </div>
  );
}
