# hummingbird

Personal GTD-style task system. The task authority is moving from Linear
(org `twinion`, team `ION`) to an app-owned server (ADR-0008); the Linear
workspace stays the live working surface — and the skills below still target
it — until the owned stack is daily-usable. See `CONTEXT.md` for the domain
glossary.

## The sweeper

`sweep.py` is the one-way capture → Triage sweeper — one drain engine
over isolated adapters (Google Tasks fail-open, Gmail label-capture
fail-closed), the only built artifact of v0 capture. Stdlib-only Python,
one-shot, fired every 15 minutes by supercronic in a Fly worker. Its
invariants (one frozen namespace per source, create-in-authority-first
ordering, no Actions `schedule:`, no `[http_service]`) are load-bearing and
decided upstream; read `docs/sweeper.md` before touching any of them.
**Currently OFF (2026-08-08)** pending the authority move to the app-owned
server (ADR-0008); it retargets to `POST /api/items` when the owned stack is
daily-usable.

## The authority server

`server/` is the app-owned authority (ADR-0008/0009), its own Cargo
workspace: `domain` (the owned-schema types both sides will compile; the
client migrates onto them at S2/S3), `authority` (pure handler logic over a
sync `Sql` seam — plus an `Entropy` seam for token minting — fixture-tested
with rusqlite), `rules-engine` (fire-time evaluation of the ADR-0013
condition vocabulary, over the Event kind registry that lives in `domain`;
its `validate_rule` is wired into both `POST /api/rules` and
`PATCH /api/rules/:id` — a malformed condition is rejected at save with a
400, not just caught later at fire time; an unrecognized `event_kind` is
the one `RuleProblem` deliberately left unrejected there, since it is an
open registry key, not a closed vocabulary), and `worker` (the thin `workers-rs`
shim — one Worker, one SQLite-backed Durable Object). It carries the full
amended ADR-0009 schema plus the notification lane's
`rules`/`push_targets`/`deliveries` (14 tables,
`SCHEMA_VERSION 4`, ADR-0012/0013/0014/0015), entity-level CAS writes (absolute
sets + `expected_version`, 409 carries the current entity, creates
idempotent by client id), the all-tables delta pull with `GET /api/sweep`
as its byte-identical backstop, bearer-token auth (sha256 at rest; scopes
`device`/`sweeper`/`ingest`; `/api/admin/tokens` gated by `ADMIN_SECRET`;
401 = bad credential, 403 = wrong scope or — for an `ingest` token, which is
bound to one alert source — a source mismatch, all empty-bodied), the
`POST /api/alerts` ingest upsert, and `POST`/`DELETE /api/push_targets`
(idempotent registration — a replay adopts a rotated `fcm_token` and
revives a revoked target, since neither event mints a new device id — and
individual, idempotent revocation). `POST /api/snapshots` (#120) is the
server-polled lane's write side: `ingest` scope, version-blind upsert on
`(source, key)`, identity in the body because `handlers/mod.rs` splits the
path on `/` and every source string contains a slash. Its one subtle rule is
that **`fetched_at` is part of the value** — the no-write applies to *exact*
replays only (payload and stamp both identical), and `fetched_at` is
required rather than defaulted precisely so replay identity is decidable;
skipping the write when only the stamp moved would freeze what
`Freshness::of_snapshot` reads and make "poller fine, nothing changed"
indistinguishable from "poller dead", which is the discrimination this lane
exists to make. An older `fetched_at` never overwrites a newer one, and the
envelope is validated at the write (a stored row no pane can parse is a
source silently answering nothing) without ever resolving `schema` against
`REGISTRY` — ADR-0015 forbids that — and without requiring `schema ==
source`. `AlertIngest::restamp_on_change` (default false) is the same
slice's polled-source rule: a daily re-poll of an unchanged occurrence must
not restamp `raised_at`, since `is_live` compares it against `dismissed_at`,
while a correction must ring over that dismissal — and the poller cannot
decide which is which, because an ingest token cannot read the alert back,
so the handler restamps only on the raises that actually change a
source-owned field, with its own **write clock**. `GET /api/settings/:key`
is the one read a non-device scope reaches (`Device | Ingest`): a poller
needs the binding that says *what* to poll, and the alternative — the same
URL duplicated into an Actions secret — makes the binding editor decorative.
The widening is real (an ingest token can read any setting by name) and
bounded by `settings` being a small closed vocabulary of binding facts that
holds no credential, on a read-only route. The notification lane's delivery leg
(#139) is `hummingbird_authority::deliver`: a **sync** function, not an
HTTP route — the real FCM send is necessarily async (the `workers-rs`
shim's `fetch`, on wasm32, where a sync trait cannot block on a future), so
`deliver` only decides and logs the transitions-only dedupe against
`deliveries` (`UNIQUE(alert_id, rule_id, generation, severity)`,
ADR-0012/0014) and hands the caller back exactly which live `push_targets`
rows to send to; the delivery row commits before the caller can possibly
begin sending, so a crash or retry never double-rings, and zero live
targets suppresses without logging rather than permanently burning the
transition. `hummingbird_authority::sweep_tick` (#138) is that async
caller's synchronous half: the DO alarm's repeat-tick evaluation, at the
`ALARM_INTERVAL_MS` interval (15 minutes, a readable `const` rather than a
buried literal, so #140 can warn when a rule duration is shorter than it).
Every enabled rule against every non-archived item, presented as a
synthetic `item_threshold` event; **every matching rule for one item is
collected before any write** — one `upsert_alert` call at the highest
matched severity (never one call per rule, which would deliver a rule at a
pre-ratchet severity and then re-fire once the ratchet moved the dedupe
generation), through the exact same upsert `POST /api/alerts` uses
(`item-threshold/v1`, `source_key` = `item:<id>`, ADR-0014's state-source
convention — occurrence identity lives in the alert's lifecycle, not the
tick) — then `deliver` runs once per matching rule against that one
already-ratcheted alert. Before minting, the sweep reads whatever alert
already exists under `item:<id>` and asks whether it is still live right
now: **still live** passes `raised_at: None` (keep the stored stamp, so an
unchanged match is a no-op landing on the same `deliver` dedupe generation
and staying quiet); **not live** — no row yet, or the row is
dismissed/resolved/expired — stamps `raised_at` fresh, which is what lets a
hand-dismissed alert ring again once its item next matches (ADR-0014's
"Live: how a settled alert rings again"). The sweep never writes to `items`
or `rules` — a tick is read-then-mint, never a write to what it read. The
worker shim (`hummingbird-authority-worker`) supplies the DO's `alarm()`
handler: it drives `sweep_tick`, reschedules the next tick unconditionally
(even if the tick itself errored, so one bad tick can't stop the clock),
and makes the actual FCM HTTP send for each `DeliveryOutcome::Logged`
target — `sweep_tick` only decides and hands back what to send, exactly as
`deliver` does one layer down. That send leg (#219) keeps the same split:
everything decidable is pure and natively tested in
`authority/src/fcm.rs` (the OAuth assertion's bytes, the FCM v1 message
body — ADR-0012's two tiers mapping onto Android's transport priority *and*
channel id — how to read a response, and the one write a send may make),
while `worker/src/fcm.rs` holds only `crypto.subtle` (RS256 over the
service-account key) and `fetch`, because `server/worker` has no test
harness of any kind and anything expressed there is untested by
construction. The credential is the `FCM_SERVICE_ACCOUNT` **Worker
secret** — never a var, never in `wrangler.toml`, and unset (every
`wrangler dev`, and CI) the lane fails closed: rules still evaluate,
deliveries still log, and each unsendable one is a `console_error!`.
**Policy: no retry, ever.** `deliver` commits the claim row before the send
is attempted, so a retry that re-sent would be the double-ring ADR-0012
says destroys trust in the channel; a failure is logged and dropped. The
one exception is FCM's `UNREGISTERED` error code, which revokes that
`push_targets` row — and it is matched on the `FcmError` detail alone,
never on the 404 status, because a wrong `project_id` 404s identically and
would otherwise revoke every device the operator owns. ADR-0014's
**resolution pass** (#217) is the tick's second phase, and the reason the
lane can ever go quiet on its own: it iterates live `item-threshold/v1`
alerts — not items, since an item-side scan only reaches the alerts whose
items it still sees, precisely the set that does not need resolving — and
stamps `resolved_at` on each one the tick did not match. Phase one records
the `source_key` of everything it minted, phase two resolves exactly the
complement, so **the two phases partition the alert set** and neither can
write a row the other touched; that is what makes ADR-0014's four triggers
one test rather than four (done is skipped by phase one, archived is
excluded by `load_live_items`, deleted is absent from `items`, and no
longer matching yields no verdict). Only *live* alerts are considered, so
the pass is idempotent — a resolved alert is not live next tick, and its
stamp never creeps. Independently, **the sweep never clears
`resolved_at`**: it carries the stored value back through `upsert_alert`
rather than passing `None`, because that handler sets source-owned fields
*absolutely* and would otherwise erase the pass's stamp one tick later.
What supersedes a resolution is a later raise — `raised_at` overtakes the
stamp, the alert is live again and rings, and the stamp stays legible
underneath. Note that `done` is a *resolution* boundary only: #138's
evaluation boundary is still `archived_at` alone.

**ADR-0015's server half** is `SCHEMA_VERSION` 4, and it is the first growth
that is not purely a new table: `alerts.subject_key` is a nullable column on
an existing table, where `CREATE TABLE IF NOT EXISTS` is a silent no-op — the
column would never appear while `schema_version` marched to 4 regardless. So
`init_schema` gains `add_missing_columns`, a real `ALTER TABLE`, run *after*
the create loop (by then `alerts` exists whatever the store started as) and
gated on the column's actual presence read from `sqlite_master`, never on the
stored `schema_version` — a v1 store's freshly-created table is already
correct and gating on the version would try to add the column twice.
**`CREATE_ALERTS` declares `subject_key` last and inline on `version`'s own
line, and that formatting is load-bearing**: it is exactly the text SQLite
splices in for `ADD COLUMN`, and the growth tests assert a migrated store and
a fresh one hold byte-identical `sqlite_master.sql`, so reformatting it
breaks them on whitespace alone. The column is the pane join's server half —
`(source, subject_key)` ↔ `(source, key)`, *additive*, so an alert matching
no pane still lives in `AlertsScreen` — and it is source-owned, set
absolutely on every re-raise like `body`/`url`; `source_key` stays occurrence
identity and is never parsed for it, and `sweep_tick` deliberately writes no
`subject_key` at all, because an item is not a standing question and has no
pane. `hummingbird_domain::SnapshotEnvelope` is the other half: the common
`{ schema, polled_every_ms, body }` every `context_snapshots.payload` now
carries, parsed shallowly — `body` is carried through opaque, an unrecognised
`schema` is passed through untouched (it must never grow a `REGISTRY` check),
an absent `polled_every_ms` is a legitimate state, and everything else is a
typed `EnvelopeProblem` naming *what* was wrong, never a quietly empty
answer. The panes, the client-side `Freshness` carve-out and the
`city-waste/v2` + race source-registry entries are each their own slice.

**Deployed in production since 2026-08-10** (#237): `hummingbird-authority`
on Cloudflare Workers, answering `hb.twinion.net/api/*`, with `ADMIN_SECRET`
set from the operator's terminal and the first `device`-scope token minted.
`#95`'s human gate H3 is closed, and so is the rest of #234's map — the
`Authority` SQLite DO was created by the first deploy's `tag = "v1"`
migration, verified against the API (`use_sqlite: true`, `migration_tag:
v1`) because **the deploy output never mentions migrations at all**, and
neither does `wrangler deployments list` or `versions view`. Local
development is unchanged: `wrangler dev` + `server/scripts/smoke.sh`.
`server/scripts/smoke-prod.sh` is its production counterpart (#239/#240) and
is a **deploy-integrity verifier, not a second acceptance suite**: the shared
test recipe below already proves handler logic, CAS, the scope matrix and
sweep/delta agreement against a real SQLite DO before a deploy can land, so
the prod script asserts only what CI structurally cannot see — that `/api/*`
still beats the shell's SPA fallback (asserted on `content-type`, not just
status, because the root genuinely answers `200 text/html`), that auth is on,
that all ten sweep lanes are present so the live schema is the built one, and
that the version cursor is live. It is **read-only, manual, and carries the
operator's own reused `device` token**, each deliberately: there is no delete
route anywhere in the API (ADR-0003), so a write could only ever have been
tombstoned, not cleaned up; and `device` is the only read-capable scope, so a
read-only script still holds a write-everything credential — which is exactly
why it never goes into Actions, the same posture `ADMIN_SECRET` gets.
**"Read-only" means zero writes to synced data, not zero writes**, and the
distinction is worth holding: `auth::authenticate` ends every successful
authentication with `UPDATE tokens SET last_seen`, so each run stamps its own
token row four times. That is outside the delta contract by construction —
`tokens` never syncs to any client, and the update takes no meta bump on
purpose, since bumping there would make every authed read dirty the cursor.
Automating it requires minting a dedicated token *first*. The standing
consequence: **nothing automated ever exercises a production write**, so a
deploy that broke writes while leaving reads intact passes it green; the only
write proof is #241's hand-run foreign-device round-trip, and #234 carries the
unfired conditional that would close it. Unlike the local smoke it also races
real traffic, so its byte-agreement and cursor assertions retry once and
distinguish a concurrent capture (a higher `version`) from a genuine
disagreement (a difference at the same one). **The test recipe is shared,
not copied** (#229): `server-test.yml` is a `workflow_call`-only workflow
holding clippy / native fixture tests / wasm32 build / `smoke.sh`, and both
`server.yml` (pull requests only) and `deploy-server.yml` (`main`, `wrangler
deploy` behind `needs: test`) call it, so the gate a PR sees and the gate the
deploy passes cannot drift. `server.yml` carries no `push:` trigger for
exactly that reason — `main` is gated by the deploy workflow's own test job,
and a second copy would run the same 30-minute job concurrently for no extra
signal. `deploy-server.yml` carries no `schedule:` — the
DO's own `alarm()` owns the cadence — and no `ADMIN_SECRET`/
`FCM_SERVICE_ACCOUNT`, which stay out of Actions deliberately: the first is
the credential that mints every other token, the second carries an RSA
private key. Both deploy workflows also carry a `workflow_dispatch` guarded
with `if: github.ref == 'refs/heads/main'`, because a Cloudflare-side change
(binding a domain, setting a secret) touches no file here and would
otherwise have no trigger at all.

## The which-cans poller

`server/city-waste/` is the out-of-process adapter behind #120's standing
question: once a day it reads the council's collection page for one address,
writes a `context_snapshots` row, and — only on a week where the collection
moved — raises one alert. It is a workspace member, so CI gates it; it must
**never** become a dependency of `hummingbird-authority-worker`, whose build
is wasm32 and has no business carrying an HTTP client, a tzdb or an HTML
parser. Not in the DO's `alarm()` for the reason that split
`authority/src/fcm.rs` from `worker/src/fcm.rs`: `server/worker` has no test
harness, so anything expressed there is untested by construction. The same
split runs one level out here — everything decidable is in the lib and
natively tested, and `main.rs` holds only `std::env`, one GET and two POSTs.

**Materiality is deviation from cadence, never a diff against the previous
poll.** `judge` never sees the last snapshot, and that one choice is what
makes the lane behave: the ordinary roll-forward is silent (the morning after
a pickup the page jumps a whole week — a *large* diff and a *zero* deviation),
the poller is correct on its first run and again after a wiped snapshot, and
it needs no state of its own, which is what lets it be a one-shot cron
process. The price is that the body must carry the cadence: without it there
is nothing to deviate *from*. `scheduled` is **derived**
(`cadence.latest_on_or_before(collected_on)`) and `collected_on` is
**observed**, which is what makes `collected_on !== scheduled` a derivable
holiday reading rather than something the council has to say out loud; there
is deliberately no `deviation` field, because a judgement duplicated into a
payload is a fact that can disagree with itself. `Deviation::SkippedCycle`
survives even though the corrected domain has no cancelled week — if it fires
it is a real cancellation or a parse failure, and both deserve to be loud —
and the backward-slide guard the prototype flagged is `MAX_SLIDE_DAYS` plus
nearest-cadence-date resolution. `date.rs` is dependency-free integer
arithmetic on day numbers except for one delegated question: when a
collection day ends *at the address*, which needs a tzdb (`jiff`) and cannot
be derived from a day number; an unknown zone is `None`, never a silent UTC.

**`alert::plan` takes no clock, and that is load-bearing rather than tidy.**
Its return becomes the alert's `title`/`body`, and the authority decides
`restamp_on_change` by diffing a re-raise against the stored row — so
anything clock-dependent in those strings makes *every* daily re-poll of an
unchanged slide a change, which restamps `raised_at`, which (since `is_live`
compares it against `dismissed_at`) undoes the reader's dismissal every
morning. That is the precise failure the whole design exists to prevent, and
it arrives through the most innocuous field there is: a relative phrase in a
title ("in 4 days", "tomorrow"). Both the prototype and this slice's first
revision wrote one. Dropping the `today` parameter is what makes it
unwritable rather than merely absent — **do not add one back**; how far away
the collection is is read-time urgency, computed on the pane like every
other read-time fact (ADR-0002), never written into a stored row.
`a_week_of_re_polls_of_one_unchanged_slide_is_byte_identical_every_day`
compares the whole serialized payload, not the identity, because "the same
`source_key` every day" is exactly the assertion that could not see it. For
the same reason `Date::today_in_zone` exists beside `end_of_day_ms`: "today"
resolved as `now_ms / 86_400_000` is the *runner's* UTC day, which agrees
with the address at the 06:40-local cron and disagrees on a manual dispatch
in the local evening.

`tests/contract.rs` is the only guard against the body drifting from
`waste.ts`, and it exists because **nothing mechanical connects the two
sides** — the body inside ADR-0015's envelope is opaque to the server by
design, so a rename on either side compiles and passes on both. It asserts
the literal snake_case keys against the TypeScript's own text.

`page.rs` is the module written against a saved sample rather than a
specification, and the sample settled the question that gated it: the
council's page **states the standing collection day outright** ("Weekly /
Monday / 08/10/2026", per stream), so the cadence is observed and
`city-waste-page` stays a bare URL — the feared widening into a JSON object
reaching into the client never happened. Three stream columns collapse into
the domain's one collection: `collected_on` is the **earliest** date any
column advertises and `streams` is the set sharing it (so the week the
biweekly bin stays in answers with the smaller set — the which-cans question
itself), `every_n_weeks` is the **shortest** period across columns, and the
anchor is `collected_on` snapped to the nearest *stated* day, which is what
keeps a holiday week off the lattice where `judge` can see it. **The one
assumption still unconfirmed** is that the stated day is standing rather than
per-cycle: if the page instead prints "Tuesday" on the week it slides, the
anchor moves with it and the holiday reads as an ordinary week — a *quiet*
failure, unfixable from the page alone (telling "moved this week" from "moved
permanently" needs the previous snapshot, which `judge` deliberately never
sees), so it is recorded in the module header to be checked against the first
real holiday. There is deliberately **no HTML-parser dependency**: the fields
hang off ids the page names itself (`trash-date` / `recycle-date` /
`organics-date`), never the Visualforce-generated `j_id0:*` ones around them,
and every absent marker is a named `PageError::Missing` so a redesigned page
fails loudly on the first poll instead of writing something plausible. The
fixtures under `tests/fixtures/` are **reduced and sanitised** — the `<main>`
region verbatim, with the operator's home address replaced and ~95 KB of
remoting bootstrap (per-request CSRF tokens, signed JWTs) dropped, since this
repo is public; only the ordinary-week one is a real observation, the
off-week and holiday ones move dates on that capture and say so in their own
header comments.

`.github/workflows/city-waste.yml` is the repo's **one Actions `schedule:`**,
and it is a scoped exception rather than a drift. #8's overturn had four
clauses and three were about a *private* repo's Actions billing (pooled
minutes, whole-minute rounding, the $0 cap); hummingbird is public, so only
the 60-day auto-disable survives in general — and this lane is
self-monitoring, since the pane bands its own answer stale at 26h and then
refuses to answer, so a stalled poller is loud within a day. The ban still
holds absolutely where it was really about *competing clocks*: supercronic
owns the sweeper's cadence and the DO's `alarm()` owns the sweep tick, and a
second cron for either would compete with a live one. This poller has no
competing clock at all. `CITY_WASTE_INGEST_TOKEN` goes in Actions secrets on
the blast-radius reasoning that keeps `ADMIN_SECRET` out: that one mints
every other token, this one reaches three routes for one source and its
worst-case abuse is a wrong bin day. `polled_every_ms` in `body.rs` must
match the cron.

**Not covered, and not this slice's to fix:** `POST /api/alerts` does not
trigger delivery. `deliver` runs only from `sweep_tick`, which evaluates
`item-threshold/v1` — so "the notification lane still delivers it" is
aspirational today for **every** webhook source, not just this one.

## The client sync engine

`client/core/src/sync/` is the device half of the owned stack (ADR-0008), and
the largest thing in `client/`: `adapter`/`transport` are the read side (the
normal pull is the delta since the mirror's own version; `GET /api/sweep` is
the correctness backstop, on app open and daily), `write/` is its mirror
image on the write side (CAS mutations, rebase-on-409, deterministic ids,
the error taxonomy), `mirror` is the local read model where absence demotes
rather than deletes (ADR-0003), `queue` is the durable FIFO plus its
dead-letter journal, and `cycle` is ADR-0007's one cycle — drain, then pull,
in that order, every time, with jittered backoff capped at five minutes.
**Durability belongs to the cycle, not the queue**: capture code calls
`SyncCycle::enqueue`/`run`, never `OutboundQueue::enqueue`/`drain` directly,
because only the cycle pairs each mutation with the snapshot write that
makes it durable before anything is sent. Clock, jitter and access token are
caller-injected on every call — bare `wasm32-unknown-unknown` has no clock or
RNG that does not panic. There is no `docs/sync.md`; the map is the module
docs in `client/core/src/sync/mod.rs` and each submodule's own header.

**A 409 is a three-way decision, not two** (#163/#164). `write/rebase.rs`'s
`decide` diffs every touched field against both the client's `base` and the
409's `current`: a field that moved to some third value is a `Collision`,
naming every colliding field; a patch whose touched fields *all* already
hold their intended value is `RebaseDecision::Achieved` — this write in fact
already landed (a crash swallowed the ack), so it is **not resent at all**,
the 409's carried entity becomes the outcome, and the authority never sees
the replay; only `Safe` — at least one field still needing reapplying,
nothing colliding — reissues the identical touched-field intent at the new
version. `write/adapter.rs`'s `patch_with_rebase` is a bounded attempt loop
(`MAX_ATTEMPTS = 3`: the original send, the retry a first `Safe` 409 earns,
and one further retry for a `Safe` second), each attempt diffed against the
`current` it was actually rebased onto rather than the original `base`. A
third attempt that is *still* disjoint is repeated churn, not a collision,
and terminates as `WriteError::Contention { current }` → the queue's
`DeadLetterReason::Contention` — carrying the server entity so the journal
still has material, because the alternative is a `Conflict` with an empty
field list masquerading as a real one, showing the reader nothing to act on.

`Core` (`client/core/src/lib.rs`) is the one door onto all of that, and it
has exactly **four mutation entry points**: `Core::capture` (a create,
whose `title` goes through `capture::parse_seam` — #110/#42's named no-op —
and reaches the mutation verbatim regardless), `Core::act` (S11's closed
`ItemAction` vocabulary: start / complete / block / cancel, where cancel
sets `archived_at` and never a stage, because the owned schema has no
"canceled"), and `Core::triage` (S13's `TriageDestination` + `TriagePatch`:
a multi-field triage is exactly ONE queued CAS `PATCH`, never one per
field, so a 409 rebases or dead-letters the whole edit together), and
`Core::set_binding` (#118's standing-question bindings — one `settings` row,
written as an ordinary absolute-value CAS `PUT`). **All four enqueue through
`SyncCycle::enqueue` and none of them may reach `OutboundQueue::enqueue`** —
the durability rule above is not per-caller advice, it is what makes an
offline capture, act, triage or binding survive at all. All four take a
caller-minted `seed` (deterministic id, same no-clock/no-RNG
reasoning). The reads are `frontier` / `triage_inbox` / `blocked` /
`steps_for` / `projects` / `bindings`; the three item queries are each a
filter over one shared `overlaid_items` view, while `steps_for` and
`projects` (over `SyncMirror::all_projects`) read the mirror directly — no
mutation entry point mints a Step or a Project, so there is nothing
optimistic to overlay there.

**Bindings are `settings` rows and nothing more** (#118, ADR-0015).
`bindings.rs` holds the closed, kebab-case, **unversioned** key vocabulary —
`race-series` / `trips-calendar` / `city-waste-page` — resolved by name at
the seam so no caller can mint a key into a table that has no DELETE, and
unversioned so a `city-waste/v1 → /v2` source bump cannot orphan one. They
ride the ordinary delta pull and full sweep (`SyncMirror::all_settings`),
with no bespoke path on either side; `Core::bindings` lists every known key
set or not, then every live row this build cannot write, each flagged
`known` — an unrecognised key is displayed, never hidden, the same reading
ADR-0015 gives an unrecognised snapshot `schema`. A value is
`Unset | Text | Other`, not a nullable string, for `Freshness`'s own reason:
"nobody set this" and "this holds something that is not text" are different
facts. The one place bindings are not like every other write is the
encoding: `PutSetting::value` is typed JSON while `Setting::value` stores its
canonical *text*, so `MutationIntent::Patch` carries `rebase_fields` — the
same intent in the entity's encoding — and `patch_with_rebase` diffs a 409
against that. Without it this client's own already-landed write reads as a
collision with itself and dead-letters a `PUT` that in fact succeeded.
The mirror-image hazard is a **success** that is not one: a `PUT` at
`expected_version: 0` against a key that already exists never 409s — the
authority answers `200` with the *stored* row — so `patch_with_rebase` asks
`rebase::divergent_fields` whether that row actually carries this write's
intent, and reports a `Conflict` when it does not. Right for a true replay,
and the difference between a dead-letter and silent loss for a device that
had simply never pulled the row (a binding edited before its first sync).

**The overlay is one representation, not one per mutation kind.**
`overlay_from_queue` rebuilds it at `Core::init` from whatever the durable
queue still holds — `MutationIntent::Create` through `item_from_create`,
`MutationIntent::Patch` through `apply_item_patch` (the same absolute-value
field overwrite the wire sends) — so a capture, an act or a triage made
offline and then reloaded is still readable, still `is_pending`, rather
than vanishing until the next successful cycle. `binding_overlay_from_queue`
is its exact twin for `settings` (a separate map: different key space,
different shape, identical lifecycle in `Core::run` — dead-letter reverts,
completed cycle clears). A queue entry that no
longer projects is an `Err`, never a silently dropped overlay entry: going
overlay-blind would tell a reader nothing is pending while something still
is. It is keyed one entry per item id in FIFO order (last enqueued wins),
which leaves the narrow `entry_id` gap `Core::act`'s own doc records —
flagged there, not fixed.

`client/core/src/rank.rs` is the other top-level module beside `Core`, and
it is no part of the sync engine: `rank()` (#162) is
`/next-up-personal`'s six ranking steps made pure, so a device can pick
"what to do right now" offline — context hard-filter, overdue / due-today,
In Progress bias, priority, energy/size fit plus the 30-minute calendar
nudge, then oldest-first. **No I/O, no credentials, no clock read**: "now"
arrives as the caller-injected `Now`, carried in two shapes (naive-local, in
exactly `Item::deadline`'s own spelling, and the same instant as epoch ms)
because deriving one from the other would need a time zone this crate has no
business guessing. It speaks only `hummingbird_domain::Item` and
`calendar::EventRecord` — no Linear vocabulary and deliberately no
translation layer onto either — and consumes `calendar::query`'s
`is_actionable`, so a cancelled future instance never fires the nudge, the
same invariant `query.rs` documents for "Next". Every candidate carries
every `ReasonCode` that applied, in step order, so #116's skill layer can
cite the actual decisive rule rather than a step index. The total order ends
`created_at` then `id`, with nothing left to chance, and
`the_same_snapshot_ranked_twice_is_byte_identical` is what pins that a
repeat call is byte-identical. Nothing consumes it yet — it crosses no FFI
seam; #116 is its caller.

`client/core/src/freshness.rs` is the third top-level module, and ADR-0015's
Rust half of the Rust/TS carve-out: `Freshness` is **not a boolean** but
`Unknown | Age { age_ms, declared_cadence_ms: Option }`, because two
different unknowns exist — `Unknown` is *we do not know the age*, `Age` with
no cadence is *we know the age but not what normal looks like*. The
invariant it exists to make unbreakable is that **`Unknown` may never render
as fresh**: `age_ms()` returns an `Option` rather than a zero, and
`is_stale_beyond(threshold)` — the one stale decision here — answers `true`
for `Unknown` against every threshold, including `i64::MAX`. The **clock
rule is stated once** in `measure`: both stamps are read against the
device's clock, so a fetch stamp in the future means the clocks disagree,
and age clamps to `0`; two prototypes independently hand-rolled
`Math.max(0, now - fetchedAt)`, which is the drift the carve-out exists to
stop, so no other caller in Rust or TS may repeat it. **The threshold is
deliberately not here** — it stays in TS beside each pane's band function,
because the driver is the cost of a wrong answer (waste calls 26h stale
where `2 × cadence` would say 48h); a `stale_after_ms` field on this type is
the rejected design. One type serves all four panes: `of_snapshot` takes the
cadence from the row's `SnapshotEnvelope` (a broken envelope costs the
cadence, not the age — the pane reads the `EnvelopeProblem` from its own
parse), while the calendar lane passes ADR-0005's poll interval into
`measure` directly. **The finished value is what crosses the seam**, not its
parts (`Core::snapshot_freshness` → `task_host.rs`'s `FreshnessResponse`,
`{"state":"unknown"}` or `{"state":"age",…}`): handing `fetched_at` over for
TS to combine would put the subtraction back on the far side of the
boundary, and the shim's busy answer is `unknown` for the same reason — a
core that has not loaded has measured nothing.

`client/core/src/pane.rs` is the fourth, and the generic read every standing
question's pane starts from (#245): `Core::pane_read(source, now_ms)` — one
source's live `context_snapshots` rows (key order, envelope parsed, age
measured per row) and the alerts it has raised that are **live right now**.
Per-source, `&self`, no `Result`, no overlay (the context lanes are
server-written, so nothing optimistic exists to overlay). The two things
ADR-0015 carves into Rust so they cannot drift are applied here and only
here — the age clamp and ADR-0014's `is_live` — while answer state, band,
headline and threshold all stay in TS. `PaneEnvelope` is
`Parsed`-or-`Malformed`-with-a-reason (`EnvelopeProblem`'s own wording), a
broken envelope costs the cadence but not the age (one parse feeds both, so
it cannot drift from `Freshness::of_snapshot` — pinned by test), an
unrecognised `schema` rides through untouched with **no registry check
ever**, and `subject_key` rides through untouched too, because the
`(source, subject_key)` ↔ `(source, key)` join is additive and the pane owns
it, in TS. `ffi-web`'s `PaneReadResponse` / `paneRead` is the seam, its wire
shape pinned byte-for-byte; `BUSY_PANE_READ` matters more than most busy
answers, since an empty pane read renders as "nothing is due" — a claim a
core that has not loaded may not make, so the host drops it.

## The web worker layer

`client/web/src/worker/` is where the device half meets the browser: **one
`SharedWorker` per origin, N views** (ADR-0010, #126). `core.worker.ts` is
the shim only — it loads the wasm core with a dynamic `import()`, wires
`ports.ts`'s `PortRegistry` (the port list, the per-port `ready`/`error`
handshake, the broadcast fan-out; a port that connects before the core
exists is queued, never dropped), and owns **ADR-0007's single 60-second
interval for the whole origin**, because a timer in a per-view hook
multiplies with tab count and blows the ADR's ~60 req/hr budget. Everything
decidable is a sibling pure module a vitest (node) test can execute:
`dispatch.ts` (cadence / task / calendar routing, plus the app-open sweep,
which fires on the first `pushTaskApiKey` — never at activation, when no
credential is known yet), `request-router.ts`, `task-worker.ts` and
`calendar-worker.ts` (the two wasm hosts' own serial queues and JSON
parsing), `visibility-tracker.ts` (a `SharedWorker` has no `document`, so
each view reports its own `document.hidden`). The view side is
`store/protocol.ts` (the whole wire contract, push-only worker→view),
`store/worker-client.ts` (the only translation of that protocol into store
writes), `shell/useSyncWiring.ts` and `shell/useTaskTokenWiring.ts` (#106's
device token: entry, rest, re-prompt — the key crosses into the core and is
never read back out through any response), and `shell/sync-status.ts` (the
staleness readout; an outcome that did not run must never read as success).

**That one interval now runs behind an in-flight guard, and trigger identity
survives it.** `worker/sync-run-guard.ts` (#184) wraps the `run` sink
`core.worker.ts` hands `createSyncCadence` — not the cadence, and not the
serial queue, whose own abandon-on-timeout is an unrelated fix that must
keep working — so at most one `runSync` is in flight for the whole origin.
Triggers arriving during a run coalesce into exactly ONE pending follow-up,
which starts the instant the in-flight one resolves; and because a
`runSync` whose promise never settles would otherwise wedge the cadence
forever, the guard releases the slot after its own bound, `releaseMs`,
passed in as `TASK_REQUEST_TIMEOUT_MS` — the same "how long is too long"
the underlying task queue already answers, never a second independent
number. A generation counter is what makes that release safe: a straggler's
late `.finally`, arriving after the bound already handed the slot to a new
run, is stale and can never free that *other* run's slot. Which of two
triggers survives in the single pending slot is
`shell/sync-cadence.ts`'s `mergePendingSyncTrigger`, never bare last-wins,
because identity is read downstream of the guard (`forceFullSweep`,
`toCoreTrigger`): `open` (3) > `reconnect` | `manual` (2) > `focus` (1) >
`timer` (0). A pending `open` overwritten by a later trigger would drop
ADR-0008's app-open full-sweep backstop for the rest of the worker's
lifetime (`dispatch.ts` fires it once), and a pending `reconnect`/`manual`
overwritten by a `focus` or `timer` would silently demote a user-facing
backoff reset — the outage-recovery path the precedence exists for. **The
guard module imports nothing, and must stay that way**: it sits in
`core.worker.ts`'s static import graph, which may never acquire a top-level
`await`.

`toCoreTrigger` maps `"focus"` onto the core's `"timer"` spelling, not
`"user"` (#190). A focus event says a window came forward — an ambient
signal, not the gesture ADR-0007's "backoff is reset by any user-facing
trigger" is about; only `open` / `reconnect` / `manual` are, and they still
reset it. The cadence itself is unchanged and `useSyncWiring.ts` still
forwards every focus, but outside backoff a focus behaves exactly as before
and during backoff it lands as a cycle the core declines, so alt-tabbing at
any rate can no longer stretch an outage's request rate past the backoff
schedule. An interpretation of ADR-0007 recorded in the code, not an
amendment to the ADR.

The protocol now carries the whole read-and-act surface: view→worker
`capture` / `act` / `triage` / `setBinding` and the reads `getFrontier` /
`getTriageInbox` / `getBlocked` / `getSteps` / `getProjects` /
`getBindings` / `getPaneRead` / `isPending`; worker→view `captureResult` /
`actResult` / `triageResult` / `setBindingResult` plus the `frontier` /
`triageInbox` / `blocked` / `steps` / `projects` / `bindings` / `paneRead` /
`isPendingResult` pushes. `getPaneRead` carries its own `nowMs` — the clock
both the measured ages and the alert-liveness filter are resolved against,
core-side — which is also why `paneRead` is *not* one of the messages
`ports.ts` replays to a late-connecting port: a replay would state a stale
age as a current fact. The calendar lane's `getCurrentNext`/`currentNext`
are **gone** with the context tile ADR-0015 replaced, and
`useCalendarWiring.ts` lost its 30-second clock with them (`useSyncWiring`'s
existing unconditional tick is the one clock Now gets); the connect flow,
the silent re-mint, the rotation and the 15-minute poll all survive
unchanged. A frontier or blocked entry is a
`FrontierItemDTO` — `ffi-web/src/task_host.rs` flattens
`hummingbird_domain::Item` and adds the computed `pending` flag, stamped in
exactly one place (`TaskHostCore::with_pending`, applied by `frontier()`,
`triage_inbox()` and `blocked()`) so the answer cannot drift between call
sites; `pending` is a read-time fact about the overlay, never a schema
column. Every wire string is resolved by name before the seam
(`parse_action`, `parse_destination`, `Stage::parse`, `Size::parse`,
`Energy::parse`), so an unrecognised name fails without ever reaching
`Core`. On an `ok` result `store/worker-client.ts` re-requests the affected
queries itself — `actResult` re-reads frontier, blocked and that item's
`isPending`; `triageResult` re-reads the triage inbox and the frontier;
`setBindingResult` re-reads the bindings —
which is what makes a mutation taken offline visible immediately, without
waiting for a cycle.

**The invariant: no top-level `await` may enter `core.worker.ts`'s static
import graph.** `vite-plugin-top-level-await` wraps such a module in an
async IIFE, which pushes the `self.onconnect` assignment past the first
turn — and a `connect` event has no platform buffering, so the connect
queued by the very view that STARTS the SharedWorker is silently dropped and
that view hangs on "Loading core…" forever. This is why the wasm module is
imported dynamically inside an async IIFE. Adding a static import to that
file, or a top-level `await` to anything it imports, re-breaks it;
`worker/sync-timer-ownership.test.ts` pins what it can from the source text,
but the real proof is the built bundle (zero `await` at function-depth ≤ 1
before `self.onconnect =`).

## The screens and shell layer

`client/web/src/screens/` and `client/web/src/shell/` are the read-and-act
surface S10–S13 built over that protocol — Now renders the frontier, its
project groups and item detail; Triage renders the inbox as **one collapsed
line per capture** (badge, title, provenance, age — `screens/TriageRow.tsx`),
expanding the one row that is *selected* into the full editor. One row open at
a time, because expanding is a selection and two open editors would put two
sets of unsent drafts on screen with nothing to say which is being worked. **Capture is shell chrome, not a screen**: `screens/CaptureBox.tsx`
lives inside `shell/CapturePopover.tsx`, opened over whatever is showing by
the header's **New** button (labelled for what the person is doing; capture
stays the verb everywhere internal — the field's label, the `feather` glyph,
the wire message, `Core::capture`) or the same global "c" hotkey, which no
longer navigates to Triage. Exactly one box exists at a time, which is what
keeps `CAPTURE_INPUT_ID` honest as a document-wide id. It offers the two
stages a capture may be *born* into (`screens/capture-destination.ts`):
**Add to Triage**, still the default and what Enter sends, and **Mint
action** — CONTEXT.md's Mint, landing in Ready — which is one ordinary
`Core::capture` at that stage, never a capture followed by a triage, so
skipping triage is still one queued mutation with nothing to rebase between
two halves of one gesture; `submitCaptureRequest` re-reads the frontier as
well as the inbox for it, since that is where a minted item actually lands.
The popover does not close on submit (capturing several things in one sitting
is the normal case) and reports what each submit did, because it covers the
screen that would otherwise have shown the item arriving. **Everything decidable is a pure `screens/*.ts` module a vitest
(node) test can execute, and the `.tsx` components only thread React state
through them**, the same split `worker/*` already uses: `frontier-order.ts`
(priority rank, then deadline, then id) over `priority.ts` (Linear's
inverted, holed 0..4 encoding survives in the schema, so nothing sorts or
renders the raw number — ADR-0002 leaves ranking to consumers),
`frontier-groups.ts` (group by `projectId`, names resolved from the
`getProjects` answer, unassigned last), `urgency.ts` (CONTEXT.md's
read-time urgency, computed fresh per render and never written back onto a
`TaskItemDTO`; `deadlineSortKey` is the TS twin of the domain crate's, and
a day-only deadline resolves to `T23:59` local wall clock, never a UTC
instant), `blocked-reason.ts`, `capture-validation.ts` (an empty or
whitespace-only capture is refused here, because `Core::capture` has no
opinion of its own and would enqueue it), `triage-order.ts` (capture order,
by `createdAt`, which reads the same before and after the overlay clears),
`item-actions.ts` (which actions a stage offers, the optimistic
`applyItemAction` projection, and `resolveFallbackPending` for an item that
has left every live query) `triage-form.ts` and `bindings.ts` (#118's editor: the human copy per
binding, the three value states read apart, which drafts are worth
sending — an empty one, a no-op one and any key this build cannot write are
all refused here, because `Core::set_binding` has no opinion of its own and
`settings` has no DELETE to undo a blanked row — plus `sameBindingValue`,
which is what lets a row reseed its field when the value underneath it
moves, so a pull carrying another device's edit can never leave a stale
draft sitting over it with Save enabled to push it back, and
`bindingWriteError`, so a failed write is words on that row rather than a
`lastBindingWrite` nothing reads).

**The triage editor edits every field of an item but its source**, and the
thing that made that possible is that its draft is **seeded from the item**
rather than blank (`screens/triage-form.ts`). The old form's `""` meant
"unchanged", which can only ever *add* a value; a draft showing what the row
actually holds turns every field into a diff, so `buildTriageEdits` can send
the three instructions `TriageEdits` (`store/protocol.ts`) carries all the way
down to `hummingbird_domain::ItemPatch`: **an absent key leaves a field alone,
an explicit `null` clears it, a value sets it**. `TriagePatch` (client core)
and `TriageEdits` (`ffi-web`) are double-`Option`/nullable for exactly that,
and the wasm seam takes the edits as **one JSON string** rather than positional
`Option<String>` arguments, which cannot express absent-vs-null at all. Only
what someone has typed is React state (`effectiveDraft`): every other field is
derived per render, which makes the stale-draft hazard `bindings.ts`'s
`sameBindingValue` patches *structurally* absent here — a pull that moves a
field nobody is editing shows through, because it was never captured to go
stale. `title` and `priority` are `NOT NULL`, so they have no clear: a blanked
title is a `triageDraftProblems` message, not an edit, and "No priority" is the
real value `0`. Every rule the authority answers 400 on (empty title, priority
range, `is_valid_deadline`, a day-only scheduled date) is checked in three
places on purpose — `triageDraftProblems` so the message lands on the field
while someone types (`urgency.ts`'s `isValidDeadline` is the TS twin of the
domain function, beside `deadlineSortKey`, its existing sibling), the `ffi-web`
seam so nothing invalid can reach `Core::triage` whatever the caller, and the
authority itself. What triage still cannot do is **save without promoting**:
both buttons set a destination stage, and an item not ready to promote simply
stays in Triage (CONTEXT.md), so editing one and leaving it there would need a
mutation path that does not exist yet. `components/forms/Textarea.tsx` is a
local **addition** to the 16-component library (the design project has no
textarea; `description` is the schema's only free-prose field) — worth raising
upstream rather than quietly keeping.

**The pane shell is `screens/questions/`** (#245, ADR-0015), and it took
over Now's Context aside: `RankedRegion.tsx` renders every standing
question's pane, ordered by how much it deserves the eye. `contract.ts` is
what a question owes the shell — an `answerState`
(`answered | bound-but-unacquired | unbound`, three states because **a gap
is not an absence**), a `band` from the five-word salience vocabulary, a
`withinBand` tie-break inside it — **epoch ms of the pane's next relevant
moment, never a duration and never a unit each pane picks for itself**, so
the sort reads no clock and a captured value cannot age between renders — a
one-line `collapsedHeadline` and up to
`MAX_GLYPHS` labelled glyphs — plus its whole expanded rendering and
nothing else; `registry.ts`'s `Record<StandingQuestion, QuestionDef>` is
compile-time exhaustive, so a question added to the vocabulary and not
registered is a type error rather than a pane that silently never appears,
and `requiredSources()` is what `shell/usePaneReadsWiring.ts` requests, so
the two lists cannot drift. `panesFrom` is the 0..N expansion `rankPanes`
runs, and it takes its registry as an argument for one reason: no shipped
question emits more or fewer than one subject, so a test registry through
the real loop is the only thing that exercises the expansion at all. `sort.ts` is the cross-pane order (answerState →
band → `withinBand`, `null` after every non-null → declared question order →
subject key; pure, clock-free, total, `frontier-order.ts`'s own discipline).
`collapse.ts` is device-local and **band-scoped**, in the injectable-
`storage` idiom and never in `settings`: an override applies only while the
pane's computed band still matches the band it was stored against, and a
mismatch is a read-time non-match rather than a deletion — which is exactly
what makes dormant → imminent → dormant *resurrect* it.

**Order is captured in state; content is read fresh.** `RankedRegion`
captures one ranking and re-samples it on two signals only — a completed
cycle (`syncOutcomeSeq`), and `samePaneIdentity` failing (a pane appeared,
vanished, or crossed between an answer and a gap) — never on band or
`withinBand` movement, which would slide a pane out from under the reader's
cursor on the 30-second tick. Position, band chrome, headline, glyphs and
the collapse resolution all read from that one sample, so an override and a
position can never disagree about the band; the expanded pane renders from
the **live** inputs every render, so an optimistic write is instant while
the order stands still. The collapsed row is drawn **entirely by the shell**
and no pane ships a compact form — for the waste pane, dormant *is* the
collapsed row.

`screens/waste-pane/` is that shell's proof, bundled deliberately: a shell
with no pane is exactly the exported, unit-tested, never-wired UI this repo
keeps rejecting. `waste.ts` holds the parser that **pins the unfrozen
`city-waste/v2` body**, the binary band (dormant vs imminent — the eve, the
day, or any day of a holiday week) with a real `withinBand` even while
dormant, `STALE_AFTER_MS = 26h` **beside the band function** (the cost of a
wrong answer, not `2 × cadence`), the bin colours (a documented exception to
"colour encodes status": here it encodes object identity, and every glyph
still carries a label), and `isStaleFreshness`, where `unknown` is never
fresh. **A collection already in the past is refused outright**, not
described: the poll is daily, so between the address's midnight and that
day's fetch the snapshot still names yesterday — well inside 26h, so
freshness says nothing — and a `daysAway <= 0` reading rendered that as
"Trash today". A negative distance is a gap with words (the schedule this
device holds is out of date), the headlines test `=== 0`, and `WasteView`
guarantees `daysAway >= 0`. `wasteSetup` is the binding read, and it is
**four answers, not a boolean**: an unread `bindings` table (`null`) and a
row holding something that is not text are both `bound-but-unacquired`,
because only an actually-unset row may render "Not set up" — telling a
configured reader to set the pane up for the round-trip between mount and
the first `bindings` answer is a wrong answer, not a slow one. A holiday is
read off the snapshot (`collectedOn !== scheduled`) and
**`liveAlerts` is deliberately never read** — a holiday *is* the answer, and
the alert row still serves the notification lane. `zoned-day.ts` closes the
one thing the prototype left open: the payload carries an IANA `zone` and
every day-shaped question is resolved in it via `Intl.DateTimeFormat`, so
"tonight" flips at the address's midnight and not the device's — a per-pane
exception documented at its point of use. An unusable zone is a malformed
payload, never a crash. (Still open, and *not* discharged by this slice:
`city-waste/v2` is not in `server/domain/src/sources.rs` — only the retired
`v1` is. Harmless here, since the read side never checks the registry and
ADR-0015 forbids checking `schema` against it; the registration is
#135–137's.)

The `shell/use*Wiring` hooks are thin glue and **own no clock**: each
re-requests its queries once the core is ready and again on every
`TaskState.syncOutcomeSeq` bump, because ADR-0007's single 60-second
interval in the SharedWorker is the only timer the origin gets.
`useCaptureWiring`'s exported `submitCaptureRequest` posts `capture` and
then `getTriageInbox` right behind it — `task-worker.ts`'s serial queue
makes the second land only after `Core::capture` has already returned — so
the optimistic item is on screen before any network call, which the
per-cycle effect alone can never do (it fires after a cycle, the exact
inverse). `capture-hotkey.ts` is deliberately DOM-free, the caller
extracting the facts from the real `KeyboardEvent`: plain "c", never with a
modifier, never while an editable control has focus or an IME composition
is in progress.

**Component tests are the gate on that threading, and they exist because
typecheck cannot see that something has no caller.** A pure module, a
`use*` hook or a prop that is exported, unit-tested and never wired
compiles clean and passes clean — which is why three of the S10–S13 PRs
were rejected on first review for shipping UI state with no reader (the
worst, `disabled={item.pending}` on a permanently-frozen flag, survived two
rounds). `client/web/vitest.config.ts` keeps `environment: "node"` as the
DEFAULT — the `worker/*` tests assert against a `SharedWorker`-shaped world
with no `document`, and handing them a jsdom global would let a test pass
on a `document` the real runtime lacks. A `*.test.tsx` opts into jsdom per
file with a `// @vitest-environment jsdom` docblock. Everything mounts
through `src/test/component.tsx`, which registers the `afterEach(cleanup)`
RTL would otherwise skip (this repo runs vitest without `globals: true`,
and unclean teardown reads as a flaky assertion) and carries the
`itemDTO`/`taskState` builders so a test states only the fields it is
about. Pure-module tests still cover the deciding logic; the component
tests cover the wiring between them, and neither replaces the other.

**The visual gate is Playwright, and it is local-only.** `pnpm visual` in
`client/web` drives three real viewports (1440 / 1024 / 768 — the third is
the wrap point of `screens/layout.tsx`'s `TwoColumn`, which uses no media
query) across both themes, writing captures to `visual/.captures/` for
review. There is no committed golden and no pixel diff: what it *fails* on
is the machine-decidable subset — horizontal overflow, an unresolved brand
token, a theme switch that never reaches the page. It is deliberately not
in `.github/workflows/client.yml` (`pnpm typecheck` already rebuilds the
wasm core and is that workflow's slow step), and it needs a one-time
`pnpm exec playwright install chromium`. Never `chrome --headless
--screenshot` for this UI — the viewport renders wrong. The registry
`/wrapup` reads is `docs/SURFACES.md`.

## The design system

The UI brand is the "Hummingbird Design System" project on claude.ai/design;
`.claude/skills/hummingbird-design/` is its repo-local mirror (tokens, the
16-component library, the web UI kit — sync record and what was deliberately
omitted in that directory's `github.md`). **All frontend/UI work must use
it: invoke `/hummingbird-design` before styling anything.** The web app
consumes it via `client/web/src/design/` (a copy of the tokens, with
`fonts.css` swapped to self-hosted `@font-face` because the production CSP
allows no Google Fonts) and `client/web/src/styles.css` (maps tokens onto
Tailwind utilities, dark mode on `[data-theme="dark"]`). When the design
project changes: re-pull the mirror first, then re-copy tokens into
`client/web/src/design/`.

## Agent skills

### Issue tracker

Issues live as GitHub issues in `JddAndrewLauren/hummingbird`, driven via the `gh` CLI;
the wayfinder map is issue #1. See `docs/agents/issue-tracker.md`.

### Triage labels

Default vocabulary — `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`,
`wontfix`, plus the non-triage `plan` role. All exist on the tracker.
See `docs/agents/triage-labels.md`.

### Microtasking

`/microtask <issue-id>` breaks one already-selected, stalled Linear issue into a checklist of
~2–5-minute Steps written into its body. See `.claude/skills/microtask/SKILL.md`.

### next-up-personal

`/next-up-personal` picks what to do right now from the Linear workspace — one ranked top
pick plus a health footer — and `/next-up-personal <issue-id>` hands one `agent`-labelled
issue to an agent. See `.claude/skills/next-up-personal/SKILL.md`.

### Domain docs

Single-context — root `CONTEXT.md` glossary plus `docs/adr/`.
See `docs/agents/domain.md`.
