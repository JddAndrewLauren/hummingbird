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

## Proving the lane on hardware

CI cannot cover any of this: there is no emulator in `android.yml` and no FCM
delivery without a real device, so the twelve checks below are the only
evidence the lane works end to end. They were run in full on 2026-08-17
(Pixel 10 Pro Fold, SDK 37, #517) and every one passed. Re-run them after any
change to `notify/`, `push/`, or the deep link.

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
2. Paste the device token on Status; confirm it reads `Synced`.
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
9. Ack from the alert detail screen; confirm `dismissed_at` again. **Give the
   mutation queue a moment** — this one goes through the queue, not
   `AckWorker`, so an immediate read of the authority still shows `null`.
10. Tap a notification with the app running: lands on that alert's detail.
11. Tap one cold. Kill with `adb shell am kill net.twinion.hummingbird`, not
    `force-stop` — a force-stopped app receives no FCM at all.
12. Swipe a notification away and confirm **nothing** is acked (ADR-0012).

Screenshots need `adb exec-out screencap -p -d <display-id>`; without `-d`
adb writes a warning banner into the PNG, and the ids differ inner vs cover
(`adb shell dumpsys SurfaceFlinger --display-id`).

Afterwards: revoke the ingest token, disable the test rules (there is no
DELETE for rules, only PATCH), and dismiss the test alerts.
