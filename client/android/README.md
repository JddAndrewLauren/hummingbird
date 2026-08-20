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
because `client/ffi-mobile/src/lib.rs` exposes no Route query at all yet.
The web no longer has a Routes screen to reach parity with: #624 replaced
its demo-fixture mock with a real **Projects** page against live state, and
#449 puts Android explicitly out of scope for that lane, so this screen
stays as it is until a mobile-seam Route read exists to feed it. The sheet also
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
The queue's collapsed rows are the SAME compact card the Now screen's
frontier renders (`NowRow.kt`, extracted for exactly this — the
Triage-parity slice, operator request 2026-08-20), fed by a verbatim-copy
adapter over `TriageItemRecord` (whose `urgency` band arrives decided from
the seam, like every other pill). One item opens at a time, expanding at
index 0 of the queue's one `LazyColumn` — `NowScreen`'s inline-expansion
pattern — into the seeded editor built from #529's shared `ui/forms/`
components (`LevelSlider`/`ContextField`/`CaptureDateField`) under the Now
panel's chrome (stage chip, `titleMedium` title, X close). The expanded
pane is deliberately NOT `ItemDetailPanel`: `available_actions` answers
nothing for Triage/Grilling stages, and the panel's plain save is the
non-promoting write this surface bans. Promote-to-Ready is the only save
destination this screen offers — there is no "save without promoting"
method on `TriageViewModel` at all, unlike item detail's own edit mode. The row checkmark goes through the existing
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
`NowScreen` and `ItemDetailPanel` all carry — asserts a `LifecycleResumeEffect`
re-reads the queue on every return to the screen, not only on the app-wide
`syncTick`: a capture minted from `CaptureActivity` while Triage was
backgrounded must not wait for the next tick to appear.

The seam doors are `MobileTaskHost::triage_board(now)` (decided from the
already-sunk `hummingbird_core::decisions::queue::triage_process_queue`) and
`::triage_item()` (`Core::triage` with a real `promote_to_ready`, sharing
`to_triage_patch`'s `ItemEdit`→`TriagePatch` conversion with `edit_item`).

## The Grill takeover and the microtask affordance (M4, #539)

`GrillTakeoverScreen.kt`/`GrillTakeoverViewModel.kt` are the one-question-
at-a-time interview (ADR-0023), mounted from both the item screen's own
Grill button and the Triage row's (above) — never inline in either caller.
The review card's predicates (`wouldStrandPlan`/`demotesFromFrontier`/
`planReplacementLabel`), its "Proposed edit" rows (`grillProposalRows`,
#595 — the patch as read-only labelled rows beside the current values;
Confirm records the proposal unchanged, since this client ships no inline
edit) and the microtask affordance
(`ItemDetailRecord.microtaskAffordance`) all arrive applied from
`hummingbird_core::decisions::skills::{review,affordance}` (ADR-0025); the
Kotlin side decides none of them and never parses `patch_json`
(`GrillTakeoverStructuralTest` gates both). A draft auto-saves after every completed round,
not only on Back, so a fold/rotation mid-interview loses nothing —
`GrillTakeoverViewModel.open()` is idempotent per item id for the same
reason, and `ScreenStateRetentionTest` gates that the screen is retrieved
from the `ViewModel` store rather than `remember`. The microtask affordance's
own transport is `skills/MicrotaskRunner.kt`, the `skill_run_*` doors' first
real caller; `skills/BackendPreference.kt` is #274's picker, read into every
run and into the one-tap "switch tiers" offer a declined, unreachable pin
gets (`hummingbird_core::decisions::skills::backend::declined_backend_fallback`).

## The Recall overlay (M4, #542; reshaped by the search-overlay slice)

