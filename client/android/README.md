# The native Android client

The ADR-0003 native client (#141), started at M0 (the walking skeleton) per
the 2026-08-14 grilling: full web parity is the destination, reached one
screen at a time, each screen's decision modules sinking into
`hummingbird-core` first (ADR-0025) — no Kotlin copy of a decision function
is ever created. Rendering is native: Jetpack Compose + Material 3, themed
from the design system's tokens (`app/src/main/kotlin/.../ui/theme/`).

## The build's two seams

Everything under `app/src/main/kotlin` is an ordinary Compose app. The Rust
side arrives through two Gradle tasks in `app/build.gradle.kts`, both
running cargo against the `client/` workspace one level up:

- **`cargoNdkBuild`** — `cargo ndk` cross-compiles `hummingbird-ffi-mobile`
  into `app/src/main/jniLibs/` (gitignored; arm64-v8a for the device,
  x86_64 for the emulator).
- **`generateUniffiBindings`** — builds the host cdylib and runs UniFFI in
  library mode: the Kotlin binding under `app/build/generated/uniffi/` is
  derived from the exported surface in `client/ffi-mobile/src/lib.rs`,
  which is the single source of truth (no `.udl`).

Prerequisites beyond Android Studio: `rustup target add
aarch64-linux-android x86_64-linux-android`, `cargo install cargo-ndk`, and
an NDK (Studio's SDK manager, or `sdkmanager "ndk;<version>"`).

## Running it

- Debug on the device/emulator: `./gradlew installDebug`.
- Release: sideload-only, signed with the operator-local keystore
  (`keystore.properties`, never committed, never in Actions — see
  `app/build.gradle.kts`). There is no store channel and no CI-signed
  release, deliberately.
- On device, the app asks once for a `device` token (minted by the
  operator against the authority); it rests in the Android Keystore.

## Sync model (grilling 2026-08-14)

Foreground: one `user` cycle on resume plus the 60-second `timer` cadence
while open, owned by `MainActivity`'s always-composed `AppRoot` — declared
above the `NavHost`, not inside a destination, so it runs the same whichever
route is showing. Background: an OS-deferred hourly WorkManager refresh
(`sync/SyncWorker.kt`) — the middle leg only; correctness never depends on
it. Sync-on-push is the third leg (M2): every arriving FCM message enqueues
a one-shot `SyncWorker` with the `"push"` trigger, which the seam maps to
`Trigger::User` and so past the core's backoff gate. Still one clock per
cadence (issue #8): the push leg is event-driven and schedules nothing.

## The notification lane (M2, #141)

The payload is data-only, so the client builds every notification itself
(`notify/`) and the Ack action is a broadcast to `push/AckReceiver`, never a
swipe — dismissing a notification acks nothing (ADR-0012). `push/` also owns
registration: a stable install id in plain SharedPreferences is a device
*slot*, replayed on every rotation, and `RegistrationWorker` retries a
transport failure but not a missing device token.

`notify/NotificationChannels.SPECS` keys must byte-match the `channel_id`
values `server/authority/src/fcm.rs` emits; nothing links the two literals at
compile time, so `NotificationChannelSpecTest` is what does. A key is a
*tier*, not a channel id: the urgent tier resolves to a second id once the
user grants Do-Not-Disturb access, because Android fixes `bypassDnd` at
channel creation and a channel first created without the grant can never
gain it. `ACCESS_NOTIFICATION_POLICY` in the manifest is what puts the app in
that Settings list at all, and `ensure` re-runs on every resume — returning
from Settings is the only signal the grant changed.

**`google-services.json` is committed at `app/google-services.json`** (Firebase
project `hummingbird-e2b01`) and the `com.google.gms.google-services` plugin
is applied. The json is in the repo on purpose: every value in it ships inside
every APK, so it discloses nothing, and the plugin fails the build without it —
gitignoring it would break CI's `assembleDebug` for no secrecy. `app/build.gradle.kts`
carries the full reasoning. The credential that can actually *send* is
`FCM_SERVICE_ACCOUNT`, a Cloudflare Worker secret that never enters this repo
or Actions (ADR-0011); it must be a service-account key **from the same Firebase
project**, since `server/worker/src/fcm.rs` builds the send URL from the
`project_id` inside it. A mismatch is invisible from here — the device
registers, the server accepts the alert, and nothing arrives.

**When a push does not arrive, read the Worker log first.** Nothing on this
side can tell you why: `POST /api/alerts` answers 201 with no delivery
information (`deliveries` rides the internal `ApiResponse`, not the body),
and #219's no-retry policy logs a failed send once and drops it. One line of
`wrangler tail` distinguishes rule-did-not-match, no-live-push-target and
credential-broken, and no client-side evidence can:

```
npx wrangler@latest tail hummingbird-authority --format json
```

(the worker name is positional; `--name` is a `secret` flag and errors here).
`FCM_SERVICE_ACCOUNT is set but unusable (Malformed)` means the secret's
*contents* are wrong even though `wrangler secret list` shows it — either
`google-services.json` was uploaded by mistake, or a pretty-printed key was
pasted into `wrangler secret put`, whose prompt reads one line and truncates
at the first newline. Pipe it minified.

CI is `.github/workflows/android.yml` (Gradle side) plus `client.yml`
(the Rust side, whose `client/**` filter covers this directory).

## The bottom nav and the More sheet (M3/#532, completed M4/#541)

`MainActivity.kt`'s `NavDestination` is the one route list the app's
navigation surface generates from — the same "one list, two derived halves"
rule `client/web/src/shell/nav-bar.ts`'s `ON_THE_BAR` follows, ported rather
than reinvented. `NavDestination.ON_BAR`/`.OVERFLOW` filter that one enum, so
a screen added to it lands on the bottom bar or in the "More" sheet by
construction, never neither. Four screens are on the bar — Now, Triage,
Alerts, Status, pinned against the web's own `ON_THE_BAR` set by
`BottomNavStructuralTest` — and Done, the Ledger, Rules, Settings and Routes
are in the sheet: #532 landed the first two, #541 the last three, completing
all nine screens' reachability and shrinking `BottomNavStructuralTest`'s own
exception list to empty. A bar or sheet tap goes through `goToTab`'s
`popUpTo`/`saveState`/`launchSingleTop`/`restoreState`, the standard
bottom-nav idiom: each tab keeps its own back stack across switches rather
than stacking a fresh copy of Now underneath every visit.

`routes` (#541) renders its **live empty state only** — "No routes yet" —
because `client/ffi-mobile/src/lib.rs` exposes no Route query at all yet;
the web's own populated `RoutesScreen.tsx` branch reads a demo fixture with
no live counterpart, and parity with a fixture is not parity. The sheet also
carries a Recall entry (#541) below the screen list, deliberately outside
`NavDestination` — a gesture, not a screen, the same distinction the web's
`onSearch` row holds by sitting outside `NAV_BAR_OVERFLOW`. #541 shipped its
placeholder; #542 replaced the body with the real search-as-you-type
surface (below). `NavigationStructuralTest` asserts full route
reachability and that the notification doors below survived the churn.

### The Done and Ledger screens (M3, #532)

`DoneScreen.kt`/`DoneViewModel.kt` and `LedgerScreen.kt`/`LedgerViewModel.kt`
are M3's one real sink. `MobileTaskHost::doneItems()`/`.ledgerRows(nowMs)`
hand Kotlin a pre-ordered, pre-decided record set —
`hummingbird_core::decisions::roster::{order_done, order_ledger,
ledger_row_state, last_touched_ms}` (ADR-0025) run once Rust-side, never
per row here — carrying a `MobileLedgerRowState::{Live, Archived{sinceMs}}`
enum and a `canMarkDone` gate that mirrors the web's `item-actions.ts`
widened one-click rule. Neither screen re-derives an order or a state; the
Ledger's one-click mark-done goes through the same `act("complete")` path
Triage's row checkmark uses, then reloads so the row's own state (and its
departure from the live set the checkmark gates on) reflects the mutation
immediately. `done-order.ts`/`ledger-order.ts` on the web are now seam
re-exports of the same sink, with their existing test suites untouched.

## The rules screen (M4, #540)

`RulesScreen.kt`/`RulesViewModel.kt` list the rules, toggle one
enabled/disabled (one CAS field), create and edit one, and show a draft's
backtest count. **The route has a permanent More-sheet entry since #541**;
`RulesScreenStructuralTest` asserts that reachability goes through the
shared `NavDestination`/`onNavigate` door, never a one-off
`navigate(Routes.RULES)` call.

Every rule verdict arrives applied from
`hummingbird_core::decisions::rules` (ADR-0025's M4 sink): validity,
operator legality, the value-widget cascade, the sub-alarm-interval
duration warning, the backtest count. There is no operator table, duration
grammar or `23:59` in either file, and `RulesScreenStructuralTest` reads
the sources to keep it so — the same no-emulator discipline
`AlertsScreenStructuralTest` uses. `core/WallClock.kt` is the one place
this app reads a timezone: the core takes "now" as an already-resolved
civil string, and the backtest needs two readings of one instant
(`deadline` is device-local, `occurred_at` is UTC).

## The Triage screen (M3, #531)

`TriageScreen.kt`/`TriageViewModel.kt` render the "triage process" queue —
captured and Grilling items together, in the core's order — headed by the
"N captured · M grilling" counts read straight off the record
(`TriageBoardRecord.capturedCount`/`grillingCount`; `TriageScreenStructuralTest`
gates that neither figure is ever recomputed from `items.size`/`items.count`).
One row opens at a time into the seeded editor built from #529's shared
`ui/forms/` components (`LevelSlider`/`ContextField`/`CaptureDateField`);
Promote-to-Ready is the only save destination this screen offers — there is
no "save without promoting" method on `TriageViewModel` at all, unlike item
detail's own edit mode. The row checkmark goes through the existing
`act("complete")` path, never a triage. The Grill button is live (#539):
gated on the row's own `canGrill` fact from the seam, it navigates to the
standalone `GrillTakeoverScreen`/`GrillTakeoverViewModel` rather than opening
an interview inline, so neither `TriageScreen.kt` nor `TriageViewModel.kt`
holds any turn, session or draft state of its own. **The route is reachable
from the bottom nav bar** (#532, above) — `NavDestination.TRIAGE` with
`onBar = true`, not an ad-hoc `navigate(Routes.TRIAGE)` call, which the
"registered first, wired later" shape `Routes.RULES` still holds now
resolves to: registered here at #531, wired at #532.
`TriageScreenStructuralTest` asserts that reachability, gates the header-count and
Grill rules above, and — the same foreground-resume discipline `AlertsScreen`,
`NowScreen` and `ItemDetailScreen` all carry — asserts a `LifecycleResumeEffect`
re-reads the queue on every return to the screen, not only on the app-wide
`syncTick`: a capture minted from `CaptureActivity` while Triage was
backgrounded must not wait for the next tick to appear.

The seam doors are `MobileTaskHost::triage_board()` (decided from the
already-sunk `hummingbird_core::decisions::queue::triage_process_queue`) and
`::triage_item()` (`Core::triage` with a real `promote_to_ready`, sharing
`to_triage_patch`'s `ItemEdit`→`TriagePatch` conversion with `edit_item`).

## The Grill takeover and the microtask affordance (M4, #539)

`GrillTakeoverScreen.kt`/`GrillTakeoverViewModel.kt` are the one-question-
at-a-time interview (ADR-0023), mounted from both the item screen's own
Grill button and the Triage row's (above) — never inline in either caller.
The review card's predicates (`wouldStrandPlan`/`demotesFromFrontier`/
`planReplacementLabel`) and the microtask affordance
(`ItemDetailRecord.microtaskAffordance`) both arrive applied from
`hummingbird_core::decisions::skills::{review,affordance}` (ADR-0025); the
Kotlin side decides neither. A draft auto-saves after every completed round,
not only on Back, so a fold/rotation mid-interview loses nothing —
`GrillTakeoverViewModel.open()` is idempotent per item id for the same
reason, and `ScreenStateRetentionTest` gates that the screen is retrieved
from the `ViewModel` store rather than `remember`. The microtask affordance's
own transport is `skills/MicrotaskRunner.kt`, the `skill_run_*` doors' first
real caller; `skills/BackendPreference.kt` is #274's picker, read into every
run and into the one-tap "switch tiers" offer a declined, unreachable pin
gets (`hummingbird_core::decisions::skills::backend::declined_backend_fallback`).

## The Recall screen (M4, #542)

`RecallScreen.kt`/`RecallViewModel.kt` are the milestone's closer: re-find
one known item across everything the mirror has ever known, live or
archived. `MobileTaskHost::search(query, nowMs)` hands back a
`MobileRecallOutcome` — rows already matched, grouped
(`MobileRecallGroup::{Live, Done, Archived}`) and ordered, plus the core's
own un-capped `total` — over `hummingbird_core::search` (#478, predating
this slice; no web change was needed here since the web seam already sank
it). Neither file re-derives any of that: no sort, filter, group-by or
title/description scan of its own, gated by `RecallScreenStructuralTest`
the same way `RulesScreenStructuralTest` gates its own surface. Search is
as-you-type with no debounce — a mirror read, not a network request, the
same reasoning `useRecallWiring.ts` states on the web. Tapping a **live**
row opens `ItemDetailScreen` via `Routes.itemDetail`; Done and archived rows
are shown, labelled and dimmed, but not tappable — this slice ships no
inline edit the way the web's #479 does. The route was registered at #541
as a gesture entry off the More sheet, deliberately outside `NavDestination`
(above); #542 replaced its placeholder body with this real surface.

## Choice rows, and the app's one layout gate (#576)

`ui/ChoiceRow.kt` is the container every row of buttons or chips the user
picks from goes in. It is a `FlowRow`, so a choice too wide for the display
moves to the next line — which matters because a plain `Row` does not clip
its overflow: it hands the trailing child whatever width is left, and the
label then wraps one character per line into a column of letters that still
takes up layout height. That shipped to four screens (item detail's action
row, the takeover's answer chips and its discard prompt, the Triage row's
two buttons) before it was sighted on hardware; on the discard prompt the
squeezed child was `Keep`, the escape from a destructive question, so the
failure was not always cosmetic. `NowScreen.kt`, `RulesScreen.kt` and
`ui/forms/PriorityRow.kt` had each already answered it with their own inline
`FlowRow`, and are left as they are.

`ChoiceRowWrappingTest` is **the only test in this module that measures a
layout**, and the reason the defect got this far is that no other one does —
`*StructuralTest.kt` assert presence and wiring, and the squeezed buttons
were all present and all wired. It runs `createComposeRule()` under
Robolectric at a 320dp qualifier and asserts each choice stays at least 48dp
wide and one line tall. Two things in it are load-bearing:
`@GraphicsMode(NATIVE)`, without which Robolectric measures text with a stub
that returns near-identical widths for every string and the defect does not
reproduce at all; and a control case that renders the old plain `Row` and
asserts the trailing button *is* squeezed (0dp × 136dp), so a widened
qualifier or a text-measurement regression cannot leave the file green while
measuring nothing.

## Proving the lane on hardware

CI cannot cover any of this: there is no emulator in `android.yml` and no FCM
delivery without a real device, so the checks below are the only evidence the
lane works end to end. Checks 1–12 were run in full on 2026-08-17 (Pixel 10
Pro Fold, SDK 37, #517) and every one passed. **13–18 were run on
2026-08-17 against merged `2ea76b5` on the same device and every one
passed** — the lane is proven end to end, sweep to pixel. Re-run all of
them after any change to `notify/`, `push/`, or the tap intent. **Check 19
(#538's skills-runner probe) was run on 2026-08-18 against `5f23ec8` on the
same device and all three cases passed** (evidence in #560); it shares
nothing with 1–18 but the device token.

**Check 19 costs you the device token.** `connectedDebugAndroidTest`
uninstalls both APKs when it finishes, and the token rests in
`EncryptedSharedPreferences` (`core/TokenStore.kt`), which goes with the
app data. Every run therefore ends with the phone un-credentialed and the
*next* run failing three cases with `no device token on this device` —
which is the check's own named error doing its job, not a regression.
Re-install (`./gradlew installDebug`) and paste the token from
`hummingbird-device-pixel-fold` again before the next run, from Status's
"Manage device token in Settings" link (#535 moved the field itself off
Status). Checks 1–18 are unaffected only because nothing in them runs an
instrumented suite.

You need the device on USB, a `device`-scope token for **this** device (there
is one per device — `hummingbird-device-pixel-fold` in 1Password; do not
paste another device's), and an `ingest`-scope token to raise test alerts
with. `POST /api/alerts` requires `Scope::Ingest` and a device token gets a
403 (`handlers/auth.rs`), so minting one is unavoidable — bind it to an
enrolled `Writes::Alerts` source with no live token, and revoke it afterwards.
Rules matter too: an ingested alert raises kind **`alert_raised`**, not
`email` — the `email` kind fires inside the poller (ADR-0011) — so the test
needs rules on `alert_raised` keyed on `source` and `severity`.

1. `./gradlew installDebug`, launch, grant `POST_NOTIFICATIONS`.
2. From Status, follow "Manage device token in Settings" and paste the
   token there (#535 — Status itself carries no token field); back out to
   Status and confirm it reads `Synced`.
3. Confirm `fcm_token` exists: `adb shell run-as net.twinion.hummingbird cat
   shared_prefs/hummingbird-push.xml`. Its absence means Firebase never
   initialised.
4. Grant Do Not Disturb access, **then return to the app** — the resume is
   what creates the bypassing generation (see `NotificationChannels`).
   Verify with `adb shell dumpsys notification --noredact`: `urgent` should
   now be `mDeleted=true` and `urgent.dnd` live with `mBypassDnd=true`.
5. Ring urgent. Confirm `channel=urgent.dnd`, `importance=4`, heads-up.
6. Ring normal. Confirm `channel=normal`, `importance=3`, no bypass.
7. Confirm `actions=1` on both — that is the Ack action, and it is what a
   full-hybrid payload would have cost (ADR-0012's amendment).
8. Ack from the notification shade; confirm `dismissed_at` on the authority.
9. Ack from the alert detail screen; confirm `dismissed_at` again. This one
   goes through the core's mutation queue, not `AckWorker`, and the
   difference is large enough to misread as a failure: measured 2026-08-17,
   a shade ack landed in **5 seconds** and a screen ack in **40–45**. Poll
   the authority for a full minute before calling it failed.
10. Tap a notification with the app running: lands on that alert's detail.
11. Tap one cold. Kill with `adb shell am kill net.twinion.hummingbird`, not
    `force-stop` — a force-stopped app receives no FCM at all.
12. Swipe a notification away and confirm **nothing** is acked (ADR-0012).

The item door (ADR-0027). These need an **`item-threshold/v1`** alert, which
is not raised by `POST /api/alerts` at all: it is minted by the Durable
Object's alarm sweep over items with near deadlines
(`authority/src/sweep.rs`). A deadline alone rings **nothing**: `sweep::tick`
evaluates *enabled rules* against an `item_threshold` event and returns early
when there are none, so the sweep has no built-in threshold. Create a rule
first, and scope it by title so it cannot fire against real work:

```
POST /api/rules  {"id":"…","event_kind":"item_threshold","severity":"urgent",
  "tier":"urgent","enabled":true,"conditions":[
    {"field":"title","op":"contains","value":"<marker>","negate":false},
    {"field":"deadline","op":"within_next","value":"1d","negate":false}]}
```

Then give a throwaway item that title and **today's** date as its deadline —
`within_next` is unbounded on the past side (ADR-0013), so today never lapses
out of the window mid-run, and the match must survive every tick from 13
through 16 or `resolution_pass` closes the alert. The alarm ticks every
`ALARM_INTERVAL_MS` (15 min), which is the floor on every ring below.

Each check needs its **own** ring, and a live alert will not ring twice:
`deliver`'s dedupe key is `(alert_id, rule_id, raised_at, severity)`, so a
fresh notification arrives only after the alert has settled and re-raised.
That forces the order **15 → 14 → 16**: ack first, then wait a tick for the
re-raise, then tap cold. Budget an hour.

Item detail has **two doors**: the Now card itself (#524) and a notification
tap (#521). Tapping the card is the quick way onto the screen, but it does not
exercise the notification path — for that, without waiting for a live alert,
fire the same intent by hand (adb shell holds `START_ANY_ACTIVITY`):

```
adb shell am start -n net.twinion.hummingbird/.MainActivity \
  -a android.intent.action.VIEW \
  -d "hummingbird://alert/<alert-id>" \
  -f 0x24000000 \
  --es net.twinion.hummingbird.extra.ALERT_ID <alert-id> \
  --es net.twinion.hummingbird.extra.SOURCE "item-threshold/v1" \
  --es net.twinion.hummingbird.extra.SOURCE_KEY "item:<item-id>"
```

The action, the data uri and `-f 0x24000000`
(`FLAG_ACTIVITY_SINGLE_TOP or FLAG_ACTIVITY_CLEAR_TOP`) are the ones
`AlertNotifier` puts on its `PendingIntent`, and they are **not optional**.
`MainActivity` is `LAUNCH_MULTIPLE`, so without `SINGLE_TOP` a hand-fired
intent aimed at an already-running instance is delivered to the top activity
without `onNewIntent` — the command reports success, and the app does not
move. Cold (`am force-stop` first) it works either way, via `onCreate`, which
is what makes the flagless form look intermittent rather than wrong (#526).

A hand-fired intent exercises the routing but **not** the `PendingIntent` —
13 and 14 must be real taps; 15 and 18 are indifferent to how the screen was
reached.

13. Tap that notification warm: lands on the **item**, not the alert. One
    Back lands on `Now`.
14. Tap it cold (`am kill` again, after HOME — `am kill` is a no-op on a
    foreground process): same destination, and still one Back to `Now` — the
    second door has its own `popUpTo`, and this is what proves it. **A cold
    push is slow**: measured 2026-08-17, it took minutes to reach the killed
    app where a warm one was instant. Wait before concluding it was dropped;
    `wrangler tail` is the only way to tell late from lost.
15. Ack from the item's live-alert card; confirm `dismissed_at` on the
    authority. **Give the mutation queue a moment**, as in check 9.
16. Complete the item from item detail: two queue entries drain, the `act`
    first, and the alert leaves live without a second gesture (ADR-0027
    part 3).
17. Degrade check: ring a *non-item* alert (`POST /api/alerts`, ingest
    scope) and confirm the tap still opens **alert** detail unchanged.
    Cheapest of the six and needs no alarm wait — an ingested alert is
    delivered inside the request, not on the tick — and the two disabled
    `m2-proof-*` rules already match `healthchecks/v1`, so re-enabling one
    beats writing a new rule. Mint the ingest token bound to that same
    source (`"source":"healthchecks/v1"` on the mint body) so its blast
    radius is one fake healthcheck.
18. Open an archived item through a stale notification: readable, its
    checklist intact, and **no edit affordance** (Recall's rule, #478). Give
    it steps first, or this proves nothing about the checklist. Archiving
    marks only the item `Absent` — steps are demoted by their own
    `deleted_at` alone, so they stay `Live` in the mirror and still render;
    2026-08-17 confirmed both the mirror presence and the screen. The screen
    reads `ARCHIVED · READ ONLY` and drops every action, so the check's older
    "Ack still offered" clause only holds while the alert is still live —
    check 16 settles it, so run 18 against a separate alert if you want that
    half too.

19. **The skills runner lane (#538/M4).** Not a notification check at all —
    no alert, no rule, no alarm wait — and it ships **no screen**: the probe
    is an instrumented test, so what is proven is that the *lane* works, not
    that a Compose surface renders. It needs only the device token from
    check 2 and a real item to grill.

    ```sh
    ./gradlew :app:connectedDebugAndroidTest \
      -Pandroid.testInstrumentationRunnerArguments.ref=HB-42
    adb logcat -s HB-SKILL-PROBE
    ```

    Three cases, and the logcat output under that one tag is the evidence to
    paste into the PR: a real turn that reaches a question or a proposal
    **with the 20s `"still running"` heartbeats collapsed** (the runner beats
    every 20s; a narration with the same sentence twice in a row means the
    core's reducer did not run), a decline carried **verbatim**, and a
    mid-stream cancellation that delivers nothing afterwards.

    **A turn too short to beat fails the first case on purpose.** The probe
    asserts a heartbeat was actually seen before it asserts the collapse,
    because "no two entries repeat" is satisfied by a narration of one
    entry — a pass that would prove nothing about the reducer. If it says
    *no heartbeat was observed*, that is not the bug: re-run it against an
    item foggy enough to take more than one 20s beat.

    **To cause a decline deliberately**, use a `ref` the runner cannot
    resolve — the probe's second case does exactly that. It declines in
    `prepare`, before a model token is spent. Threading past
    `PROVISIONAL_TURN_CAP = 8` is the other way and also declines in
    `prepare`, but it costs eight real turns to get there.

    **This spends real model tokens**, on the first and third cases. It
    writes nothing: `grill-me` has no `apply` (ADR-0023), so unlike a
    `microtask` smoke test there is nothing to clean up afterwards. The
    automated half of this lane's evidence is `:app:testDebugUnitTest`'s
    MockWebServer suite plus `:app:assembleDebug` — CI runs those and
    nothing else, so a green badge is not a claim about a real run.

Screenshots need `adb exec-out screencap -p -d <display-id>`; without `-d`
adb writes a warning banner into the PNG, and the ids differ inner vs cover
(`adb shell dumpsys SurfaceFlinger --display-id`).

Afterwards: revoke the ingest token, disable the test rules (there is no
DELETE for rules, only PATCH), and dismiss the test alerts.
