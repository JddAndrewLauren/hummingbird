import { useEffect, useState } from "react";
import { demoData, demoTaskState } from "./fixtures/demo";
import { AlertsScreen } from "./screens/AlertsScreen";
import { DoneScreen } from "./screens/DoneScreen";
import { LedgerScreen } from "./screens/LedgerScreen";
import { NowScreen } from "./screens/NowScreen";
import { RoutesScreen } from "./screens/RoutesScreen";
import { RulesScreen } from "./screens/RulesScreen";
import { SettingsScreen } from "./screens/SettingsScreen";
import { StatusScreen } from "./screens/StatusScreen";
import { TriageScreen } from "./screens/TriageScreen";
import type { CaptureDestination } from "./screens/capture-destination";
import { closesItemDetail, isCaptureHotkey } from "./shell/capture-hotkey";
import { CapturePopover } from "./shell/CapturePopover";
import { Header } from "./shell/Header";
import { NavBar } from "./shell/NavBar";
import { NavRail } from "./shell/NavRail";
import { useIsPhone } from "./shell/useIsPhone";
import { readRailCollapsed, writeRailCollapsed } from "./shell/rail-collapse";
import { canRefresh } from "./shell/refresh-gate";
import { SCREEN_TITLES, type Screen } from "./shell/screens";
import { coreStatusLabel } from "./shell/status-label";
import { syncStatusLabel } from "./shell/sync-status";
import { useCalendarEventsWiring } from "./shell/useCalendarEventsWiring";
import { tripsCalendarId } from "./calendar/selection";
import { useCalendarWiring } from "./shell/useCalendarWiring";
import { useCaptureWiring } from "./shell/useCaptureWiring";
import { useFrontierWiring } from "./shell/useFrontierWiring";
import { useGrillTakeoverWiring } from "./shell/useGrillTakeoverWiring";
import { useItemActions } from "./shell/useItemActions";
import { useTriageWiring } from "./shell/useTriageWiring";
import { useBackendSelection } from "./shell/useBackendSelection";
import { useBindingsWiring } from "./shell/useBindingsWiring";
import { useItemDetailWiring } from "./shell/useItemDetailWiring";
import { useMicrotaskWiring } from "./shell/useMicrotaskWiring";
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
  // #172's ADR-0010 diagnostic, read straight off the handshake and rendered
  // in Settings' "Local core" card.
  const coreId = useStore((state) => state.coreId);
  const viewOrdinal = useStore((state) => state.viewOrdinal);
  const error = useStore((state) => state.error);
  const calendar = useStore((state) => state.calendar);
  const liveTask = useStore((state) => state.task);

  // A lazy initializer rather than a ref: reading `ref.current` during render
  // is what React's rules forbid, and this needs to be constructed exactly
  // once per mount either way.
  const [worker] = useState<WorkerLike>(() => injectedWorker ?? realWorker());

  // Lazy initializer, not a ref: `demoData()` returns null in production, and
  // `ref.current ??= …` would re-run it on every render forever.
  const [demo] = useState(demoData);

  // The board world (#420), same lazy-initializer reason. Read-only by
  // construction: it substitutes for the published state the sync engine would
  // have sent, and no mutation is rewired to it — the point is photographing
  // and eyeballing the real render path at production's density, not a second
  // writable world. A capture typed into the popover still goes to the worker,
  // which knows nothing of these fixture ids; `demo`'s own fixture-queue arm
  // does not apply here because `demoData()` is null in this mode.
  const [demoTask] = useState(demoTaskState);
  const task = demoTask ?? liveTask;

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
  // The nav rail and the bottom bar are different DOM trees, not one tree at
  // two sizes, so this is the media query as state rather than a CSS rule —
  // `responsive.css`'s header argues where that line falls.
  const isPhone = useIsPhone();
  // #274's picker choice: device-local, never synced (`useBackendSelection.ts`).
  const { selection: backendSelection, setSelection: setBackendSelection } = useBackendSelection();
  const {
    handleConnectClick,
    handleCalendarSelectionChange,
    handleRefreshClick,
  } = useCalendarWiring(worker, status, calendar, tripsCalendarId(task.bindings));
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
  // #273's microtask lane. Deliberately main-thread and outside the sync
  // engine entirely — see `useMicrotaskWiring.ts`'s header for why a
  // worker-hosted run would be #269's banned queue in all but name.
  // #274 threads the picker's own choice through, and gives the hook a way
  // to move that same device-local selection when a pinned decline's
  // fallback button is tapped.
  const microtaskWiring = useMicrotaskWiring(worker, selectedItemId, backendSelection, {
    onSelectBackend: setBackendSelection,
  });
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
        return;
      }

      // Escape closes the open item detail — the same shell-level listener,
      // for the same reason: the panel is not focused once the reader has
      // clicked into the board behind it, and a handler on its own markup
      // only sees what bubbles out of it.
      //
      // It is the LAST claimant, deliberately: `CapturePopover` sits over the
      // detail panel and owns Escape while it is open, so the shallowest open
      // thing must be what closes — otherwise one Escape shuts the popover
      // AND the panel underneath it, and the reader loses something they
      // never asked to close. The guard reads `captureOpen` rather than the
      // event, because one document listener cannot see another's intent.
      //
      // `NavBar`'s phone sheet owns Escape the same way and is **not** guarded
      // here, because its open state lives inside that component and reaching
      // it would mean lifting sheet state into the shell for one keystroke.
      // The cost is bounded and stated rather than hidden: on a phone, with a
      // hardware keyboard, with both the sheet and an item open, one Escape
      // closes both. Lift the state if that stops being hypothetical.
      //
      // The Grill takeover is not in the chain: it owns no Escape at all
      // (`GrillTakeover.tsx` — leaving an interview is a deliberate Back),
      // and it replaces the centre column rather than covering the panel.
      if (
        closesItemDetail({
          key: event.key,
          isComposing: event.isComposing,
          captureOpen,
          itemDetailOpen: selectedItemId !== null,
        })
      ) {
        handleCloseItemDetail();
      }
    }
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [captureOpen, selectedItemId, handleCloseItemDetail]);
  const { act: handleAct } = useItemActions(worker);
  const { triage: handleTriage } = useTriageWiring(worker);
  // #355/ADR-0023's Grill takeover — the Triage screen's own composition of
  // the turn lane and the Confirm mutation (`useGrillTakeoverWiring.ts`'s
  // own doc). Absent in demo mode, same reason `onTriage` is: demo has no
  // real `TaskItemDTO` to grill.
  const grillTakeover = useGrillTakeoverWiring(worker, task.stepsByItem, task.lastGrillCompletion);
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

  // The rail's mark is the way home: Now, refreshed. A page reload would be
  // the other reading of "refresh", but it would tear down the core and the
  // SharedWorker for data the same press already re-fetches (ADR-0010).
  // One counts object for whichever nav is mounted — the two forms show the
  // same numbers, so a second literal would be a second answer waiting to
  // diverge.
  const navCounts = demo ? { triage: demo.triage.length, alerts: demo.alerts.length } : {};

  function handleHome() {
    setScreen("now");
    if (refreshEnabled) {
      handleRefresh();
    }
  }

  return (
    // `className`, not a style object: this is one of the pure-layout elements
    // whose phone form is a media query, and at equal importance a stylesheet
    // rule loses to an element's own `style` attribute. `!important` on every
    // phone declaration would beat it, which is exactly the cost that deleting
    // this element's inline object avoids. `shell/responsive.css` has the whole
    // argument and every rule.
    <div className="hb-shell">
      {/* Exactly one navigation landmark is mounted, never both — two would
          break `surfaces.spec.ts`'s strict-mode `getByRole("navigation")` on
          every visual project. The rail renders before `<main>` and the bar
          after it, so each is in the DOM where it is on screen and neither
          needs a CSS `order` to correct it. */}
      {isPhone ? null : (
        <NavRail
          screen={screen}
          onScreen={setScreen}
          counts={navCounts}
          statusLabel={coreStatusLabel(status, apiVersion)}
          theme={theme}
          onToggleTheme={() => setPreference(toggledPreference(theme))}
          collapsed={railCollapsed}
          onToggleCollapsed={handleToggleRailCollapsed}
          onHome={handleHome}
        />
      )}

      {/* `minHeight: 0` alongside `minWidth: 0`, and both are load-bearing: a
          flex item's default `min-height`/`min-width` is `auto`, which refuses
          to shrink below its content. On the desktop row the width one is what
          mattered. On the phone the shell is a COLUMN, so without the height
          one this box grows to its content, the scroll container inside it
          never scrolls, and the nav bar below it is pushed off the screen
          entirely. */}
      <main
        style={{ display: "flex", flex: 1, minWidth: 0, minHeight: 0, flexDirection: "column" }}
      >
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
            the context panel, and lets only the centre column move. Its
            styling — and its phone form — is `shell/responsive.css`. */}
        <div className="hb-scroll">
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
              microtask={demo ? undefined : microtaskWiring}
              // The same two callbacks the Triage screen gets below: Now is a
              // second view of one inbox, never a second entry point into it.
              onTriage={demo ? undefined : handleTriage}
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
              grill={demo ? undefined : grillTakeover}
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
          {screen === "status" && (
            <StatusScreen
              demo={demo}
              onScreen={setScreen}
              task={task}
              nowMs={syncNowMs}
              calendarReads={calendar.eventReads}
              calendarConnected={calendar.connected}
            />
          )}
          {screen === "settings" && (
            <SettingsScreen
              demo={demo}
              status={status}
              apiVersion={apiVersion}
              coreId={coreId}
              viewOrdinal={viewOrdinal}
              error={error}
              calendar={calendar}
              themePreference={preference}
              onThemePreference={setPreference}
              backendSelection={backendSelection}
              onBackendSelection={setBackendSelection}
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

      {isPhone ? (
        <NavBar
          screen={screen}
          onScreen={setScreen}
          counts={navCounts}
          statusLabel={coreStatusLabel(status, apiVersion)}
          theme={theme}
          onToggleTheme={() => setPreference(toggledPreference(theme))}
        />
      ) : null}

      {/* The shell's capture box (#107): over the current screen, never
          instead of it. Rendered outside `<main>` — it is `position: fixed`
          chrome for the whole window, not content in the scroll column. */}
      <CapturePopover
        open={captureOpen}
        focusRequestId={captureFocusRequestId}
        onClose={() => setCaptureOpen(false)}
        onSubmit={handleCapture}
        projects={demo ? [] : task.projects}
        demo={demo !== null}
        lastCapture={demo ? null : task.lastCapture}
      />
    </div>
  );
}
