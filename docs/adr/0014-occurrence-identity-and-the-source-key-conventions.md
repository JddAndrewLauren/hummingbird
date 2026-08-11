# ADR-0014: Occurrence identity — what `source_key` names, per source

**Status:** accepted · 2026-08-09
**Context:** the `source_key`-conventions grilling of 2026-08-09 (issue
#134). Discharges the design obligation
[ADR-0012](0012-the-notification-lane.md) placed but did not decide:
*"`source_key` must encode occurrence identity (this week's slide, not
slides-in-general)."* Amends
[ADR-0009](0009-the-owned-schema-and-context-lanes.md) (every `source`
string on every provenance-carrying table takes a version suffix; **live**
gains a definition; `expires_at` is read, never written back), ADR-0012
(`deliveries.generation` gets a source and `rule_id` joins the dedupe key;
the re-raise path is corrected for resolution as well as dismissal;
multi-rule severity is resolved), and
[ADR-0013](0013-the-rule-condition-vocabulary.md) (a retired source is
flagged in the rules UI exactly as a retired field is; `item_threshold`
gains the resolution pass its state-source classification implies). New
glossary terms
(Occurrence, State source, Event source, Live) land in `CONTEXT.md`.

## The axis ADR-0012 didn't name

ADR-0012 said occurrence identity must live in `source_key`, on the strength
of one scenario (next holiday's slide must not be absorbed by last
holiday's). Taken as a universal rule it is wrong, because there are **two**
mechanisms by which an alert becomes a new occurrence, and ADR-0012 named
only one:

| Axis | Mechanism | Rings because |
| --- | --- | --- |
| **New key** | a new `(source, source_key)` → a new row | first raise |
| **Lifecycle** | the same row re-enters live | it had left, and came back |

Which mechanism a source gets is not a style choice. It is forced by whether
the source can ever say *"it's over."*

- **State sources** report the state of a thing: healthchecks (down, then
  up), Home Assistant, and — the surprise — `item_threshold`, because the
  authority holds the item and therefore knows when it stops matching.
  `source_key` names **the thing**; occurrence is carried by the lifecycle.
- **Event sources** report discrete happenings that never end: mail,
  calendar instances, snapshot changes, GitHub and photo-site events. Their
  rows leave only by ack or expiry. `source_key` names **the occurrence**.

> **`source_key` names the thing when a source reports the state of a thing,
> and the occurrence when a source reports events. Which one a source is, is
> declared at wiring time beside its frozen namespace.**

Making the key occurrence-scoped *universally* was considered and rejected:
it leaves an infra up-event with nothing to aim at, so an outage sits live
and top-of-stack until hand-acked, and a flapping check mints a row per
flap — repealing ADR-0012's "an identical re-raise of a live alert is
absorbed silently."

Note the split does **not** follow ADR-0013's `mints` flag, and cannot be
derived from it. Pushed sources divide too: healthchecks reports state,
while a GitHub event is a discrete occurrence. The declaration is per
source, not per kind.

## Occurrence keys are pure functions of the occurrence

Two properties already in the codebase depend on an occurrence key being
re-derivable from the same occurrence tomorrow:

- ADR-0011: *"Losing a cursor degrades to re-fetch-and-upsert, which the
  dedupe key absorbs."*
- `server/authority/src/handlers/alerts.rs`: a replayed payload is
  byte-identical, so the upsert is a no-op.

Hence:

> **An occurrence `source_key` is a pure function of source-supplied values
> that identify the occurrence, and of nothing the observer knows.** No
> `now()`, no poll count, no cursor position, no row id.

And a second clause, which is the one that bites in practice:

> **Where an occurrence has both a fixed coordinate and a mutable value, the
> key uses the fixed one.** A correction updates the alert; it never mints a
> second.

The flagship scenario is the test. The city page says trash slides Monday →
Tuesday, then next poll corrects it to Wednesday. Keyed on the *new* date
that is two alerts for one slide; keyed on the *scheduled* date
(`trash:2026-08-17`) it is one alert whose title updates, and no second
ring. Google Calendar reached the same conclusion independently: an
instance's `originalStartTime` identifies it *even after the instance is
moved*.

## One occurrence is one alert

Two rules may match one event. For `mints: false` kinds ADR-0013 already
settled it — the alert exists, N rules produce N deliveries. For `mints:
true` kinds it was open, and it is a `source_key` question because the
alternative is putting `rule_id` in the key.

- **`rule_id` stays out of the key.** The key names the occurrence, and the
  occurrence is the email — not the match. Keying on the match re-couples
  provenance to rule config, so rebuilding a rule orphans every alert it
  ever minted, and one email tripping two rules puts two identical-looking
  rows in the stack, each wanting its own ack.
- **Severity ratchets up and never down.** Two rules on one email produce
  one alert at the higher severity and two delivery rows. The second rule
  can escalate the alert; it can never quiet it. ADR-0012 already treats
  escalation-while-live as a delivery-warranting transition, so this needs
  no new mechanism — only a severity rank function in `domain`, with a
  defined fallback for an unranked string, which the rules UI needs anyway
  to order its dropdown.

Without the ratchet the collision is silent: a `normal` rule's mint upserts
an `urgent` row and downgrades it, and nothing surfaces that.

## Live: how a settled alert rings again

ADR-0012 says a delivery is warranted when an alert re-enters live-unacked,
*"re-raise after resolution or dismissal."* The shipped handler says
`dismissed_at` *"is human-owned: never touched"* and its re-raise `UPDATE`
omits the column. Both cannot be true: today, an acked infra alert can never
ring again, and the axis above makes every state source depend on that path.

Clearing `dismissed_at` on every re-raise is not the fix — each "still down"
ping would resurrect the alert just acked, which is the flapping spam
ADR-0012 rejected by name. The distinguishing fact is already stored:

> **An alert is live when it has not expired and neither its resolution nor
> its dismissal is still current — where a stamp is current only if the
> alert has not been raised since:**
>
> ```sql
>     (resolved_at  IS NULL OR raised_at  > resolved_at)
> AND (dismissed_at IS NULL OR raised_at  > dismissed_at)
> AND (expires_at   IS NULL OR expires_at > now)
> ```

`dismissed_at` already means "the human waved this away at T." A raise
stamped after T is self-evidently a later occurrence; a replay of the raise
that *was* dismissed is stamped before T and stays quiet. No schema change,
no column cleared, and no machine writing a human-owned field.

The same comparison is owed to `resolved_at`, and for the same reason: a
state source that says "up" and later says "down again" re-raises the row it
resolved, and a predicate testing `resolved_at IS NULL` alone would leave
that second outage invisible. ADR-0012's read-time sort (`dismissed_at IS
NULL AND resolved_at IS NULL`) is superseded by the three clauses above —
the two lifecycle columns are symmetric, and each is dead the moment a later
raise overtakes it.

Expiry is the third clause rather than an auto-dismissal. ADR-0009's
`expires_at` comment called it "auto-dismiss," which cannot be right: only
an explicit human gesture sets `dismissed_at` (ADR-0012, "swipe is not a
gesture"), and a machine writing that column both violates the human-owned
rule and makes an expired-then-re-raised occurrence indistinguishable from
an acked one. `expires_at` is read, never written back. Corrected in
ADR-0009.

Two things follow:

- **`deliveries.generation` is the alert's `raised_at` at send.** ADR-0012
  left the column with no source in the schema; it needs no counter and no
  migration. Its dedupe key becomes literally checkable: `(alert_id,
  rule_id, raised_at, severity)`. `rule_id` is in the key because a delivery
  is one rule's ring, not the alert's: ADR-0013 already settled that N
  matching rules produce N deliveries, and the severity ratchet above says
  two rules on one email produce two delivery rows. Omitting `rule_id` would
  silently collapse exactly the case where both rules assign the *same*
  severity — the two-rule promise holding for `urgent`+`normal` and quietly
  failing for `normal`+`normal`. Dedupe suppresses a *repeat of the same
  rule's* ring, which is the flapping case it was written for.
- **A wiring-time obligation on every state source:** `raised_at` is the
  moment the condition began, never the moment the webhook fired. A source
  stamping `$NOW` on every ping un-dismisses its own alert on every ping.
  Documented, not validated — a rejection rule cannot distinguish a genuine
  outage that started 20 seconds ago from a `$NOW` stamp, and it would
  reject legitimate raises to catch a convention error. In this lane a
  rejected raise is silence.

The cost, accepted: "live" stops being a column test and becomes a
three-clause predicate that three consumers must get right — the read-time
sort, the delivery-warranting check, and the client mirror. Getting it wrong
shows acked alerts (visible, annoying) rather than hiding live ones
(invisible), which is the right direction to fail, but the schema cannot
enforce it. The mitigation is that it is written **once, in `domain`**, as
the single predicate all three call; no consumer re-spells it in SQL.

## The conventions

| Source | Shape | `source_key` |
| --- | --- | --- |
| `gmail/v1` | event | the Gmail message id |
| `m365-mail/v1` | event | `internetMessageId` |
| `google-calendar/v1` | event | `<eventId or recurringEventId>:<originalStartTime>` |
| `m365-calendar/v1` | event | `<seriesMasterId or id>:<originalStart>` |
| `city-waste/v1` ***(retired → `v2`, #189)*** | event | `<stream>:<scheduled-date>` — `trash:2026-08-17` |
| `city-waste/v2` ***(#120)*** | event | the scheduled collection date alone — `2026-08-17` |
| `item-threshold/v1` | **state** | `item:<id>` |
| `healthchecks/v1`, `home-assistant/v1` | **state** | the check or entity id, authored in the webhook body |
| `github/v1`, `photo-site/v1`, `gmail-alert/v1` | event | the source's own event or message id |

Two of these are judgment calls rather than consequences:

- **Mail keys on the message, not the thread.** A twenty-reply thread that
  trips a rule gives twenty alerts. Thread-level keying instead lets the
  *first* message's alert absorb every later one, so a thread already acked
  goes silent exactly when it heats up. Reply-all noise is the rule's
  problem — add a condition — not the key's.
- **M365 keys on `internetMessageId`, not the Graph `id`.** The Graph `id`
  **changes when a message moves folders** (verified 2026-08-09), so a
  rule-matched mail that gets filed re-alerts as a new occurrence.
  `Prefer: IdType="ImmutableId"` fixes that but must be sent on every
  request and still breaks on a move to an archive mailbox.
  `internetMessageId` is assigned by the sender and preserved by Exchange —
  the one mail identity that does not depend on a provider's storage
  decisions.

`item_threshold` deserves its own note. ADR-0013 already wrote its
behaviour down as load-bearing — *"a permanently-overdue item produces one
alert that sits live until acked or resolved"* — which is `item:<id>`
exactly. Keying it `item:<id>:<deadline>` would have made a re-committed
deadline a fresh row, dodging the dismissal fix above; it was rejected
because that fix is owed to healthchecks regardless, and because a live
alert during a deadline edit would orphan a row showing the old deadline.

### Who says "it's over" for `item_threshold`

Calling `item_threshold` a state source is the claim that something can
report the condition ended. Nothing did. The gap is not cosmetic: ADR-0013's
`within_next D` is *unbounded on the past side*, so an overdue item keeps
matching forever, and archived items are **excluded from evaluation
entirely** — so completing or archiving the item removes it from the scan
that would otherwise notice, and the alert it minted sits live until
hand-acked. The one source that knows the condition ended is the one that
never gets to say so.

> **The DO alarm's resolution pass iterates live `item-threshold/v1` alerts,
> not items.** For each, it reads the item named by the `item:<id>` key and
> stamps `resolved_at` when the item is done, archived, deleted, or no
> longer matches the rule that minted the alert.

Driving the pass from the alert side is the whole fix. An item-side scan can
only resolve alerts for items it still sees, which is precisely the set that
does not need resolving. The alert row already carries the key, so the
lookup is a primary-key read per live alert — bounded by the live stack,
which the operator keeps small by construction.

Two consequences worth stating:

- **A missing item resolves rather than errors.** A deleted or unknown
  `item:<id>` is the condition ending in its most total form.
- **Resolution is not dismissal, and does not silence the next occurrence.**
  A re-committed deadline re-raises the same row, `raised_at` overtakes
  `resolved_at`, and the live predicate above lets it ring again. This is
  the second half of what keying on `item:<id>` bought.

Mail and the other event sources need no equivalent: they never end, which
is why they leave by ack or `expires_at` instead.

## The registry, and why it is a tripwire

Nothing can validate a recipe. `source_key` is opaque to the server by
design — no delimiter grammar, no parsing, ADR-0009 rule 4's "entire common
core" intact. What can be caught is *drift*, and both drift symptoms are
quiet: change a recipe and old rows orphan (an acked alert returns once
under a new key) or new occurrences absorb into stale rows (silence).

**A `domain::sources` registry, one entry per frozen `source` string**,
holding its shape (state or event), its key recipe as a doc comment, and a
frozen test vector — a sample occurrence in, the expected `source_key` out.
Adapters compute keys through it.

The precedent is in the repo, written for this exact failure. `sweep.py`:

> NEVER CHANGE. Every Linear issue id an adapter has ever minted is
> `sha256(namespace + source_key)`. Changing a namespace byte string
> re-mints every id in that source, which silently breaks idempotency and
> duplicates every open capture. One frozen namespace per source keeps the
> id spaces disjoint (ADR-0002); each is guarded by its own frozen test
> vector.

Substitute "alert id" for "Linear issue id" and the sentence holds verbatim:
`alerts.rs` mints ids as `sha256` over `(source, source_key)` too. The
registry reads at build time, not run time — its job is not configuration,
it is converting a silent recipe change into a failing build.

## Freezing, and the only sanctioned change

**Recipes are frozen. The only sanctioned change is bumping the `source`
string to `/v2`.** Old rows stay under `city-waste/v1` and age out through
their normal lifecycle, new occurrences land under `/v2`, the id spaces
never collide, and no migration touches a Durable Object.

The sharp cost: **a bump silently breaks every rule naming the old source.**
`source eq 'city-waste/v1'` matches nothing forever after, in a default-deny
lane where that is indistinguishable from a quiet week. ADR-0013 solved this
shape for a different field — *"a rule whose kind later stops declaring a
field it names is flagged invalid in the UI at load, never silently
non-firing."* A retired source is flagged identically, and the registry
above is what knows a source is retired. That is its second job.

*Instantiated by [#189](https://github.com/JddAndrewLauren/hummingbird/pull/204):
`city-waste/v1` above is not a hypothetical any more — the registry retires
it to `v2` for real, both to give the retired-source flag (`RuleProblem`'s
`RetiredSource` variant, wired into `validate_rule`) and the
`POST /api/admin/tokens` mint gate ("nothing new should be minted under a
retired source," below) something real to exercise end-to-end. Safe to do
for real: nothing is deployed yet and the city-waste poller (#135-137) is
unbuilt, so no adapter has ever minted a row under it, and `city_waste_v1_key`
stays defined with its frozen vector intact — retirement is a namespace-string
change, never a recipe change. No `city-waste/v2` entry exists in the
registry yet; that lands with the poller that actually produces `/v2` rows.*

*Amended 2026-08-10 by [#120](https://github.com/JddAndrewLauren/hummingbird/issues/120):
`city-waste/v2` is registered, and it is not merely v1's recipe under a new
string — three things changed, each recorded in the tables above.*

*One: **the recipe drops the stream.** The domain corrected under this
source after v1 was written — there is only ever one collection day, and
everything going out that week goes out together — so one week's slide is
one occurrence, and the key is the scheduled date alone. Keying per stream
would mint three near-identical rows for one holiday, each wanting its own
ack. Which bins go out is a property of the occurrence, carried in the alert
body and in the snapshot the pane reads, never part of its identity. This is
the one case so far where a `/vN` bump carried a real recipe change rather
than a namespace change, which is exactly what the bump mechanism is for.*

*Two: **the expiry wording is tightened to the later of the two dates.**
v1's "end of the affected collection date" is ambiguous, and read as the
originally-scheduled Monday it takes the holiday text off the pane on the
Tuesday morning it exists to warn about. v1's own string stays as written —
a retired entry is frozen, including the wording that describes what its
already-minted rows meant.*

*Three: **the shape stays `event`, despite one address with one schedule
looking stateful.** The test above is whether the source can ever say *it's
over*, and the council's page never reports a cancelled slide. Declaring it
`state` would force resolution to be a diff against the previous reading,
which destroys the one property this lane rests on (materiality is deviation
from cadence, never a diff — that is why the ordinary roll-forward is
silent). It also keeps two consecutive holiday weeks as two occurrences,
which is ADR-0012's Christmas/New Year case, written about this very
source.*

This requires **every `source` string to carry a version suffix from the
start**, uniformly. ADR-0009's examples were inconsistent (`'healthchecks'`,
`'home-assistant'`, but `'gmail-alert/v1'`), and a bare name has no escape
hatch: `healthchecks` → `healthchecks/v2` reads like a different service
rather than a revision. Corrected there; free now, because nothing is
deployed and no rows exist.

"Every" means all three provenance-carrying tables, not just `alerts`.
`items.source` and `context_snapshots.source` are the same frozen namespace
by ADR-0009 rule 4 — `'web'` becomes `'web/v1'`, `'anthropic-usage'` becomes
`'anthropic-usage/v1'`, `'f1'` becomes `'f1/v1'` — and ADR-0013's
calendar-busy snapshot is written under `google-calendar/v1`. A rule reading
`source eq …` cannot tell which table a source string came from, so a
convention holding on one of them is not a convention. One consequence: the
suffix claims the slash, so a source that was naming a sub-scope with it
folds that into the name (`github-hummingbird/v1`, not `github/hummingbird`,
which would read as version `hummingbird`).

## Expiry: only where the occurrence ends

Event sources never resolve, so their rows leave only by ack or
`expires_at`. A blanket TTL for stack hygiene is the wrong instinct — an
urgent alert vanishing unacked on a timer is the same defeat as
swipe-equals-ack, which ADR-0012 rejected outright.

> **`expires_at` is set only where the occurrence has a natural end the
> source already knows.**

| Source | `expires_at` |
| --- | --- |
| `google-calendar/v1`, `m365-calendar/v1` | the instance's end time |
| `city-waste/v1` ***(retired → `v2`)*** | end of the affected collection date |
| `city-waste/v2` ***(#120)*** | end of the **later** of the scheduled and the slid-to collection date |
| `gmail/v1`, `m365-mail/v1` | none, ever |
| `github/v1`, `photo-site/v1` | none unless the event carries one |

A "meeting starts in 15 min" alert is genuinely meaningless once the meeting
ends, and a slide alert once the pickup happened — both have a referent to
expire against, which is what a source-declared lifetime is for. An email
has no such moment; nothing about it becomes untrue with time, so it sits
until acked. That is the contract, not a leak.

## Rejected alternatives

- **Occurrence in the key for every source, no exceptions.** One sentence
  instead of a conditional, and no wiring-time judgment to get wrong. It
  deletes resolution as a working concept: an infra up-event has nothing to
  aim at, so the outage sits live until hand-acked, and a flapping check
  mints a row per flap — the spam ADR-0012 rejected when it declined "every
  re-raise rings." Worst reversibility of the options, too, since keys are
  baked into `sha256` ids.
- **A structured key (`<entity>#<occurrence>`) with resolve-by-prefix.**
  Genuinely gets both properties, and pays by making `source_key` parsed
  structure: a delimiter convention, an escaping rule for entities
  containing it, and a "newest live row" tiebreak. That contradicts ADR-0009
  rule 4's *entire* common core, and it is the JSON-path-into-payloads
  pattern ADR-0013 already rejected, wearing a different hat.
- **`rule_id` in the key.** One event tripping two rules becomes two
  identical-looking rows, each wanting an ack, and rule identity leaks into
  provenance so rebuilding a rule orphans its history.
- **First-match-wins instead of the severity ratchet.** No rank function to
  build, and it makes rule *order* significant — which nothing else in the
  system is.
- **Clearing `dismissed_at` on every re-raise.** The obvious fix; every
  "still down" ping resurrects the alert just acked.
- **A `dismissed_generation` column.** The comparison predicate needs no
  column: the dismissal timestamp already carries the information.
- **Resolving `item_threshold` alerts from an item-side scan.** It can only
  resolve alerts whose items it still sees — and the cases that need
  resolving (done, archived, deleted) are exactly the ones the scan has
  stopped visiting.
- **Expiry implemented as an auto-dismissal.** A machine writing the
  human-owned `dismissed_at`, and an expired occurrence becoming
  indistinguishable from an acked one.
- **A delivery dedupe key without `rule_id`.** Collapses two rules that
  happen to agree on severity, so the "N rules, N deliveries" contract holds
  or fails depending on the severities the operator picked.
- **Validating that `raised_at` is not `now()`.** Cannot distinguish a
  genuine outage that began seconds ago from a `$NOW` stamp, and rejects
  legitimate raises to catch a convention error.
- **Keying `item_threshold` on `item:<id>:<deadline>`.** Repeals ADR-0013's
  stated "minted once, dedupe absorbs every later tick," and orphans a live
  row showing a stale deadline whenever a deadline is edited.
- **The Graph message `id` for M365 mail.** Changes on a folder move, so
  filing a matched mail re-alerts it.
- **Thread-level keying for mail.** The first message's alert absorbs every
  later one, silencing a thread precisely as it heats up.
- **ADR prose without a registry.** Nothing catches a recipe change, and
  both drift symptoms are quiet.
- **A blanket TTL on alerts.** Unacked urgents vanish on a timer.

## Amendment (2026-08-11): the registry becomes the source registry, not the alert registry

[#254](https://github.com/JddAndrewLauren/hummingbird/issues/254) found the
crack the "registry, and why it is a tripwire" section above left open:
`domain::sources::REGISTRY` was scoped, by its own module doc, to sources
that mint an **alert** `source_key` — but `#145`'s mint gate
(`handlers/admin_tokens.rs::mint`) checks every `ingest`-scope token's
`source` against it regardless of which table that token is destined to
write, and `#120` gave the ingest scope a second table
(`POST /api/snapshots`) the registry had no opinion about at all.
`city-waste/v2` passed the gate by accident of being genuinely both an
alert and a snapshot source under one string — the first **snapshot-only**
source (ADR-0009 names three: `anthropic-usage/v1`, `github-hummingbird/v1`,
the races lane) would have 400d at mint with "not a registered alert
source," which is true and useless: the operator's correct next step is not
"register it as an alert source."

Three options were on the table (issue #254's grilling session,
2026-08-10): a second, parallel snapshot registry with its own
frozen-namespace discipline and its own retirement story; a shape flag on
[`SourceEntry`](../../server/domain/src/sources.rs) naming which tables a
source may write, read by the mint gate; or dropping the registry gate
entirely for a token whose intended use is snapshot-only.

**Decided: the shape-flag option, and it goes further than the mint gate
alone.** `SourceEntry` gains `writes: Writes` — `Alerts | Snapshots | Both`
— and `REGISTRY` stops being an alert-only registry and becomes *the*
source registry, covering every source string that can appear on either
`alerts.source` or `context_snapshots.source` (ADR-0009 rule 4's shared
frozen-namespace convention already said these are one naming convention
across three tables; this amendment is that rule's registry catching up to
the rule's own claim). `key_recipe` narrows from a required field to
`Option<&'static str>`, present exactly where `writes.writes_alerts()` is
true — a snapshot-only source mints no `source_key` and so has no recipe to
document, and the frozen-recipe tripwire loses nothing from this: it keys
off each recipe *function's* own frozen test vector, never off this field.

Rejected, and why:

- **The second registry** is the most faithful reading of "the registry
  exists because a snapshot source mints no `source_key`," but it is also
  the most machinery: a second frozen-namespace discipline, a second
  retirement story (`retired_as` duplicated onto a second type), and the
  mint gate consulting two lists forever, for a distinction (which
  table(s) one string may write) that is a single field on the entry the
  registry already has.
- **Dropping the gate for a snapshot-only binding** needs its own way to
  say "this token is snapshot-only" at mint time — which is the shape flag
  again, just moved from the entry onto the mint request, and now
  unenforceable against a token minted for the wrong intent (nothing
  would catch a snapshot-only token later posting alerts under a source
  no rule engine or alert reader was ever told to expect).

**The declaration has two readers now, not one, or it is decoration.**
`handlers/admin_tokens.rs::mint` keeps checking only enrollment (`find`
resolves) and retirement (`retired_as.is_none()`) — an ingest token is not
itself table-scoped, so the mint gate has no business asking which table a
source declares; its 400 for an unenrolled source is corrected to name the
actual remedy, `"is not enrolled — add a registry entry"`, in place of the
old "is not a registered alert source." The real per-table check moves to
the write side: `handlers/alerts.rs::ingest` rejects a source the registry
finds but does not declare `writes_alerts()` for, and
`handlers/snapshots.rs::ingest` its `writes_snapshots()` mirror — each a
400 naming the source and the table, both **after** the existing
structural (400) validations and **before** the token-source mismatch
(403) check. A malformed payload therefore still reports its own problem
first; a wrong-source token does not. A token bound elsewhere that posts a
declared-but-wrong-table source is answered with this 400's body, not the
empty 403 it would otherwise get — which discloses nothing, since the only
facts the body names are the source string the caller itself sent and the
table it itself chose, and it is the more useful of the two answers for the
case the check exists for: a poller aimed at the wrong lane learns *that*,
instead of an empty 403 that reads as a credential problem it does not
have. A source the registry has never heard of is left
alone at both write sites: enrollment itself stays the mint gate's sole
job, and every legitimately-minted ingest token has already passed it —
this is defense in depth for a *declared-but-wrong-table* source, not a
second enrollment gate.

**Enrollment is at wiring time, not speculative.** This amendment does not
enroll `anthropic-usage/v1` or `github-hummingbird/v1` — each enrolls when
its own lane is actually built, exactly as every alert source above did. The
races lane did exactly that at #266 (2026-08-11) and is now enrolled as
`race-schedule/v1`: `Shape::Event`, `Writes::Both`, `Expiry::Always("the
race's start time")`, keyed `<series>:<race start instant, epoch ms>` — the
start instant rather than the tidier `season:round`, because a postponement
must mint a *new* occurrence to ring at all (under `season:round` the row
already exists, the clock-free title changes nothing source-owned, so
`restamp_on_change` never restamps and a race moved by a month is silent). What it does enroll for real is `city-waste/v2`'s
`writes: Writes::Both` declaration, absorbing #245's open note that the
snapshot half of that source had never been registered at all (the read
side never checked the registry, so the gap was silent rather than
broken) — and, retroactively, the observation that every evaluated-stream
poller's source (`gmail/v1`, `m365-mail/v1`, `google-calendar/v1`,
`m365-calendar/v1`) is `Writes::Both` too: each mints alerts on a rule
match **and** writes its own delta cursor (`google-calendar/v1` also the
`busy_now` gauge) as a `context_snapshots` row under the same string. Only
the webhook sources that write no cursor of their own
(`item-threshold/v1`, `healthchecks/v1`, `home-assistant/v1`, `github/v1`,
`photo-site/v1`, `gmail-alert/v1`, and the retired `city-waste/v1`, which
predates the poller entirely) stay `Writes::Alerts`. No shipped source is
`Writes::Snapshots` alone yet — the fixture proving that direction of the
mechanism is a locally-built `SourceEntry` in `sources.rs`'s own tests,
the same pattern `a_retired_source_is_representable_and_distinct_from_unknown`
already used for retirement before `city-waste/v1` gave it a real one.

## Deferred, not rejected

- **Pruning acked alert rows** (#155). Decided in
  [ADR-0016](0016-the-alert-horizon.md): a *wire* horizon, not a prune —
  rows are never deleted, and a settled alert stops riding the sweep 90 days
  after every stamp that settled it. Not a TTL: a live alert rides forever.
- **A `word`-boundary-style refinement for reply-all storms.** If
  message-level keying on a hot thread demonstrates as noise, the answer is
  a rule condition, not a key change.
