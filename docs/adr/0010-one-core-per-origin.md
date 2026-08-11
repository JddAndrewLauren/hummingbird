# ADR-0010: One core per origin, held in a SharedWorker

**Status:** accepted · 2026-08-09
**Context:** the multi-instance question
[ADR-0007](0007-sync-is-one-cycle-drain-then-full-sweep.md) left open — it
specifies the cycle but never says how many run at once — raised as issue
[#97](https://github.com/JddAndrewLauren/hummingbird/issues/97) under plan
[#95](https://github.com/JddAndrewLauren/hummingbird/issues/95), and renumbered
here by [ADR-0009](0009-the-owned-schema-and-context-lanes.md), which took 0009.
Amends [ADR-0003](0003-one-rust-sync-core-embedded-per-device.md): on the web
host the core is embedded once per *origin*, not once per document. Also amends
[ADR-0007](0007-sync-is-one-cycle-drain-then-full-sweep.md), which was silent
on multiplicity rather than wrong about it. Narrowed by
[ADR-0008](0008-the-authority-is-an-app-owned-server.md), whose entity-level CAS
already makes concurrent **sends** safe, leaving only client-side coherence to
decide.

## Decision

**The core lives in a `SharedWorker`, and there is exactly one per origin.**
Every browser tab and the installed PWA window is a **view**: it connects a
`MessagePort`, receives the same event stream, and may issue triggers. It never
owns a core, an outbound queue, a delta cursor, an in-memory mirror, or a handle
on the snapshot store.

- **What a view may do:** issue the ADR-0007 cadence triggers (app open, window
  focus, reconnect, manual refresh), submit captures and edits, and render the
  core's published state.
- **What only the core has:** the outbound queue and its dead-letter journal,
  the delta cursor, the in-memory mirror, the IndexedDB snapshot handle, the
  backoff state, and the device credential
  ([ADR-0004](0004-client-linear-credential-is-scoped-per-device-host-supplied.md),
  as ported to the owned API's bearer token).
- **Lifetime:** the core starts on the first connection and ends when the last
  port disconnects. **Closing a tab is a port disconnect, never a handover** —
  the cycle in flight is not interrupted, and no other instance inherits a queue
  whose head may or may not have reached the server.

### The failure modes this makes unrepresentable

Duplicate *sends* were never the problem: creates are idempotent by
client-supplied id and every mutation is an absolute-value set gated by
`expected_version`, so the server serialises concurrent writers (ADR-0008). The
three failures that survive CAS are all failures of a *second local queue*:

1. Two queues carrying divergent conflict metadata dead-letter the same edit
   twice.
2. Last-writer-wins snapshot replacement resurrects another instance's
   already-drained queue entry.
3. Two instances rebasing the same 409 independently each apply the rebase, and
   the second write is no longer the edit the human made.

None is defended against here. All three require two queues, and only one is
constructible.

**This hazard is structural, not hypothetical.**
`client/core/src/storage/indexed_db.rs` `put`s the whole envelope under one
fixed key with no read-modify-write, and `client/core/src/storage/mod.rs`
promises atomicity against a *crash* — "either fully publishes `bytes` or leaves
the previously published snapshot intact" — not against a concurrent writer. Two
cores holding divergent in-memory mirrors are plain last-writer-wins over the
entire queue.

**It is also already shipping.** `client/web/src/worker/core.worker.ts`
hard-codes the `hummingbird-calendar` namespace and `client/web/src/App.tsx`
constructs a fresh dedicated `Worker` per app instance, so two tabs open today
are two cores writing one IndexedDB database on their own 15-minute timers. That
is benign only because the persisted calendar state is derived data the next
poll refills. The moment a queue of user-authored captures lives in that
envelope, the same structure is silent data loss.

### The delta cursor, and duplicate triggers

**Exactly one cursor exists, owned by the single core.** No arbitration rule is
needed and no gap can open between two advancing readers. ADR-0007's full sweep
remains the correctness backstop it already was, for the reasons that ADR gives
— not as a repair for concurrency.

N views firing focus and timer triggers converge on the core's existing
one-at-a-time request queue (`createRequestQueue` in
`client/web/src/worker/calendar-worker.ts`, which exists because the host is not
re-entrant). Duplicate triggers are therefore wasteful, never incorrect;
collapsing them into a "cycle already running" no-op is an optimization, not a
correctness requirement.

### The assumption this rested on — probed and confirmed

**Assumed:** an installed PWA standalone window shares a `SharedWorker` instance
with ordinary browser tabs on the same origin.

**Confirmed by live probe on 2026-08-11**
([#172](https://github.com/JddAndrewLauren/hummingbird/issues/172), build
`v0.1.0+dev`, Chrome on macOS): two ordinary tabs and the installed standalone
window all reported the **same** core instance id (`01d13673`) with ordinals
#1, #2 and #3 — one core, three views, exactly as this ADR specifies. It was
committed to without a probe (neither Chrome's nor MDN's documentation settles
it), and two earlier agent runs could not run one — no PWA install in a sandbox,
and a confirmation-gated browser tool in an unattended batch — so it stood as a
recorded open question through #126 and #105. It is load-bearing: had a
standalone PWA window been its own SharedWorker scope, the intended
configuration — the installed app alongside a drift of tabs — would have
produced two independent cores, and every guarantee above would have evaporated
in exactly the case
[#97](https://github.com/JddAndrewLauren/hummingbird/issues/97) named as
arriving immediately.

**Had it proved false, this ADR would have been superseded by `navigator.locks`
leader election** (the first rejected alternative below), which carries no
platform assumption, and #102's outbound-queue single-writer-per-origin
guarantee would have had to be re-examined. That branch is closed.

The probe is **not** a counter posted from a throwaway page, which is what this
section originally proposed: a standalone window has no URL bar, so a
`/probe.html` is unreachable from inside one (and `vite-plugin-pwa`'s
`navigateFallback` would fight it anyway), and `PortRegistry.ports` is never
pruned, so a raw `ports.size` of 2 cannot tell two live views from one tab
opened twice. It ships instead as a **permanent diagnostic inside the app's own
`start_url`** — a core-instance id minted once per `SharedWorker` scope plus a
per-connect ordinal, both riding the `ready` handshake and rendered in Settings'
"Local core" card. Same instance id in two windows proves sharing; two different
ids refute it. Re-running it costs opening Settings in two windows, so a future
platform change is cheap to re-check.

### Scope

This ADR governs the **desktop web host**. Native clients embed the core
directly per ADR-0003 and the question does not arise there. `SharedWorker`
reached Chrome for Android only in version 148 (Firefox for Android 33, Safari
on iOS 16), so a mobile *web* client is possible but would rest on a far newer
baseline than the desktop one — re-check it rather than assume it if one is ever
wanted.

Support is not otherwise a constraint. The desktop floor is `SharedWorker`
itself — Chrome 5, Firefox 29, and Safari 16, which is when Safari returned it
after removing it in 7. Module workers raise the Chrome and Firefox floors to 80
and 114; MDN records Safari 15 for the constructor's `options.type`, but that
cannot precede the constructor, so 16 remains Safari's effective floor.
`client/web/vite.config.ts` already commits the project to ES module workers
(`worker.format: "es"`, required by `vite-plugin-wasm` and
`vite-plugin-top-level-await`).

### Consequences for the slices

- **S4 ([#102](https://github.com/JddAndrewLauren/hummingbird/issues/102))** —
  the outbound queue is single-writer by construction. It needs no
  cross-instance coherence guarantee and no test for one. Its crash-replay
  criterion stands unchanged, on its own merits.
- **S6/S7 ([#104](https://github.com/JddAndrewLauren/hummingbird/issues/104),
  [#105](https://github.com/JddAndrewLauren/hummingbird/issues/105))** — the
  seam gains an `onconnect` handler and a port list, and the ready handshake
  must be posted **per connecting port** rather than once at module evaluation,
  which is what `client/web/src/worker/announce.ts` does today.
- **S9 ([#107](https://github.com/JddAndrewLauren/hummingbird/issues/107))** —
  sync status is genuinely shared state, so every view shows the same cycle
  rather than its own.

### The top-level-await invariant (amendment, 2026-08-09)

Committing to ES module workers above has one consequence sharp enough to
state as a rule of its own, because it is invisible in review and fatal in
production: **no top-level `await` may enter
`client/web/src/worker/core.worker.ts`'s static import graph.**

`vite-plugin-top-level-await` rewrites such a module into an async IIFE. That
moves the `self.onconnect` assignment out of the module's first synchronous
turn — and a `connect` event has **no platform buffering**, so the connect
queued by the very view that *starts* the SharedWorker is delivered to
nothing and dropped. That view never gets a wired port and never gets a
handshake: it sits on "Loading core…" forever, while every other tab opened
afterwards works perfectly. This is why the wasm module is loaded with a
dynamic `import()` inside an async IIFE rather than a static top-level
import, and why a plain-looking `import` added to that file is a breaking
change.

It is enforced today by the file's own header comment and the source-text
pins in `client/web/src/worker/sync-timer-ownership.test.ts`; the only real
proof is the built bundle (zero `await` at function-depth ≤ 1 before
`self.onconnect =`), which four separate reviewers hand-checked during the
S6–S9 batch. If a mechanical check ever becomes cheap — a build-time assert
over `dist/assets/core.worker-*.js` — it belongs in CI, not in review.

## Rejected alternatives

- **`navigator.locks` leader election, with read-only followers.** The strongest
  alternative, and the fallback had the PWA assumption above failed — it did
  not (probed 2026-08-11, #172), so this stays rejected on its own merits
  below rather than waiting in reserve. Rejected
  because its guarantee is *behavioural*: the lock prevents a follower from
  draining only if the follower's code asks, while the follower still holds a
  complete in-memory mirror and a live handle on the same IndexedDB database —
  the resurrection path stays physically open and is closed only by discipline,
  spread across S4, S5, S6 and S7. That discipline also lands in the layer this
  repo does not test (`client/web/vitest.config.ts` is `environment: "node"` and
  covers no `.tsx` at all), so #97's "S4's tests can assert it" is not really
  satisfiable. Two further costs: every instance is a full core, so N tabs is N
  wasm modules and N copies of the mirror in memory; and a leader tab closing
  transfers leadership at an arbitrary moment, possibly mid-drain with a send in
  flight, making the crash-replay path a routine event rather than an edge case.
  Followers would in any case need a `BroadcastChannel` to see the leader's
  writes and to forward their own focus triggers and captures — which is most of
  what `SharedWorker` provides as a platform primitive, hand-rolled.
- **Last tab wins, argued safe.** CAS covers duplicate sends and nothing else.
  Given the single-key whole-envelope `put`, queue resurrection is a two-line
  proof rather than a speculation, and the double-rebase case silently replaces
  the user's edit. This is the one option whose cost is paid in lost captures.
- **The service worker hosts the core.** Genuinely one per origin and already
  shipping — but `client/web/vite.config.ts` sets `registerType: "autoUpdate"`,
  so Workbox may replace it mid-drain; idle service workers are terminated
  aggressively, which is hostile to a queue with exponential backoff capped at
  five minutes; and it couples the sync core to the offline app-shell's
  lifecycle.
- **Single-instance lockout** — the second window refuses to open a full app and
  says so. Simplest of all, and rejected on use: the operator leaves many tabs
  open by habit, and this punishes exactly that.
