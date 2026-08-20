import { useEffect, useState } from "react";
import { demoCalendar, demoTaskState } from "./fixtures/demo";
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
import { liveWriteFailureCount } from "./screens/write-failure";
import { isCaptureHotkey } from "./shell/capture-hotkey";
import { isRecallHotkey } from "./shell/recall-hotkey";
import { escapeClaimant, type EscapeClaimant } from "./shell/escape-claimants";
import { CapturePopover } from "./shell/CapturePopover";
import { Header } from "./shell/Header";
import { RecallOverlay } from "./shell/RecallOverlay";
import { NavBar } from "./shell/NavBar";
import { NavRail } from "./shell/NavRail";
import { useIsPhone } from "./shell/useIsPhone";
import { readAsideCollapsed, writeAsideCollapsed } from "./screens/questions/aside-prefs";
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
import { useGrillDraftListWiring } from "./shell/useGrillDraftListWiring";
import { useGrillTakeoverWiring } from "./shell/useGrillTakeoverWiring";
import { useItemActions } from "./shell/useItemActions";
import { useTriageWiring } from "./shell/useTriageWiring";
import { useBackendSelection } from "./shell/useBackendSelection";
import { useBindingsWiring } from "./shell/useBindingsWiring";
import { useItemDetailWiring } from "./shell/useItemDetailWiring";
import { useMicrotaskWiring } from "./shell/useMicrotaskWiring";
import { useLedgerWiring } from "./shell/useLedgerWiring";
import { useRecallWiring } from "./shell/useRecallWiring";
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

  // The board world (#420). A lazy initializer, not a ref: reading
  // `ref.current` during render is what React's rules forbid, and this needs
  // to be constructed exactly once per mount either way. Read-only by
  // construction: it substitutes for the published state the sync engine would
  // have sent, and no mutation is rewired to it — the point is photographing
  // and eyeballing the real render path at production's density, not a second
  // writable world. A capture typed into the popover still goes to the
  // worker, which knows nothing of these fixture ids.
  //
  // `DemoData` and the kit world it seeds left this component in #457 —
  // Routes and Alerts, its last two consumers, now read it through their own
  // dev-gated accessor (`fixtures/demo-data.ts`'s `demoData()`) instead of a
  // `demo` prop threaded from here, and every guard that used to keep writes
  // inert while the kit world showed went with it: this component no longer
  // has any opinion about which world is loaded.
  const [demoTask] = useState(demoTaskState);
  const task = demoTask ?? liveTask;

  // The board world's Settings calendar card (#452, piece 4), same
  // lazy-initializer reason as `demoTask`. Injected ONLY into
  // `SettingsScreen`'s `calendar` prop below — `useCalendarWiring` two lines
  // down keeps reading `calendar` (the live store slice) unconditionally, and
  // so does everything else in this component that reads `calendar`. See
  // `fixtures/demo-calendar.ts`'s header for why a store-level override would
  // start a real poll timer against a worker with no token.
  const [settingsDemoCalendar] = useState(demoCalendar);

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
  // Now's standing-questions aside, held here rather than in `NowScreen` for
  // one reason: shut, its reopen control is a `?` in the header, and the header
  // is the shell's. The same device-local idiom as the rail above — read once
  // through the same storage guard, written on every toggle.
  const [asideCollapsed, setAsideCollapsed] = useState(() =>
    readAsideCollapsed(typeof localStorage === "undefined" ? undefined : localStorage),
  );
  const handleToggleAsideCollapsed = () => {
    const next = !asideCollapsed;
    setAsideCollapsed(next);
    writeAsideCollapsed(typeof localStorage === "undefined" ? undefined : localStorage, next);
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
  // Whether `CaptureBox` currently has a live dictation session, reported up
  // through `CapturePopover`, and the bumped counter that asks it to cancel
  // one in place (#380) — see the Escape branch below and
  // `CapturePopover.tsx`'s own doc on why this is plumbing, not state this
  // component has any opinion about.
  const [captureDictating, setCaptureDictating] = useState(false);
  const [cancelDictationRequestId, setCancelDictationRequestId] = useState(0);
  // The phone nav's More sheet. Held here rather than in `NavBar` because it
  // is one of three things an Escape can mean, and only a place that can see
  // all three can order them (`escape-claimants.ts`). It is read as
  // `isPhone && navSheetOpen` everywhere below: `NavBar` is only mounted on a
  // phone, so a stale `true` left over from a narrow window would otherwise
  // hand the desktop's Escape to a sheet that is not on screen.
  const [navSheetOpen, setNavSheetOpen] = useState(false);
  // Closing capture is its own function because two facts have to move
  // together: the popover's flag, and the shell's copy of whether the box
  // inside it is dictating. `CaptureBox` reports that upward from an effect
  // on its own `listening` state, and unmounting never runs that effect
  // again — so a popover closed while a session was live would leave
  // `captureDictating` stuck true, and the NEXT Escape over a reopened
  // popover would cancel a dictation that is not happening instead of
  // closing. (The session itself is genuinely ended: `CaptureBox`'s unmount
  // cleanup aborts it and releases the microphone. It is only the report
  // that cannot follow it.)
  const closeCapture = () => {
    setCaptureOpen(false);
    setCaptureDictating(false);
  };
  const requestCapture = () => {
    setCaptureOpen(true);
    setCaptureFocusRequestId((id) => id + 1);
    // #480 follow-up: the two overlays are siblings at one z-index and
    // neither traps focus, so both open was a reader tabbing between two
    // things that each claimed to be modal. Opening either now closes the
    // other — the exclusivity lives in these two openers, which every one of
    // the six triggers already funnels through, rather than in six call
    // sites or in an effect watching both flags.
    setSearchOpen(false);
  };

  // **Recall** (#478/#480): every trigger — the header's Search button, the
  // `/` hotkey, Escape, the rail's magnifier and the phone More sheet's entry
  // — opens this same state. `searchQuery` lives here rather than inside
  // `RecallOverlay` so `useRecallWiring` can key its request effect on the
  // same value the overlay renders, with no second, out-of-band copy of it.
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  useRecallWiring(worker, status, searchQuery, task.lastTriage);
  const requestSearchOpen = () => {
    setSearchOpen(true);
    // The other half of the exclusivity rule — see `requestCapture` above,
    // and note this is the one path that closes the popover while it may be
    // DICTATING, so the flag comes down with it (`closeCapture`'s doc). The
    // two setters are spelled out rather than calling `closeCapture()`
    // because the `/` hotkey calls this from the shell's keydown effect, and
    // a helper whose body is nothing but `useState` setters is the only kind
    // that effect can call without becoming a dependency of it.
    setCaptureOpen(false);
    setCaptureDictating(false);
  };

  function handleCapture(title: string, destination: CaptureDestination, fields: CaptureFields) {
    submitCapture(title, destination, Date.now(), fields);
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

      // #480: the `/` hotkey people already expect from search-first apps —
      // `recall-hotkey.ts`'s own pure matcher, guarded the identical way
      // `isCaptureHotkey` is (no modifier, no editable target, no IME
      // composition in progress).
      if (
        isRecallHotkey({
          key: event.key,
          ctrlKey: event.ctrlKey,
          metaKey: event.metaKey,
          altKey: event.altKey,
          targetIsEditable,
          isComposing: event.isComposing,
        })
      ) {
        event.preventDefault();
        requestSearchOpen();
        return;
      }

      // Escape closes exactly one open overlay — the shallowest — and this is
      // the only place in the app that decides which. Every claimant's flag is
      // shell state, so the ordering is a lookup rather than a negotiation
      // between listeners that cannot see each other; `escape-claimants.ts`
      // holds the order and the argument, and the closer map below is a
      // `Record` over it, so a new claimant cannot be silently forgotten here.
      //
      // Still bound to the document, not to any overlay's markup: an Escape
      // must close the popover when focus has tabbed out of its card, and the
      // detail panel once the reader has clicked into the board behind it.
      const claimant = escapeClaimant({
        key: event.key,
        isComposing: event.isComposing,
        open: {
          capture: captureOpen,
          search: searchOpen,
          navSheet: isPhone && navSheetOpen,
          itemDetail: selectedItemId !== null,
        },
      });
      if (claimant) {
        const close: Record<EscapeClaimant, () => void> = {
          // #380: while the box is dictating, this Escape is not a close —
          // it undoes the dictation and leaves the popover open. The next
          // Escape sees `captureDictating` false (`CaptureBox` reports it the
          // instant the session ends) and closes as it always did — still
          // one owner, still no second listener, just a branch on a fact
          // only `CaptureBox` has.
          capture: () =>
            captureDictating ? setCancelDictationRequestId((id) => id + 1) : closeCapture(),
          search: () => setSearchOpen(false),
          navSheet: () => setNavSheetOpen(false),
          itemDetail: handleCloseItemDetail,
        };
        close[claimant]();
      }
    }
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [
    captureOpen,
    captureDictating,
    searchOpen,
    isPhone,
    navSheetOpen,
    selectedItemId,
    handleCloseItemDetail,
  ]);
  const { act: handleAct } = useItemActions(worker);
  const { triage: handleTriage } = useTriageWiring(worker);
  // #355/ADR-0023's Grill takeover — the Triage screen's own composition of
  // the turn lane and the Confirm mutation (`useGrillTakeoverWiring.ts`'s
  // own doc).
  useGrillDraftListWiring(worker, status);
  const grillTakeover = useGrillTakeoverWiring(
    worker,
    task.stepsByItem,
    task.lastGrillCompletion,
    task.grillDraftItemIds,
    task.grillDraftByItem,
  );
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
  //
  // #455: derived from the store (`task`, which is `demoTask ?? liveTask`),
  // not from `DemoData` — the kit fixture used to be the only source, which
  // meant a real device never showed either badge at all, however full its
  // triage inbox actually was. `triageInbox.length` is the same real count
  // `TriageScreen`'s own "N captured" reads; `liveWriteFailureCount` is the
  // one thing the store has that answers to "alerts" at all, since
  // `AlertsScreen` itself stays demo-fixture-only (ADR-0016) — it is 0 on an
  // ordinary real device and 2 under the board fixture, which seeds both of
  // Now's stranded-write alerts on purpose (`demo-task-state.ts`).
  const navCounts = {
    triage: task.triageInbox.length,
    alerts: liveWriteFailureCount(task.lastTriage, task.lastAct),
  };

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
          onSearch={requestSearchOpen}
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
          syncLabel={hasTaskToken ? syncLabel : undefined}
          onRefresh={refreshEnabled ? handleRefresh : undefined}
          onSearch={requestSearchOpen}
          // Only on Now — the aside exists on no other screen. Same rule as
          // `onSearch`/`onRefresh` above: the affordance appears exactly where
          // it would do something.
          onToggleQuestions={screen === "now" ? handleToggleAsideCollapsed : undefined}
          questionsCollapsed={asideCollapsed}
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
              onScreen={setScreen}
              task={task}
              nowMs={syncNowMs}
              selectedItemId={selectedItemId}
              onOpenItem={handleOpenItem}
              onCloseItemDetail={handleCloseItemDetail}
              onAct={handleAct}
              calendarReads={calendar.eventReads}
              calendarConnected={calendar.connected}
              onSetScheduledDate={handleSetScheduledDate}
              microtask={microtaskWiring}
              // The same two callbacks the Triage screen gets below: Now is a
              // second view of one inbox, never a second entry point into it.
              onTriage={handleTriage}
              asideCollapsed={asideCollapsed}
              // #359: the SAME `grillTakeover` instance the Triage screen gets
              // below — one interview session for the whole app, not a second
              // one per screen.
              grill={grillTakeover}
            />
          )}
          {screen === "triage" && (
            <TriageScreen
              task={task}
              onTriage={handleTriage}
              onComplete={(itemId) => handleAct(itemId, "complete")}
              nowMs={syncNowMs}
              grill={grillTakeover}
            />
          )}
          {screen === "routes" && <RoutesScreen />}
          {screen === "alerts" && <AlertsScreen />}
          {screen === "rules" && (
            <RulesScreen
              rules={task.rules}
              kindRegistry={task.kindRegistry}
              frontier={task.frontier}
              lastRuleWrite={task.lastRuleWrite}
              syncOutcomeSeq={task.syncOutcomeSeq}
              onCreateRule={handleCreateRule}
              onPatchRule={handlePatchRule}
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
              onScreen={setScreen}
              task={task}
              nowMs={syncNowMs}
              calendarReads={calendar.eventReads}
              calendarConnected={calendar.connected}
            />
          )}
          {screen === "settings" && (
            <SettingsScreen
              status={status}
              apiVersion={apiVersion}
              coreId={coreId}
              viewOrdinal={viewOrdinal}
              error={error}
              calendar={settingsDemoCalendar ?? calendar}
              calendarIsDemo={settingsDemoCalendar !== null}
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
          sheetOpen={navSheetOpen}
          onSheetOpen={setNavSheetOpen}
          onSearch={requestSearchOpen}
        />
      ) : null}

      {/* The shell's capture box (#107): over the current screen, never
          instead of it. Rendered outside `<main>` — it is `position: fixed`
          chrome for the whole window, not content in the scroll column. */}
      <CapturePopover
        open={captureOpen}
        focusRequestId={captureFocusRequestId}
        onClose={closeCapture}
        onSubmit={handleCapture}
        projects={task.projects}
        // #457: this component no longer has a kit world to be inert for —
        // `demo`'s own fixture-queue arm lives on in `CaptureBox`'s own
        // `demo` prop for a future caller, but nothing left in this
        // component ever passes `true`.
        demo={false}
        lastCapture={task.lastCapture}
        cancelDictationRequestId={cancelDictationRequestId}
        onDictatingChange={setCaptureDictating}
      />

      {/* **Recall** (#478/#480): every trigger — the header's Search button,
          the `/` hotkey, the rail's magnifier and the phone More sheet's
          entry — opens this same state; see `useRecallWiring`'s doc above.
          Rendered after `CapturePopover` as a sibling, both fixed chrome at
          the same z-index. That DOM order used to decide which one a reader
          sees on top; since `requestCapture`/`requestSearchOpen` above close
          each other, the two are never open together and it decides nothing
          — the order is kept because it is still the one that would paint
          correctly, and `escape-claimants.ts` explains what now rests on
          it. */}
      <RecallOverlay
        open={searchOpen}
        query={searchQuery}
        onQueryChange={setSearchQuery}
        onClose={() => setSearchOpen(false)}
        rows={task.search?.rows ?? null}
        total={task.search?.total ?? 0}
        projects={task.projects}
        onTriage={handleTriage}
        lastTriage={task.lastTriage}
        nowMs={syncNowMs}
      />
    </div>
  );
}
