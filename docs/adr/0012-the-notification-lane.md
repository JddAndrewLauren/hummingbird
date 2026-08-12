# ADR-0012: The notification lane — rules promote, deliveries ring, humans ack

**Status:** accepted · 2026-08-09 · **amended 2026-08-09 by
[ADR-0013](0013-the-rule-condition-vocabulary.md):** the condition operator
set and typed field catalogue are decided there; `event_kind` loses its
`CHECK` and becomes an open registry key, nullable (NULL = any kind);
conditions gain a `negate` flag; a fifth kind `alert_raised` gives pushed
sources a path to a delivery. · **amended 2026-08-09 by
[ADR-0014](0014-occurrence-identity-and-the-source-key-conventions.md):**
the `source_key` obligation below is discharged and split — only *event*
sources encode occurrence identity, state sources name the thing and use the
lifecycle; `deliveries.generation` is the alert's `raised_at` at send, and
`rule_id` joins the delivery dedupe key so two rules agreeing on severity
still ring as two; re-entering live-unacked after a dismissal *or a
resolution* works by the `raised_at >` comparison rather than by clearing the column; and two
rules matching one event mint one alert whose severity ratchets up, never
down.
**Context:** the push-notifications grilling of 2026-08-09. Companion to
[ADR-0011](0011-context-ingestion-moves-server-side.md), which owns how
stream events reach the rule engine; this ADR owns what a rule is, what a
match does, and how a notification lives and dies. Amends
[ADR-0002](0002-sources-join-by-role-urgency-computed-at-read-time.md)
through its own petition mechanism: the out-of-scope list named
"hummingbird as pager/alert-router" and "M365 mail" as re-entrants awaiting
"a consumer and a demonstrated desire" — both now exist, and this is the
petition, granted. New glossary terms (Rule, Promotion, Tier, Delivery log,
Ack) land in `CONTEXT.md`.

## The clean-layer principle

The operator silences or ignores most native notifications, deliberately.
Hummingbird's notification lane is **additive to and independent of**
whatever native apps notify — that a phone's calendar app *could* announce a
meeting is never a reason to exclude calendar events from rules, because the
native channel is part of the noise floor the operator has already muted.
Hummingbird is the one channel configured to be trusted completely:

- **A rule's existence is the importance judgment.** A Rule is the
  operator's standing declaration that matched events must cut through the
  noise — human-set intent persisted as a record, the same category as
  `priority` on items (ADR-0009), never machine inference.
- **Default-deny.** Nothing rings unmatched. Unmatched events still land
  where their lane puts them (mirror data, snapshots, push-source alerts) —
  they just never ring. Silence is the system's default state, which is what
  makes the sound trustworthy.

## Decision

### Doctrine: where judgment lives

**Rules shape records at birth and emit deliveries; humans mutate lifecycle;
ranking is a read-time query over both.** ADR-0002's urgency doctrine is
reaffirmed, not repealed:

- Rules evaluate **at fire time** — on each poll batch, each incoming
  webhook, and a periodic DO alarm tick for time predicates. Time-varying
  predicates ("due within 2h") work because evaluation repeats on the
  clock; no record ever migrates or is re-classed as time passes.
- A rule may stamp `severity` on the alert **it is minting** — creation-time
  data about that event, indistinguishable in kind from a source-supplied
  severity. A rule may **never mutate an existing record**: no urgency
  field, no priority writes, no stored class for other consumers to read.
  (The three failure modes that killed write-back: stale stamps needing
  decay machinery, a two-writer fight with human-set `priority`, and rule
  ticks racing human edits through CAS.)
