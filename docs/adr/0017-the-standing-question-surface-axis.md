# ADR-0017: The standing-question surface axis and probe semantics

**Status:** accepted · 2026-08-12 · **amended 2026-08-21 by
[ADR-0033](0033-the-status-surface-is-a-board-of-tiles.md):** decision 1's
rejected alternative — a fixed grid of health tiles — is adopted for the web
client, which now renders Status as a labelled tile board with single
selection and identity ordering rather than as a second `RankedRegion`. The
Android client's "quiet stack" is decision 1 as written, not an amendment.
· **amended 2026-08-23 by
[ADR-0034](0034-a-standing-question-can-be-switched-off.md):** a surface no
longer renders every question declared for it — a question switched off
emits no panes on its surface at all. The registry filter this ADR gives a
surface is unchanged; the toggle sits in front of it.
**Context:** the 2026-08-12 Status-screen grilling, opened on #310 as the
first slice of the batch that also carries #311 (the surface axis and the
filtered registry), #313 (`kimi-balance/v1`), #314 (`github-hummingbird/v1`),
#315 (`uptime/v1`), and #316 (authority reachability). Docs only — no code, no
schema change. Lands first so those five slices are reviewed against a
written decision rather than against this issue's own comment thread. Amends
[ADR-0015](0015-the-standing-question-read-contract.md).

`SCHEMA_VERSION` does **not** move. `context_snapshots` is additive — the four
new infra panes are registry entries in `server/domain/src/sources.rs`, not
columns, exactly as every prior source enrollment (`race-schedule/v1`,
`city-waste/v2`) added a row rather than a column.

## Decision 1 — Status is a second surface of ADR-0015's ranked region

A **Status screen** is not a new pane framework. It is the same ranked
region ADR-0015 designed for `NowScreen`'s aside, instantiated a second time.
`QuestionDef` gains a `surface: "now" | "status"` field declaring which
region a question renders into; the sort, the band vocabulary, the
`AnswerState` axis, the collapse rule and the write path (`Core::triage`)
are unchanged and shared by both surfaces.

**Why:** ADR-0015's collapse-when-dormant already produces exactly the
behaviour a status board wants — "all green is one quiet stack, red
announces itself" — for free, because a board *is* a ranked region where
most rows are expected to be uninteresting most of the time.

**Rejected: a bespoke fixed-grid `HealthTile` framework.** A grid of
always-visible tiles is the natural first instinct for a status board, but
it would have reinvented collapse-when-dormant badly: either every tile
stays visible all the time (the five-plus infra panes #314 anticipates
crowd out anything actually wrong) or the grid grows its own
show/hide rule, duplicating machinery ADR-0015 already built and tested.

**The cost, recorded honestly.** `AnswerState`'s three arms
(answered / bound-but-unacquired / unbound) and the five-band salience
vocabulary were justified in ADR-0015 by *cross-question ranking among
Now's four panes* — races, waste, weekend, vacation. Infra panes inherit
both axes whether or not they individually need that much expressiveness:
an uptime probe's `Shape::State` is naturally binary (matches its manifest
or it doesn't), yet it still answers on the same three-arm axis and sorts
into the same five bands as everything else on its surface. That is the
price of one shared contract instead of two, paid deliberately.

## Decision 2 — Question ≈ platform ≈ credential ≈ poller ≈ source string; pane ≈ one resource ≈ one `context_snapshots` row

