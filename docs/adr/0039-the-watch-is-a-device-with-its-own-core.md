# ADR-0039: The watch is a device with its own core

**Status:** accepted · 2026-09-10 · amended inline 2026-09-10 (the Wear
capture design handoff, on #807: decisions 3, 5 and 6, and "what this does
not decide")
**Context:** map #35's build order (desktop web → native Android → Wear OS)
reached the watch, and #129 — a watch tile per standing-question pane — had
parked as `ready-for-human` because no Wear client existed and its specifics
were left to a grilling. That grilling happened on 2026-09-10 and settled
what the first Wear slice is: **a watch app, not tiles first** — a large
capture button, a smaller button opening the Now surface's standing
questions, and one capture tile. Per-question tiles (#129's original ask)
are a later slice. This ADR records the decisions that shape the client,
what was rejected and why, and what it costs. It amends
[ADR-0003](0003-one-rust-sync-core-embedded-per-device.md) (a third
consumer of the embedded core exists, and `:core-binding` is its Gradle
seam) and supersedes map #35's "watch capture stays on the Tasks widget
funnel". `CONTEXT.md` gains no term: the watch is not a **View** (a View
renders one engine's published state; the watch has an engine of its own).

## The decision

1. **The watch embeds its own Rust core, with its own `device` token,
   `device-watch`.** ADR-0003's doctrine applies unchanged: one core per
   device, a full mirror, a durable outbound queue, sync as a library. The
   alternative — a phone-relay client over the Wearable Data Layer — was
   rejected on three counts: it would be a View over *another device's*
   core, which nothing else in this system is; it would be a second sync
   transport with its own failure modes and no ADR-0007 guarantees; and it
   would make a glance depend on the phone being present and awake, which
   is the one thing a glance surface must not do. `device-watch` joins the
   `device` token population (CLAUDE.md, credential blast radius); it is
   write-everything like every other member, and the phone's own token is a
   different credential.

2. **The token is delivered from the phone, once, over the Data Layer, and
   the phone keeps nothing.** Settings' Watch card takes the raw token, sends
   it on `/hummingbird/device-token` (`TokenMessage`, one definition read by
   both ends) to the paired nodes, and clears the field on success; no
   `ViewModel` field and no store on the phone ever holds it. The watch's
   `TokenListenerService` stores it in `EncryptedSharedPreferences`,
   **pushes** it to the core (the entry that resumes a credential hold),
   refreshes the home screen's line and enqueues a user-trigger sync.
   Typing on the watch, a QR code, and a Bluetooth-independent hand-off
   were rejected: the first is hostile on a 1.2-inch display, the second
   needs a camera the watch does not have, and the third would be a fourth
   credential path for one device. The Data Layer's delivery condition —
   same package name AND same signing certificate on both ends — is the
   whole reason `:wear` ships under the phone's `applicationId` and key
   (decision 8).

3. **Capture is the system's own input chooser.** `CaptureActivity` opens
   Wear's `RemoteInput` chooser at once — voice first, keyboard and the rest
   behind it — and hands the answer to the core's `canSubmitCapture` gate,
   then to the same `capture` door the phone uses, as a title-only Triage
   draft with every other field empty (deciding is mint-time work). A brief
   confirmation, and back to the button; a cancelled chooser finishes
   silently; no undo. There is no hummingbird-owned microphone on the watch.
   [ADR-0022](0022-dictation-is-local-only.md) needs no amendment: its
   decision 4 already places OS text entry, dictation included, outside the
   local-only guarantee.

   *Amended 2026-09-10 (the Wear capture design handoff, #807):* the line no
   longer goes straight to Triage. The core's gate is asked first, then a
   **destination screen** draws the transcript as spoken over the web
   capture box's three squares as three rounds — Triage (the inbox on
   `Sky600`), Mint action (the plus on the accent), Mint for today (the
   `calendar-check` on `Ember700`, stamping `WallClock.todayDeadline`, the
   one such rule on Android, shared with the phone's third button) — and
   the confirmation names where it landed (`TRIAGE`, `READY`, `READY · DUE
   TODAY`). Still title-only otherwise; deciding is still mint-time work,
   and the two extra facts are the tap's, not a form's. **Voice is the
   first door** (operator, 2026-09-10): the activity opens the system
   speech recogniser already listening, and the input chooser — keyboard,
   emoji, voice again — is the second, reached only when the recogniser
   hands nothing back. Both are the OS's UI; Wear's chooser has no setting
   that picks a method, so zero-taps-to-listening is a recogniser intent.

4. **The questions list is the Now surface's `rank_panes`, rendered on Wear,
   in the order the core returns it.** The six Now questions, in salience
   order exactly as the web's Now screen and the phone's Now panes draw
   them; each row a band dot, the roster's label
   ([ADR-0034](0034-a-standing-question-can-be-switched-off.md) decision 4's
   `question_roster`, with the race pane's series appended as the phone
   does), and `paneHeadline`'s one line. Tap expands the facts in place
   (`WearPaneExpanded`: the phone's Now cards said in text alone, over the
   same `:brand` words). A switched-off question never appears — the core
   already omits it (ADR-0034). No Settings door on the watch: an unbound
   question shows its sentence and stops. Nothing on the watch sorts,
   filters, or re-derives a band; `WearQuestionsStructuralTest` pins it.

5. **One capture tile ships now; per-question tiles are deferred, with
   their shape fixed.** The tile is static — a line and an edge button whose
   `LaunchAction` opens `CaptureActivity`. Per-question tiles stay #129's
   scope, and when they come their opt-in will be **a fourth `settings`
   vocabulary shaped like `question_switch.rs`** — a synced preference over
   the standing-question roster — and never a stored tile record: #117's
   argument against a per-device record in the synced schema applies whole.
   Their refresh budget is the open question #129 still owes a grilling.

   *Amended 2026-09-10 (the Wear capture design handoff, #807):* **the one
   tile is no longer static.** It draws from the mirror — an urgency arc
   around the face (overdue, then everything due within the core's three-day
   window as one segment; two colours read at a glance on a 1.4-inch face,
   three do not — operator decision) and a mono count line — around the
   ember feather disc that opens `CaptureActivity`, with two glyph rounds
   beneath it that open the Items and Questions lists. Its refresh is
   decided: a redraw request after every completed sync (both legs;
   `SyncWorker` gained a host hook the phone leaves unset) and an hourly
   freshness interval as the fallback — one cadence, no second clock. Its
   honesty rule is decision 6's: once the mirror is an hour old the count
   line reads `SYNCED nH AGO` rather than a stale number as a current one;
   with no token it draws the disc alone. Every colour is a `:brand`
   constant through the tile's own ProtoLayout `ColorScheme`. Per-question
   tiles, their content and *their* refresh budget stay #129's.

6. **The home screen is two buttons and at most one line.** Capture (ember —
   the one accent, its one use here; `feather`, the brand's verb) and
   Questions (tonal), and beneath them either "Send a token from the phone"
   (no token, or the last cycle refused it) or "synced Nh ago" once the
   mirror is over an hour old — the design README's honesty rule. Inside the
   hour the screen says nothing.

   *Amended 2026-09-10 (the Wear capture design handoff, #807):* three
   buttons — Capture, then **Items** (tonal, `zap`) and Questions (tonal,
   `help-circle`), the tile's two rounds as words. **Items by urgency** is a
   third screen: the frontier on the core's `Urgency` axis — the `nowBoard`
   door the phone's Now screen reads, flattened column by column so the
   flattening is the order and nothing on the watch sorts
   (`WearItemsStructuralTest`, decision 4's rule re-applied). One card per
   item: the dot in the urgency colour, one mono line folding the band and
   the deadline (`OVERDUE · THU`, `DUE TODAY`, `DUE FRI`, `DUE OCT 3`, `NO
   DEADLINE` — a rendering of the core's band against the board's own day,
   per-client under ADR-0025 as the phone's `urgencyLabel` already is), the
   title, the context and size when set. Tap expands one card at a time:
   the description (`itemDetail`, fetched then), and **Open on phone** — an
   item link, `hummingbird://item/<id>`, spelled once in `:core-binding`'s
   `ItemLink`, carried by `RemoteActivityHelper` and claimed by the phone's
   `.ItemLink` alias onto its existing item-detail route. The watch still
   reads and never writes: a launch on the phone is not a write on the
   watch. The questions list is restyled to the same card.

7. **The sync model is the phone's, minus push.** One deliberate cycle on
   every resume, the 60-second foreground cadence while resumed
   ([ADR-0007](0007-the-sync-engine.md)), WorkManager's hourly leg — and no
   FCM on the watch this slice. Between opens the watch may be up to an hour
   stale, and decision 6 prints that rather than hiding it. Still one clock
   per cadence (issue #8): the one-shots the listener and capture enqueue
   are event-driven, not a second tick.

8. **Build mechanics.** `client/android/` is a multi-module Gradle build:
   `:core-binding` (the two cargo tasks, the UniFFI binding,
   `AUTHORITY_BASE_URL`, `CoreHolder`/`TokenStore`/`SyncWorker` and the
   rest of the host core package) and `:brand` (tokens, the bundled
   typefaces, every Lucide drawable, the pane words) are libraries both
   `:app` and `:wear` consume; the cargo tasks run once. `:wear` ships under
   `applicationId net.twinion.hummingbird` and the phone's release key, minSdk
   34 (Wear OS 5; no Wear release is API 35, so the two libraries sit at 34
   and `:app` stays 35), **armeabi-v7a plus arm64-v8a** — *amended
   2026-09-10 by the first hardware install*: this decision first read
   "arm64-v8a only (the Pixel Watch 3/4 is the only target)", and the Pixel
   Watch 4 refused that APK with `INSTALL_FAILED_NO_MATCHING_ABIS` because its
   userspace is 32-bit (`ro.product.cpu.abilist` is `armeabi-v7a,armeabi`,
   with no 64-bit list at all). So `:core-binding`'s one cargo task now
   cross-compiles a third target, `armv7-linux-androideabi`, and `:wear`
   packages armeabi-v7a (the watch) beside arm64-v8a (the arm64 Wear AVD, and
   any 64-bit watch); x86_64 is still filtered at packaging. Wear Compose stays on 1.5.x because 1.6 requires Compose 1.9 and
   would force a BoM bump on the phone for a watch-only need. Every colour
   the watch draws is a `:brand` constant — no `Color(0x…)` literal under
   `wear/src/main`, pinned — so the design mirror's drift gate covers the
   watch through `Color.kt`.

## Rejected

- **Phone relay over the Data Layer** (decision 1's three counts).
- **A stored tile record** for per-question tiles (decision 5; #117).
- **Duplicating the cargo tasks per application module** — one `.so` build,
  one binding, or the two drift.
- **Typing or a QR code on the watch** for the token (decision 2).
- **A hummingbird-owned microphone on the watch** (decision 3; ADR-0022 D4).
- **arm64-v8a only on the watch** — the original text of decision 8,
  overturned by the hardware on 2026-09-10 (see the amendment there).
- **x86_64 on the watch** — the emulator is not a target this slice; a
  `-PwearEmulator` widening is documented, not built.
- **FCM on the watch this slice** — the hourly leg plus the honesty line is
  the accepted staleness; a push lane is a later decision.
- **Bumping the Compose BoM for Wear Compose 1.6** (decision 8).

## Costs

- A second APK per deploy (`deploy.sh` builds both and prints both
  certificate lines, which must match).
- Developer options on the watch for the first install (`adb pair` /
  `adb connect` over Wi-Fi); later installs go over the same route.
- **The debug/release key mismatch breaks the token send silently**: the
  Data Layer accepts a message to a watch running the debug build while the
  phone runs release, and it never arrives. Install the watch with the
  release key from the first install, or the token is lost on the key
  switch (an uninstall costs the token and the queue, as on the phone).
- About 10 MB of `.so` per watch install.
- Up to an hour of staleness between opens (decision 7), printed.
- `internal` → `public` on the shared words in `:brand`.

## What this does not decide

- Per-question tiles' refresh budget and content (#129, still owed a
  grilling; the opt-in shape is decided above).
- A push lane on the watch.
- Whether the watch ever writes anything but a capture (marking done, an
  ack): the first slice is capture and read. *(Unchanged by the 2026-09-10
  amendments: "Open on phone" launches the phone's item detail and edits
  nothing on the watch.)*
- Any change to what a Now pane *says* — the words are `:brand`'s, decided
  once for both devices; a watch that needed different words would be a
  per-client rendering decision under
  [ADR-0025](0025-decisions-sink-to-the-core-rendering-stays-per-client.md).