- "Top of stack until addressed" is a **read-time sort over lifecycle
  state**: live urgent alerts (ADR-0014's live predicate, severity urgent)
  rank above everything, on every surface, because they are live and
  urgent — computed fresh at every read. The stored things are facts (what
  severity this event was born with; whether it has been addressed), not
  judgments. Facts don't go stale; judgments do.

### The Rule

A structured record, one row per rule, individually CAS-editable and
delta-pulled like everything else — editing rule A can never conflict with
editing rule B, and enable/disable is a one-field toggle from a phone.

```sql
CREATE TABLE rules (
  id         TEXT PRIMARY KEY,
  name       TEXT NOT NULL,
  event_kind TEXT NOT NULL CHECK (event_kind IN
               ('email','snapshot_change','item_threshold','calendar_event')),
  conditions TEXT NOT NULL,       -- JSON: [{field, op, value}, …], ANDed
  severity   TEXT NOT NULL,       -- stamped on the alert this rule mints;
                                  --   free text, mirroring alerts.severity
  tier       TEXT NOT NULL CHECK (tier IN ('urgent','normal')),
  enabled    INTEGER NOT NULL DEFAULT 1,
  updated_at INTEGER NOT NULL,
  version    INTEGER NOT NULL
);
```

- **Structured predicates, no DSL.** Conditions are ANDed field tests
  (`from contains 'twinion'`, `label = 'alert-high'`); OR is a second rule.
  Each condition renders as one row in the config UI — phone-editable,
  serializable, backtestable, no parser to build or debug. Calendar state is
  available as a condition field on any rule ("AND calendar not busy"), so
  suppression is a condition, not a mechanism.
- **All four event kinds ship at launch** — email (M365 + Gmail),
  snapshot-change (ADR-0009 rule 2's material changes), item-threshold
  ("due within 2h", evaluated by the DO alarm over items already in the
  authority), and calendar-event ("starts within 15 min"). Calendar-event
  is included *because of* the clean-layer principle, not despite native
  redundancy.
- **The config UI lives in the web app**: a rules screen, one row per
  condition, with a backtest action that re-fetches recent source history
  and shows which events a draft rule would have promoted (ADR-0011).

### Delivery

- **FCM is the single rail.** It serves Android now and iOS later through
  the same API. Desktop/web gets **no push, ever** — desktop surfaces read
  the alert stack in-app.
- **Two tiers, urgent and normal**, mapping to Android notification
  channels and FCM priority. Tier is metadata on the *notification*, never
  on the record. The **urgent channel requests Do-Not-Disturb bypass** at
  setup — a filter built to cut through blanket silencing must not get
  caught in it.
- Devices register as push targets — the notification sibling of `tokens`,
  individually revocable:

  ```sql
  CREATE TABLE push_targets (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,              -- 'pixel-9', 'pixel-watch'
    platform   TEXT NOT NULL CHECK (platform IN ('android','ios')),
    fcm_token  TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    last_seen  INTEGER,
    revoked_at INTEGER
  );
  ```
- **Wear is bridged**: Android notifications reach the paired watch
  automatically, Ack action intact, zero Wear code. On-watch surfaces — a
  complication with the live urgent count, an alert-stack tile — are the
  **planned** next step (a standalone Wear app with its own push target),
  recorded here so it's a destination, not speculation.
- Every send writes one row to the **delivery log** — the durable memory of
  what rang, whose job is silence:

  ```sql
  CREATE TABLE deliveries (
    id         TEXT PRIMARY KEY,
    alert_id   TEXT NOT NULL REFERENCES alerts(id),
    rule_id    TEXT NOT NULL REFERENCES rules(id),
    generation INTEGER NOT NULL,           -- the alert's lifecycle generation at send
    severity   TEXT NOT NULL,
    tier       TEXT NOT NULL,
    sent_at    INTEGER NOT NULL
  );
  ```

### When a push may fire again: transitions, not states

A delivery is warranted when an alert **enters** live-unacked (first raise,
or re-raise after resolution or dismissal — a second outage, next holiday's
slide) or when its severity **escalates** while live. An identical re-raise
of a live alert is absorbed silently — flapping sources cannot spam. The
dedupe key is effectively (alert, rule, lifecycle generation, severity
level), and it places a design obligation already implicit in ADR-0009's
"re-raise upserts": **`source_key` must encode occurrence identity** (this
week's slide, not slides-in-general). **[ADR-0014] discharges this and narrows it:**
the obligation holds for *event* sources, which have no way to report that
an occurrence is over; a *state* source (an infra check, an item's deadline)
names the thing instead and re-enters live through its own lifecycle.
"Enters live-unacked" is made precise there, and the naive column test this
ADR assumed (`dismissed_at IS NULL AND resolved_at IS NULL`) is superseded:
each lifecycle stamp holds only until a later raise overtakes it, and expiry
is a third clause. The rule in the dedupe key is the one that rang; the
generation is the alert's `raised_at`.

*Amended 2026-08-12 by [#188](https://github.com/JddAndrewLauren/hummingbird/issues/188):
**"severity level" in the dedupe key is compared by rank, not by string
equality: once something has rung for an (alert, rule, generation), a further
delivery is warranted only by an escalation above the highest severity already
rung for it.** The *entry* transition above is untouched and asked first — the
first ring of an occurrence lands whatever its severity, ranked or not, and
only the escalation half consults the order. Collapsing the two is a live trap:
`severity` is free text, an unranked string ranks below every known one, and a
rank-only test would silently never ring a source that raises at `warning`. As
shipped,
`deliver` matched the severity string exactly, which made any *change* of
severity a fresh transition — including a fall. That was invisible only
because the alerts ingest handler kept the stored severity monotonic, and
[ADR-0014's note of the same date](0014-occurrence-identity-and-the-source-key-conventions.md)
withdraws that ratchet: the row is now a reading, free to fall, so this layer
has to decide direction for itself. It is the layer that should — this ADR's
own transitions-not-states rule says a ring is warranted by an escalation,
and it never said a de-escalation warrants one. The reasoning for the split
lives in ADR-0014's note; only the dedupe consequence is recorded here.*

*Concretely: a fall is silent, and so is a later climb back to a level this
generation has already rung — the reader was told that much already. An
unchanged re-raise is suppressed exactly as before, since an equal rank is
not an escalation, so the rank comparison strictly subsumes the string match
it replaces. The `UNIQUE` constraint in the DDL below is unchanged and needs
no migration: it still keys the log rows, which continue to record the exact
severity sent. The ranking is `domain`'s one total order, so an unranked
string ranks below every known severity and cannot ring past one.*

Deferred, not rejected: a nag re-ring for urgents unacked after N hours.
The top-of-stack sort already nags visually; add the timer only if that
proves insufficient.

### Ack: swipe is not a gesture

Only an **explicit gesture** — the notification's Ack action (from phone or
wrist) or an in-app dismiss — sets `dismissed_at`. Swiping a notification
away ends *that delivery* and nothing else: the alert stays live and holds
the top of the stack. A reflexive swipe must not be able to silently
discharge an urgent alert; the whole lane exists on the promise that what
rang was worth addressing.

### Sequencing: notifications ship in their final home

**Task-parity cutover (ADR-0008) → full Android client (embedded core,
ADR-0003) → this lane goes live.** No notification shell, no interim rail:
the operator's highest-trust channel does not launch on throwaway
infrastructure, and ADR-0009's "alerts must not delay the cutover" already
sequences the wiring after task parity. The cost — the stated top personal
priority ships last in the chain — is accepted deliberately; it makes the
notification lane the Android client's flagship feature rather than a
half-app preceding it.

## Rejected alternatives

- **Rules write urgency back onto existing records.** Considered seriously
  (it was the initial ask) and dissolved by its own motivating scenario:
  "trash slides to Tuesday → push, and top-of-stack until addressed" needs
  only severity-at-mint plus the alert lifecycle plus a read-time sort —
  the write-back the scenario seemed to demand was the alert lifecycle
  already in the schema. What remained of write-back failed three concrete
  scenarios: stale stamps (stored urgency describing a moment that passed,
  requiring decay machinery — i.e., the recomputation it tried to avoid),
  the two-writer fight with human-set `priority`, and rule ticks generating
  CAS churn against the operator's own edits.
- **An expression DSL** ("email.from ~ 'twinion' && …"). Power that can't
  be comfortably edited on a phone screen, plus a parser and its error
  surfaces, for a personal rule set unlikely to exceed dozens of rules.
  The structured escape hatch (one raw-expression condition type) was also
  declined: committed complexity for flexibility not yet demonstrated.
- **A digest tier from v1.** Digest is the one tier that isn't
  fire-and-forget — held matches, a scheduled flush, composed summaries.
  Deferred until the two-tier stream demonstrates a real "too many
  normals" problem; `tier` is a string, so adding `'digest'` later is a
  vocabulary extension, not a migration.
- **Swipe = ack.** One-gesture convenience at the cost of reflex-swipes
  silently discharging urgent alerts — defeats the "until addressed"
  contract. Per-tier ack semantics were also declined as machinery ahead of
  need.
- **A notification-shell Android app, or ntfy as an interim rail.** Both
  ship sooner; both put throwaway artifacts on the highest-trust channel —
  the shell a half-app to maintain and migrate, ntfy a third-party
  dependency to retire. Rejected in favor of shipping in the final home.
- **Every re-raise rings.** Maximum fidelity, guaranteed spam from
  flapping sources; incompatible with a lane whose value is that its sound
  is always worth attending.