A standing question's identity tracks the credential and the poller
binary that answers it, one `source` string per question, and a pane is
one resource inside that source's payload — one `context_snapshots` row.
Concretely: `github-hummingbird/v1` is one question with one poller and
one `GITHUB_TOKEN`-shaped credential, and it emits one pane *per scheduled
workflow* (#314) — many panes, one source, one question.

**Rejected: per-service source strings** (`github-hummingbird-status/v1`,
`github-hummingbird-city-waste/v1`, one per workflow watched). This is the
same unbounded, runtime-generated vocabulary ADR-0015 rejected for
`race-schedule:f1` and ADR-0014's frozen-namespace convention rejects
generally: it multiplies rows in a registry meant to stay frozen and
reviewable, for what is mechanically one feed polled once.

**One deliberate refinement: the credential-free prober breaks its own
rule.** `uptime/v1` (#315) spans both Cloudflare (the web origin, and the
authority per [ADR-0008](0008-the-authority-is-an-app-owned-server.md):35 —
a Worker plus a SQLite Durable Object) and Fly (the skill runner) under a
single source, even though those are two platforms.
Splitting it by platform would need two `ingest`-scoped
tokens minted for what is otherwise one credential-free binary issuing
plain HTTP requests — machinery bought for a distinction the poller itself
does not need to make. **Where a poller holds no credential, the credential
axis stops binding and the poller axis wins**: one binary, one source,
however many platforms it happens to point requests at.

*Amended 2026-09-02 (#775): a second, deliberate exception to this
decision's own identity chain.* The `poller` question answers a single
meta-question — is a source's newest `context_snapshots` row fresh against
the cadence that row itself declares — over **every** source
`server/domain/src/sources.rs` registers as a snapshot writer at once, so
its "pane ≈ one resource" half holds (one pane per source) but its "one
question, one source" half does not: `poller`'s identity is the *registry*,
not a single credential or poller binary. This is what frees
`github-hummingbird/v1` from double duty. Before #775 that source's pane
was this decision's worked example **and** the board's only signal that any
poller was alive at all, because it was the only pane reading a poller's
own freshness. `poller` takes over the second job for every registered
source, `github-hummingbird/v1` included — read straight off
`FreshnessFact::declared_cadence_ms`, never by parsing a workflow's run
history — which narrows `github.rs`'s own pane back to exactly what this
decision already said it was: one question, one source, one poller,
answering only whether this repo's own scheduled GitHub Actions workflows
last ran and succeeded. See `client/core/src/decisions/panes/poller.rs`'s
own module header for the mechanism, and #773 for where the
`github-hummingbird/v1` lane goes next (open PRs and issues across named
repos) now that it is free to.

## Decision 3 — Probe semantics: the right refusal is the signal

For a poller with no credential to prove reachability with (`uptime/v1`),
the signal is not a 200 — it is **the correct refusal**. An unauthenticated
request against an authenticated route returning **401 with an empty body**
proves, with no secret anywhere: DNS resolved, TLS terminated, the machine
booted, the process is listening, `/api/*` still beats the SPA fallback
ahead of it, and auth is switched on — because auth-before-dispatch makes
the refusal itself the evidence, before any credential is checked.

**The limit is stated plainly, not glossed over: reachable ≠ functional.** A
half-landed migration, a broken handler behind the auth check, a poisoned
cache — all still return a correct 401. This lane answers *"is the door
open and did the crons run,"* never *"is everything correct behind the
door."*

**Rejected: automating deep verification.** The existing hand-run
`smoke-prod.sh` does check functional correctness, and folding it into a
scheduled poller was considered and rejected — it would require a
write-everything `device` token to live inside an unattended monitor,
which is exactly the credential-blast-radius trade CLAUDE.md's
"credential blast radius" section already rules out for a much smaller
token (`RUNNER_BEARER_TOKEN`). Deep verification stays a human running a
script by hand.

## Decision 4 — Expected state is a committed, reviewable manifest

`uptime/v1`'s per-service expectation (`expected: "on" | "off"`) lives in a
committed file (`server/uptime-probe/services.json`), not in a database
row. Changing what a service is *supposed* to be doing is then a normal
reviewable PR diff — the same "editing the file is the whole override
gesture" posture CLAUDE.md already documents for `VERSION`. **Divergence
from the manifest's stated intent is the only thing that lifts a band**: a
service deliberately taken down — the runner suspended for a rebuild, say —
is a one-line flip to `off` in a reviewed PR, and its pane reads `dormant`
— quiet agreement — rather than sitting permanently red until it returns.

**`expected` has exactly one meaning: supposed to be answering HTTP at its
declared `url` right now.** For every service in this axis that is the
same fact as "supposed to be running", because reachability is precisely
the health proxy decision 3 chose — and that equivalence is the membership
test for the axis itself. A process for which the two facts come apart is
not an `uptime/v1` subject.

**The sweeper is that process, and it has no line in the manifest.** It is
live (since the 2026-08-12 go-live; #123 is closed — `docs/sweeper.md`),
yet it never opens a listener: its `fly.toml` deliberately carries no
`[http_service]` or `[[services]]`, because either would let Fly's
autostop machinery suspend a sweep mid-run. No truthful
`url`/`method`/`expect_status` triple can be written for it, and neither
`expected` value describes it — `"on"` paints a healthy service
permanently red, `"off"` claims a running service is supposed to be down.
Its liveness signal is healthchecks.io (`docs/sweeper.md`), which alerts
after three missed sweeps. The manifest #315 ships therefore carries three
services — authority, web origin, runner — all `expected: "on"`.

**Rejected: a sweeper line with `expected: "off"` reread as "never
HTTP-reachable".** It looks like a cheap tripwire on the no-listener
invariant, but the pane it produces reads quiet agreement forever —
including while the sweeper is dead — a second, weaker liveness signal
sitting alongside the healthchecks.io alarm that actually pages. A status
row that cannot go red when the thing it names breaks is worse than no
row.

**Rejected: a `settings` binding.** Whether a service is *supposed* to be
serving right now is a fact about the deployment, not a per-device
preference two devices of the same person could reasonably disagree about
— and `settings` has no DELETE, so an expectation that later becomes
irrelevant (a service retired, a migration finished) would accrete a key
nothing ever removes. A manifest reviewed in a PR has no such leak: a
retired service's line is deleted along with it.

## Decision 5 — Balance, not spend

`kimi-balance/v1` (#313) reports Moonshot's `available_balance` as a
**gauge** — "replaced wholesale each poll, never drained," exactly the
`context_snapshots` doctrine ADR-0009 already states for every snapshot
row — because Moonshot exposes no consumption endpoint at all. Spend is
only ever derivable as a delta between two polls, and the pane is
documented as reporting the balance directly rather than trying to infer a
burn rate from noisy deltas.

**This is also the better tile, and the ADR records why:** "$4.10 left,
~2 days" is a decision a reader can act on immediately; "$1.80/day" is
trivia that still requires the reader to do the runway arithmetic
themselves. The envelope's `schema` discriminator (`"kimi-balance/v1"`,
per ADR-0015's enveloped-payload convention) is left as the seam a later
history-carrying `kimi-balance/v2` would use if Moonshot ever ships a
consumption endpoint.

**Rejected: a new time-series table.** Recording every poll as a row
instead of overwriting the gauge would let a burn rate be computed
properly, but it breaks ADR-0009's "standing questions read the lanes,
they never add storage" — every other pane on both surfaces reads
`context_snapshots`/`alerts` as they already exist, and a table invented
for one pane's convenience is exactly the kind of bespoke storage that
rule exists to prevent.

## Decision 6 — Scale-to-zero inverts cadence

Of the two Fly apps, only the skill runner is probed at all — the sweeper
has no listener and no manifest line (decision 4) — and the runner scales
to zero: `runner/fly.toml:21` sets `min_machines_running = 0`. No machine
running is the runner's normal, healthy, idle state, and a 401 probe
against it **cold-boots the machine**. The authority holds
no such setting to invert: per ADR-0008:35 it is a Cloudflare Worker plus a
SQLite Durable Object, live on every request with no machine to sleep or
wake.

This inverts the intuitive polling frequency **for the whole probe
workflow**, not per service: probing every 15 minutes would cost the
runner 96 avoidable wake-ups a day to learn nothing new when nothing has
changed, so #315's workflow that carries every `uptime/v1` service —
authority, web origin, runner — runs **hourly as one unit**.

**Fly machine state is not a health signal, and this is the rejected
alternative recorded explicitly:** reading machine state via the Fly API
(is a machine running right now?) looks like the obvious healthcheck for
the runner, but under `min_machines_running = 0` it would read the runner
as *down* for most of every day — its correct idle state — and #306
already records `flyctl` itself misreading its own boot timing on top of
that. The 401-probe design in decision 3 is the one that survives
scale-to-zero correctly; platform machine state is actively wrong here.

## What this obliges

- **CLAUDE.md's map table** gains a **Status screen** row pointing at
  `client/web/src/screens/StatusScreen.tsx` (#311), with
  `questions/contract.ts` (where `surface` is declared) and this ADR as
  its read-first.
- **ADR-0015** gains an amendment-pointer entry in its Status header, per
  [the pointer convention](README.md): the region it designed is now
  instantiated per surface, and its placement rules ("the ranked region
  owns Now's Context aside," "one region, one slot") stay true *of Now*
  specifically.
- **#311** builds the surface axis, the filtered registry and
  `StatusScreen` itself against decision 1, and is blocked on this ADR
  landing first.
- **#313/#314/#315** each enrol one `server/domain/src/sources.rs` source
  against decisions 2 and, for #315, 3/4/6. #315's `services.json` ships
  authority, web origin and runner, all `expected: "on"` — no sweeper line
  (decision 4; its brief's `expected: "off"` predates the sweeper's
  2026-08-12 go-live).
- **#316** is pure client work answering decision 1's surface split with no
  new source, no new credential, and no schema change — the one pane only
  the device itself can answer.
