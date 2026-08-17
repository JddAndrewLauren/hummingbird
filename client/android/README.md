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

`notify/NotificationChannels.SPECS` must byte-match the `channel_id` values
`server/authority/src/fcm.rs` emits; nothing links the two literals at
compile time, so `NotificationChannelSpecTest` is what does.

**`google-services.json` is not in the repo and the
`com.google.gms.google-services` plugin is not applied.** The
`firebase-messaging` dependency links without either, and every Firebase
touch is guarded (`push/PushBootstrap`), so the app builds and runs with no
push until the operator adds the file and the plugin line — one commit,
kept out of CI because the key is an operator credential.

CI is `.github/workflows/android.yml` (Gradle side) plus `client.yml`
(the Rust side, whose `client/**` filter covers this directory).