`RecallOverlay.kt`/`RecallViewModel.kt` are the milestone's closer: re-find
one known item across everything the mirror has ever known, live or
archived. Since the search-overlay slice (operator request 2026-08-20) it
is the web `RecallOverlay.tsx`'s shape as well as its counterpart: **an
overlay `AppRoot` draws over whatever route is showing, not a navigated
screen.** `Routes.RECALL` is gone entirely; the top bar's magnifier and the
More sheet's "Search everything" row both flip `AppRoot`'s `recallOpen`
flag, Back (or the X) closes the overlay in place, and the query field
auto-focuses on open. `MobileTaskHost::search(query, nowMs)` hands back a
`MobileRecallOutcome` — rows already matched, grouped
(`MobileRecallGroup::{Live, Done, Archived}`) and ordered, plus the core's
own un-capped `total` — over `hummingbird_core::search` (#478). Neither
file re-derives any of that: no sort, filter, group-by or
title/description scan of its own, gated by `RecallScreenStructuralTest`
the same way `RulesScreenStructuralTest` gates its own surface. Search is
as-you-type with no debounce — a mirror read, not a network request, the
same reasoning `useRecallWiring.ts` states on the web. Tapping a **live**
row expands it in place into the shared `ItemDetailPanel` below the row
(the web's own expansion; edit/act/steps in one implementation with the
notification door) — never a navigation. Done and archived rows are shown,
labelled and dimmed, but not tappable (#597). The expanded selection lives
in `RecallViewModel` (never a `remember {}`), closes on any keystroke, and
resets on a fresh open — the query itself survives, matching the web's
App-owned query.

## The standing-question panes (the pane-parity slice, over #536/#537)

`PaneShell.kt` renders the web `RankedRegion.tsx`'s own two-form contract.
Collapsed, a pane is one row: the band dot, the question's per-surface
label, up to `MAX_GLYPHS` of the pane's own marks, and its one-line
headline. Expanded (tap toggles), the same header collapses it again over
the headline, an unbound pane's "Open Settings" door, and the question's own
expanded content. The Status four's cards live in
`ui/panes/StatusPanesExpanded.kt` (the pane-content slice) — each web
`*PaneExpanded.tsx` ported: kimi's balance headline with the voucher/cash
split and the "cash owed" caveat, github's last-run/last-scheduled-success
lines with the "cron stalled"/"cadence unreadable" words, uptime's
expected-vs-observed observation line, reachability's synced-age headline —
dispatched from `StatusScreen`'s `expandedContent` and nowhere else
(`PaneContentStructuralTest`). The Now surface's pair lives in
`ui/panes/NowPanesExpanded.kt`: waste (the web's kerb-colour bin figures,
the wordier expanded headline, the holiday word) and race (series line,
countdown headline, next-session day/clock in the device's own zone, the
circuit). **Weekend and Vacation render no card of their own**: the mobile
seam hardcodes `calendar_connected = false`, so both are permanently
unbound on Android and the shell's setup rendering is the honest whole
story — the calendar-lane follow-up issue owns turning them on. One
recorded seam gap: the race card says "starting soon" without the live
alert's title, because only the `hasLiveAlert` fact crosses the mobile
seam. The web's Badge chips render as coloured meta words (no Badge
composable exists in the Android port), and the freshness caveat is the
shared `staleWords` line. The words and marks live in `ui/panes/PaneAnswers.kt` —
each question's `collapsedHeadline`/glyph decisions ported from its web
renderer (`client/web/src/screens/<q>-pane/`), composed from the decided
facts every `MobileRankedPane` carries since the pane-facts seam slice.
Nothing Kotlin-side re-derives a band or an answer state
(`PaneShellStructuralTest`); the one recorded deviation from the web is
the github/uptime stale-escalation wording, which prefers the staleness
caveat over recomputing a raw band (`PaneAnswers.kt`'s header).

Whether a pane STARTS collapsed is `ui/panes/PaneCollapse.kt`'s
band-stamped rule — the web `collapse.ts`'s semantics verbatim (default
collapsed when dormant or unanswered; an override applies only while the
pane is still in the band it was made in; a mismatch is a read-time
non-match, never a delete, so dormant → imminent → dormant resurrects it;
writes prune unranked keys). Overrides persist per surface in `PanePrefs`
(a Preferences DataStore, `FrontierPrefs`' reasons), owned by
`NowViewModel`/`StatusViewModel` — never a `remember {}` (the recorded
fold/unfold defect). `PaneCollapseTest` is `collapse.test.ts` ported case
for case.

## The UI iteration: icons, compact cards, filter disclosure, inline expansion, the capture FAB

Five slices bringing the surfaces in line with the design kit
(`.claude/skills/hummingbird-design/ui_kits/android/`) and the PWA:

- **Bar and sheet glyphs.** The web's `screen-icons.ts` map, as vendored
  Lucide vector drawables (`res/drawable/ic_*.xml`, each header naming its
  source glyph — `ic_inbox.xml`'s recipe). `navIcon()` beside
  `NavDestination` is an exhaustive no-`else` `when`, so a tenth
  destination fails to compile until it names a glyph;
  `BottomNavStructuralTest` pins the mechanism.
- **Compact cards.** A collapsed card is title + meta only — the action
  `FlowRow` left the Now cards for the opened item (the web `ItemCard`'s
  own shape). `NowScreenStructuralTest` pins that `NowScreen.kt` never
  reads `availableActions`. The refinement round (below) brought back
  exactly one inline act — the mark-done checkmark, gated on the seam's
  `canMarkDone`, with a complete-only door on `NowViewModel` — and the
  structural test now pins the door to that one verb.
- **The filter disclosure.** Only the axis switch keeps permanent space;
  the facet groups open behind one Filter chip carrying the active count.
  "N of M shown" comes decided across the seam
  (`NowBoardRecord.shown_count`/`total_count` — Kotlin never holds the
  pre-facet list, ADR-0025).
- **Inline expansion.** Tapping a Now card opens `ItemDetailPanel` — the
  whole former `ItemDetailScreen` body, extracted so the route (still the
  notification and Recall door, ADR-0027) and Now render one
  implementation — as item 0 of Now's one `LazyColumn`, above the board,
  which keeps rendering below (ADR-0021 decision 7). Selection is
  `NowViewModel.selectedItemId`, Triage's one-open-at-a-time shape.
  `NowItemDoorTest` pins the door end to end.
- **The capture FAB and sheet.** The design kit's extended FAB (the one
  sanctioned large ember fill) opens `CaptureSheet`, over the same
  `CaptureViewModel` and `ui/forms/` components as `CaptureActivity` (still
  the launcher icon's and shortcut's door); `CaptureSheetStructuralTest`
  pins the FAB and the no-second-Intent-door rule. The sheet was the *light*
  form here — no details disclosure — until round 4 gave it full field
  parity. It shipped mic-less first, too — a mic without recognizer
  plumbing is ADR-0022's dead control — and gained one in the refinement
  round via #611's extraction.

## The refinement round: top bar, mark-done, panel chrome, width parity (continues the iteration above)

Operator feedback on the iteration, applied as six slices on top of it:

- **The top bar.** The design kit's Android `TopBar`: the brand icon at
  24dp on its squircle plate (light/dark exports from
  `client/web/src/design/brand/`, swapped with the resolved theme), the
  lowercase wordmark, and a "Search everything" trigger to Recall — which
  until then was two taps deep in the More sheet. Same visibility rule as
  the bottom bar: top-level surfaces only.
- **Title-first cards, and the checkmark.** `NowRow` reads
  title-over-meta (the web phone `ItemRow`'s wrap order) and carries the
  web's `MarkDoneButton` as a trailing check `IconButton` in the Done
  green, gated on `NowItemRecord.can_mark_done` (decided in
  `ffi-mobile`, `MobileLedgerRowRecord`'s own field). Completing acts
  through `NowViewModel.complete` — `TriageViewModel.complete`'s exact
  shape — and re-reads the board in the same gesture.
- **One-line axis strip.** `AxisRow` scrolls horizontally instead of
  wrapping; the facet groups inside the disclosure keep their
  one-labelled-row-per-facet layout. (Round 4 kept the one line and dropped
  the scroll — the chips shrink to fit instead. See below.)
- **Panel chrome.** `ItemDetailPanel`'s header is the web `ItemPanel`'s:
  `HB-<seq>` mono meta line over a `titleMedium` title, the × close
  IconButton top-right (Cancel while editing, dirty path still confirms),
  StageBadge leading the meta row, 12dp gaps.
- **Width parity.** `ui/ContentMax.kt` caps the bar-tab screens' content
  at the web's `--content-max` (880dp), centred — the unfolded display
  stops stretching rows across its whole width.
- **The capture-sheet mic (#611).** `speech/Dictation.kt` extracted from
  `CaptureActivity` (`DictationHost` + `DictationFailure` + the
  raw-transcript listener, ADR-0022 invariants intact); the sheet wires
  the mic through it, its host living exactly as long as the sheet is
  composed. `DictationLocalityTest` retargeted; the sheet test's no-mic
  pin flipped to its positive.

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
`FlowRow`, and were left as they were. `PriorityRow` has since left that
group — round 5 dropped its fifth chip and the four that remain fit one
line, so it is a fixed `Row` with a measuring test of its own.

`ChoiceRowWrappingTest` is **the first test in this module that measured a
layout** — there are three now, and the two after it are built from its rig
— and the reason the defect got this far is that at the time no test did:
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

## Round 4: the capture submit pair, the shrunk axis strip, the Triage header

Operator batch 2026-08-20, three areas on one branch.

- **Two submit buttons, not a destination switch.** Both capture surfaces
  drop the Triage/Ready `FilterChip` pair for a **Triage** (outlined) and
  **Add** (filled) submit pair, `Add` carrying `CaptureDestination.READY`.
  The destination left `CaptureFormState` with it: once two buttons carry
  it, a field holding it is state no control writes. The title field's IME
  Done still submits to Triage, the funnel's own default.
- **The submit row is pinned on the Activity, scrolled to on the sheet.**
  The buttons were the last child of a scrolling column, so a raised
  keyboard could push the only way out of the screen off the bottom of it.
  On `CaptureActivity` the answer is a pinned footer: the fields scroll, the
  row does not, and `consumeWindowInsets(padding)` sits between the
  `Scaffold`'s padding and `imePadding()` — `padding()` applies insets
  without consuming them, so the IME inset would otherwise land on top of an
  already-paid navigation bar (the #614 dead band, in the other direction).
  **The sheet could not be pinned at all**, which took two failed attempts
  on hardware to establish: the IME inset does not reach a
  `ModalBottomSheet`'s window here, so `imePadding()` is a no-op and so is
  the sheet's own `contentWindowInsets` (`safeDrawing`, which nominally
  includes the IME). A `weight(1f, fill = false)` field column broke it a
  second, independent way — a `verticalScroll` child's desired height is its
  whole content, so with `fill = false` it claimed every pixel and left the
  row none. The sheet therefore scrolls to its submit row, the last thing in
  that scroll. The structural pin now says this per surface —`imePadding()`
  required on the Activity, banned on the sheet — because asserting it on
  both is what let the bug through.
  `CaptureViewModel.submitting` is the second `enabled` term on both
  buttons: three doors reach one `captureFn` (two buttons and the IME
  action), so a tap inside the first's suspension minted the same words
  twice, and the duplicate was indistinguishable from a deliberate one. It
  is released in a `finally`, so a failed enqueue does not leave both
  buttons dead for the screen's life.
- **The sheet is no longer the light form.** It gains the "More details"
  disclosure — description, project, priority, deadline, scheduled date —
  so the two capture surfaces differ only in which door they are, never in
  what a person can record through them. `CaptureSheetStructuralTest`'s ban
  on `detailsOpen` is inverted into the parity assertion it became, checked
  field by field rather than by the disclosure's flag alone. The Project
  picker moved to `ui/forms/ProjectField.kt` so its refusal of free text
  (`items.project_id` is an FK — a typo mints locally and dead-letters at
  the authority) holds on both. The sheet also stops gating on
  `canSubmit(draft.title)`: its dates are editable now, and the title rule
  alone would pass a malformed deadline through.
- **The axis strip shrinks to fit instead of scrolling** (superseding the
  2026-08-19 scroll, which superseded the original `FlowRow`). The "N of M
  shown" meta line leaves with the scroll — no room on one line, and the
  facet panel's footer already carries it. A fixed single-line `Row` clips
  what runs out of width, and the chip at the trailing edge is the Filter
  disclosure, the only door to an active filter, so this came with a
  **measuring** test. Measuring corrected the plan twice. First: five
  `FilterChip`s cannot fit at any text size, because a `FilterChip` spends
  32dp per chip on horizontal chrome — hence `AxisChip`, the same
  `secondaryContainer`-or-outline treatment built from a `Surface` as
  `StageBadge` already is, on 12dp of chrome. It also lost `ic_search`:
  measured, the icon was what pushed the row over, and the chip says
  "Filter" in words either way. Second, and caught only on hardware: the
  label must be `bodyMedium`, the sans body style, **not** `labelSmall` —
  that is the mono meta style (11sp Space Mono at +0.08em), which the design
  system reserves for computed values, and it is the widest small style in
  the scale. Using it cost 44dp the strip did not have, and the Filter chip
  clipped to "Fi" on the device while the unit test read green (see below).
  **The width the strip fits is the device's, not a stress width** (operator
  decision 2026-08-20): the Fold cover display is 443dp, leaving 419dp of
  content, and the strip wants 276dp. It does *not* fit the 272dp that
  `ChoiceRowWrappingTest`'s 320dp qualifier leaves — measured there, the
  Filter chip's count digit clips. The accepted limit is ~336dp of content;
  below that the trailing chip clips, which is the stated cost of a strip
  that neither wraps nor scrolls. `AxisRow` is also the one place in the app
  that waives `LocalMinimumInteractiveComponentSize`, and only its *layout*
  inflation: the chips stay 28dp tall, their full width is hittable, and the
  platform expands the touch target at the input layer regardless.
- **The Triage header title is the display and the edit both.** A "Title"
  text box below the header used to say the same words the header said, so
  the panel claimed the title twice and editing one changed the other. The
  header now reads the *draft's* title — an edit has to show where it was
  made — and `ic_pencil` swaps an inline field in for it; the title sits
  above the stage badge. The header row is the wide door out, through the
  same confirmation the × routes through, and clickable only while not
  editing so a tap into the field is not a tap on the way out. Editing ends
  on IME Done or when the pane closes, deliberately **not** on focus loss:
  that fires once with `isFocused = false` before the field is ever focused
  and would need a flag to tell the two cases apart.
- **The pane can mark done, and can no longer drop a draft silently.** The
  green check is `NowRow`'s own, on the seam's decided `canMarkDone`, wired
  to the same `complete` lambda the collapsed rows call — one act path
  whether the pane is open or shut. It sits on a line of its own below the
  `ChoiceRow` rather than beside it, because that row wraps at narrow widths
  (#576) and anything sharing its line moves when the buttons do. And
  `select(sameId)` nulls the draft (`TriageViewModel.select`), so re-tapping
  the already-open row dropped typed words without asking — the one leaving
  gesture that skipped the confirmation the ×, Back and the header tap all
  route through.

`AxisRowWrappingTest` is the module's **third** layout-measuring test,
`ChoiceRowWrappingTest`'s rig with the same two halves — the measurement and
a control proving it has teeth. One thing in it is worth copying rather than
re-deriving: it measures **unconstrained**, in a 2000dp box, not at the
272dp budget. Rendered at exactly the budget the `Row` squeezes whatever
runs out of width, so the trailing chip reports bounds *inside* the budget
no matter how badly it overflows. The first draft of that test passed with
the Filter chip measuring 272dp..272dp — crushed to nothing, which is the
defect itself. Measuring what the strip *wants* is the only form of the
assertion that can fail.

And a second thing, which cost a shipped clip to learn: **a Compose
measurement test measures the theme it is given.** That first draft rendered
`AxisRow` bare, so `MaterialTheme.typography.labelSmall` resolved to
Material's default — Roboto 11sp, no tracking — instead of the app's Space
Mono at +0.08em. It measured 268dp and passed while the device clipped the
Filter chip to "Fi". Wrapping the content in `HummingbirdTheme` is what made
the numbers real, and it is not optional in any test that measures text.

## Round 5: the capture panel's own shape

Operator batch 2026-08-20, four requests, all on the capture surfaces. The
sheet is what the operator was looking at; where a change is layout the two
surfaces share, `CaptureActivity` took it too, because two doors onto one
field set must not disclose it with two different controls.

- **The sheet opens cold at the top of the window.** `skipPartiallyExpanded`
  plus `Modifier.fillMaxHeight()`, and both are needed: the flag removes the
  half-height resting stop, and the modifier is what makes the sheet tall at
  all, since an expanded `ModalBottomSheet` is otherwise only as tall as its
  content and this form is shorter than the window with the disclosure shut.
  A sheet that starts half-height and grows moves its own fields under a
  reader who is already typing into them.
- **No heading on the sheet.** The FAB that opens it is labelled Capture and
  the focused field's placeholder asks the question, so a headline spent the
  top of a full-height panel restating the gesture. `CaptureActivity` keeps
  its own — it is a launcher destination that arrives over whatever was on
  screen before, and has no FAB behind it to have said so.
- **The details disclosure is a chevron.** `ic_chevron_down`, rotated a
  half-turn when the fields are out — `NowScreen`'s `ColumnHeader` idiom,
  and the design system's "Unicode as icons: never" rule. The words it
  replaces survive as the `contentDescription`, so the control still names
  itself to a screen reader; a bare glyph with no accessible name would have
  been a downgrade, not a simplification.
- **Priority is one line of four chips.** "No priority" is gone — not
  picking one already says it, and a chip for the absence of a choice is a
  fifth target meaning what the resting state means. Clearing is what it
  always was: re-tap the selected chip (the `""` sentinel `PriorityRow`
  clears to, and the reason it differs from `LevelSlider`'s `null`). Losing
  the fifth chip is what let the `FlowRow` become a fixed `Row` — measured,
  four default `FilterChip`s want 303dp against the 395dp of content the
  Fold's cover display leaves after 24dp gutters, and the five-chip row did
  not fit. It is a `Row` rather than the compact `AxisChip` treatment
  because `LevelSlider` sits one field above it in the same form: two
  adjacent chip rows with different chrome read as two different controls.
- **Deadline and scheduled date share a line.** They are read as a pair —
  when it is due against when it is planned — and stacking them spent two
  rows of a disclosure holding five fields. `weight(1f)` each, `Alignment.Top`
  so a refusal under one does not shift the other. `CaptureDateField` grew a
  `modifier` parameter for it, defaulting to the full-width field the Triage
  editor still stacks inside its narrower card.

`PriorityRowWrappingTest` is the module's **fourth** layout-measuring test —
`ChoiceRowWrappingTest`, `FacetLabelAlignmentTest` and `AxisRowWrappingTest`
are the others, and `docs/SURFACES.md`'s Android row names all four —
`AxisRowWrappingTest`'s rig unchanged — including the two things that rig
exists to carry: measure unconstrained (a `Row` rendered at its budget
squeezes the overflow into bounds that look like a pass), and wrap the
content in `HummingbirdTheme` (a bare render measures Material's default
faces, not the app's). Its control renders the five-chip row this replaced
and asserts it overflows, which is what makes dropping the fifth chip a
recorded consequence rather than a taste.

One structural test in this round was green and vacuous before it was
right, and it is worth naming because the shape recurs. The date-pair pin
counted `modifier = Modifier.weight(1f)` occurrences with an unbounded
regex; the submit buttons a few lines below carry the same string, so
deleting a date field's weight left the count at two and the file green. It
was only caught by deliberately mutating the source and watching the test
*not* fail. A structural pin over source text needs its search bounded to
the block it is claiming something about — and needs the mutation run, since
nothing else distinguishes a pin that holds from a pin that matches
elsewhere.

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
nothing with 1–18 but the device token. Checks **20–27 are #527's Hardware
Checkpoint 3** — the nine-screen parity pass, in its own section below;
they are not part of the notification lane and re-running 1–19 does not
cover them.

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
   token there (#535 — Status itself carries no token field). The verdict
   is Settings' own SYNC card, not Status: it reads `Held — device token
   needed` before the paste and carries a `Sync now` button, and the DEVICE
   TOKEN card above it flips to `This device has a token`. Status shows no
   sync line at all since #536 replaced ProofScreen — read the panes
   instead: an uncredentialed phone renders every one of them with a
   no-answer headline (`No answer yet` / `Not set up` / `Never synced on
   this device.` since the pane-parity slice), which is the fastest tell
   that the token is missing.
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

### The nine-screen pass (#527's Hardware Checkpoint 3)

Checks 20–27 are the parity checkpoint, not the notification lane: nine
screens reachable, Status matching the web, one real streamed grill turn,
and Recall re-finding a known item. They share only the device token with
1–19. **All of 20–27 were run on 2026-08-19 against `9fdead3`** (Pixel 10
Pro Fold, folded). 20 and 22 passed clean; 26 and 27 passed their own
claims, though 26 also turned up #597; 21, 23 and 24 found #574, #575 and
#594/#595/#596; 25 passed every clause it can (#599 explains the one it
cannot). Two clauses in this section are **unprovable through the shipped
UI** rather than merely unrun — 24's decline (#594) and 25's tier fallback
(#599) — so a future pass should re-read those two issues before treating
either as outstanding work.

Do **not** reach for check 19's instrumented probe to satisfy 24. It was
the pre-screen substitute; #539 shipped the real takeover, and the probe
costs the device token every run.

20. **Install and credential.** `./gradlew installDebug` from
    `client/android` (about a minute with the toolchain warm), then check 2
    above. The mirror is populated when Now's Context facets carry real
    values and the waste pane leaves `Not set up yet`.
21. **All nine screens.** Four on the bar — Now, Triage, Alerts, Status —
    and Done, Ledger, Rules, Settings, Routes in the More sheet, with
    "Search everything" below the screen list (a gesture, outside
    `NavDestination`; Recall renders no bottom bar at all, which is the
    tell that it is not a destination). Routes must render **only** "No
    routes yet": a populated Routes means a demo fixture shipped. Then the
    back stacks: scroll one tab, visit another, come back — the screencaps
    should be byte-identical (`shasum` them), which is `saveState`/
    `restoreState` doing its job. **Tap each bar destination from a
    *foreign* stack too**, not just from Now — #574 is exactly that case,
    and a tab that silently restores someone else's screen looks like a
    dead tap.
22. **Status against the web** (#536's own proof line). Compare **set,
    order and band** — never the wording: `RankedPaneRecord` carries
    `question`, `subject_key`, `pane_key` and the answer, and no copy at
    all, so "Kimi balance" against "Model credit balance" is two renderings
    of one record. Expect Uptime to fan out one row per subject
    (`uptime_subjects` → authority, runner, web) on both clients, and
    `pending` (`NEVER_POLLED_SUBJECT`) wherever a source is unprovisioned —
    #416/#486 are open, so unprovisioned is the normal reading and does not
    block this check. **Reachability is the one legitimately divergent
    pane**: each client reads its own sync history (Android's
    `SyncHistoryStore`, the web's own), so its band may honestly differ.
    The web renders it as a "This device" line that reads like a page
    header rather than a pane — count it, or you will report a missing
    pane. Measured 2026-08-19: six panes, identical order both sides —
    reachability, Uptime ×3, Kimi, GitHub.
23. **Now's panes, and the weekend do-date write** (#537). The three panes
    below the queue render through the same shell Status uses. The write
    half **cannot be run on this device**: `weekend.rs`'s
    `!calendar_connected` is the only path to `Unbound`, Android reports no
    calendar (#564), and the affordance was never built anyway (#575). Skip
    it until #564 lands rather than hunting for the control.
24. **A real grill turn** (#539), from **both** mounts — item detail's
    Grill button and a Triage row's (gated on `canGrill`). One turn must
    show all three: heartbeats **collapsed** (the runner beats every 20s;
    the same sentence twice running means the core's reducer did not fire —
    and a turn too short to beat proves nothing, so pick a genuinely foggy
    item), a decline carried **verbatim** (cheapest deliberate cause is a
    `ref` the runner cannot resolve — it declines in `prepare`, before a
    model token is spent), and a mid-stream **cancellation** delivering
    nothing afterwards. Fold or rotate mid-interview: the draft auto-saves
    after every completed round. **Spends real model tokens**, and writes
    nothing — `grill-me` has no `apply` (ADR-0023), so there is no cleanup.
25. **The microtask affordance and the backend picker.** The affordance on
    item detail, a run that narrates as it streams, and Settings' SKILLS
    BACKEND picker (Auto / Cloud runner) persisting device-locally.
    **Unlike a grill, this writes** — `microtask` has an `apply` and mints
    Step records (ADR-0023) — so run it against a throwaway item and clean
    up afterwards: soft-delete the steps (`PATCH /api/steps/:id` with
    `deleted_at`; there is no DELETE) and archive the item. Measured
    2026-08-19: narration streamed `reading <id> from the authority` →
    `item <id> has 0 live steps` → `running skill microtask`, then 11 steps
    about 100s later; the affordance then flipped to **"Rewrite 11 steps"**,
    which is `MicrotaskAffordance::Rewrite` applied — a free check that the
    sunk affordance decision is live.
    **The tier fallback cannot be checked here (#599).** Android's registry
    ships one entry, so `fallback_backend_id` finds no id that is not the
    dead one and a pinned `cloud` offers `None`; an Auto selection returns
    `None` at its own early return. Every reachable configuration answers
    `None`, which is correct, not broken. Blocked until #275/#276 append a
    second entry — do not go hunting for the affordance.
26. **Recall** (#542, dimming corrected by #597): search as you type, three
    groups — Live, Done, Archived — and live rows opening item detail.
    **Done and archived rows are both dimmed** (`INERT_ALPHA`) **and both
    inert**: the alpha tracks tappability, so the two inert groups read the
    same and only the live row reads solid. Neither Done nor archived is
    tappable, so prove inertness against a **positive control** — tap a
    live row in the same column first, or "nothing happened" equally
    describes a tap that missed. Re-find a known archived item; that is the
    milestone's closing claim. Measured 2026-08-19: `office` populates all three groups (live
    *Kathryn office disco ball smart plug*, done *Dr. Shira Keri's Office*,
    archived *Fwd: Dr. Shira Keri's Office* ×2) and `526 repro` re-finds
    both archived probes. Seed first if the mirror is thin.
27. **The doors survived the nav churn.** #532/#541 rewrote navigation
    under #521/#524, so re-fire the item intent by hand — the block above
    has the command, and `-f 0x24000000` is not optional. A tap on the Now
    card is the other door and does not exercise this one. Measured
    2026-08-19: warm (from Recall) and cold both land, and firing two
    payloads naming **different** items landed on each in turn — check that,
    not merely that some detail screen opened, or a hardcoded destination
    would pass. `am` printing "Activity not started, intent has been
    delivered to currently running top-most instance" is the **success**
    line for the warm case. The `healthchecks/v1` fallback was fired too and
    landed on alert detail.

### Round 5's capture-panel pass (2026-08-20, `be39ede`)

Pixel 10 Pro Fold, folded, 443dp cover display. Read off the **accessibility
tree** (`adb shell uiautomator dump`), not off the screenshots, and that is
the transferable part: driving this by screenshot alone drifted twice — a
`monkey -c LAUNCHER` launch picked the *second* launcher icon
(`CaptureActivity`, not `MainActivity`; use `am start -n` and name the
Activity), and a swipe meant to scroll the sheet dismissed it instead, after
which taps at remembered coordinates landed on whatever screen was
underneath. The dump gives node bounds, which answers the layout question
directly and cannot land on the wrong screen without saying so.

Measured: the four priority chips share `y=1064–1105` and trail at `x=768`
of 1080 — one line, with room; Deadline `[98,1241]` and Scheduled date
`[589,1241]` share a line; no `Capture` text node exists on the sheet; the
drag handle sits at `y=213`, clear of the status bar.

That last one is a fix, not a measurement. The first full-height build put
the title field's outline against the status-bar clock: a half-height sheet
had never met the status bar, so this sheet had never paid the inset and had
never needed to. `contentWindowInsets = { WindowInsets.statusBars }`, and
`CaptureSheetStructuralTest` pins it now — nothing else in the module can
see it, since the sheet composes and every field pin passes with the line
deleted.

Screenshots need `adb exec-out screencap -p -d <display-id>`; without `-d`
adb writes a warning banner into the PNG **and the file is plain text, not a
PNG at all**, and the ids differ inner vs cover — they are the long
`SurfaceFlinger` ids (`adb shell dumpsys SurfaceFlinger --display-id`), not
`0`/`1`, which `screencap` rejects as invalid. On a folded Fold the
outer panel is the live one — `adb shell dumpsys display | grep -c
mScreenState=ON` will not tell you which; read `state ON` off the
`DisplayDeviceInfo` block for "Outer Display" / "Inner Display".

Afterwards: revoke the ingest token, disable the test rules (there is no
DELETE for rules, only PATCH), and dismiss the test alerts.
