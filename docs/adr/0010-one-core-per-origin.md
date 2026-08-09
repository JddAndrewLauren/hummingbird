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

### The assumption this rests on, and what reopens it

**Assumed:** an installed PWA standalone window shares a `SharedWorker` instance
with ordinary browser tabs on the same origin.

This could not be confirmed from Chrome or MDN documentation, and it was
deliberately committed to without a probe. It is load-bearing: if a standalone
PWA window is its own SharedWorker scope, then the intended configuration — the
installed app alongside a drift of tabs — produces two independent cores, and
every guarantee above evaporates in exactly the case
[#97](https://github.com/JddAndrewLauren/hummingbird/issues/97) named as
arriving immediately.

**If it proves false, this ADR is superseded by `navigator.locks` leader
election** (the first rejected alternative below), which carries no platform
assumption. The place it gets exercised for real is
**S7 ([#105](https://github.com/JddAndrewLauren/hummingbird/issues/105))**, when
the worker protocol is first wired against a real installed PWA; whoever builds
S7 owns checking it and reporting the verdict on this ADR either way. The cheap
check is a `SharedWorker` that increments a counter in `onconnect` and posts it
back: one tab plus the PWA window reaching 2 confirms the assumption; two
instances each reporting 1 refutes it.

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

## Rejected alternatives

- **`navigator.locks` leader election, with read-only followers.** The strongest
  alternative, and the fallback if the PWA assumption above fails. Rejected
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
