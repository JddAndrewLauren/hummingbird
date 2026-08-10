# ADR-0013: The rule condition vocabulary — a typed catalogue over one Event shape

**Status:** accepted · 2026-08-09 · **amended 2026-08-09 by
[ADR-0014](0014-occurrence-identity-and-the-source-key-conventions.md):**
the invalid-rule flag below extends to *retired sources* — a rule naming a
`source` that has been bumped to a new version is flagged in the UI at load,
never left silently non-firing; and the state/event split ADR-0014 decides
is **not** derivable from the `mints` flag, so it is declared per source
rather than per kind; and `item_threshold`, being a state source, gains the
alert-side resolution pass that lets its alerts end.
**Context:** the condition-vocabulary grilling of 2026-08-09 (issue #132).
Amends [ADR-0012](0012-the-notification-lane.md), which declared conditions
to be `[{field, op, value}]` ANDed field tests and left the operator set
undecided; that gap blocked the rule engine (#133). Amends
[ADR-0009](0009-the-owned-schema-and-context-lanes.md): its provenance rule
4 is generalized into the Event core, and `items.due_date` is renamed and
widened to carry an optional time. New glossary terms (Event, Event kind)
land in `CONTEXT.md`.

## The generalization that came first

ADR-0012 fixed four `event_kind`s behind a `CHECK` constraint and gave each
a hand-written field set. Two things fell out of that during the grilling
and neither was survivable:

- **Pushed sources had no path to a delivery at all.** Home Assistant,
  infra checks and the Gmail alert-label raise alerts directly at
  `/api/alerts`; they never pass through a rule, and rules are what emit
  deliveries. Infra alerts would have landed in the stack and never rung.
- **Every new source cost a migration.** A closed enum plus per-kind
  hand-written fields means a source is rule-*ineligible* until someone
  does that work. Input sources are expected to grow.

ADR-0009 rule 4 already pointed at the fix: *"Provenance is the one shared
shape. Items, alerts and snapshots all carry `(source, source_key[,
source_url])`. That is the entire common core."* This ADR promotes that
common core into what the rule engine sees.

## Decision

### The Event: one shape at the rule engine's door

**Every rule-eligible thing is an Event — a common core, plus whatever its
kind declares.** Mail arriving, a meeting nearing, a snapshot materially
changing, a deadline approaching, an alert pushed in from outside: all the
same shape by the time a rule sees it.

Core fields, present on **every** event from **every** source, always:

| Field | Type | Note |
| --- | --- | --- |
| `source` | string | frozen namespace: `gmail/v1`, `photo-site/v1`, `home-assistant/v1` |
| `source_key` | string | identity within the source |
| `occurred_at` | timestamp | |
| `title` | string | what the notification shows |
| `body` | string? | |
| `url` | string? | deep link back |
| `severity` | string? | source-supplied where one exists |
| `calendar_busy` | bool | the cross-cutting suppressor; see below |

`body`, `url` and `severity` are **present on every event but may be
absent-valued** — ADR-0009 declares all three nullable on `alerts`, and an
`alert_raised` event carrying no body is ordinary, not malformed. They are
optional at the type level and absent at the condition level: evaluation
rule 2 below already makes a missing field's condition false, even negated,
so a null `body` behaves exactly like a kind that never declared one. No
normalization to `""` — that would make `body eq ''` and "no body at all"
the same fact, and would give an unsourced severity a rank.

Every kind has a natural `title`: the subject, the meeting name, the change
itself ("Trash pickup: Mon → Tue" — ADR-0009 rule 2 already requires the
alert state the change), the item title, the alert title.

**The property this buys: a new source is rule-eligible the moment it
exists**, on core fields alone, before anyone writes a single field
descriptor. Declared fields are a refinement, never a prerequisite.

### The kind registry

`event_kind` is a **registry key, not a closed vocabulary** — the `CHECK`
constraint is dropped. Kind is the field-set *family*; `source` is the
instance. A second mail provider costs nothing: same kind, different
`source`. A genuinely new shape costs one registry entry and no migration.

Each registry entry declares its extra field descriptors and one flag:

- **`mints: true`** — raw stream events (mail, calendar, snapshot change,
  item threshold). A matching rule mints an alert and stamps the severity
  it declares.
- **`mints: false`** — the event *is already* an alert (pushed sources at
  `/api/alerts`). A matching rule promotes it to a delivery, mints nothing,
  and cannot restamp severity. `rules.severity` is simply unused for these.
  ADR-0012's "a rule may never mutate an existing record" governs records a
  rule did not mint; the one write a rule makes to a row it did not create
  is ADR-0014's severity ratchet on a `mints: true` alert another rule
  already minted, which is the same occurrence and never a downgrade.

**The registry is a `domain` artifact — code, not a table** — exported as
JSON so the rules UI renders its dropdowns from the same source the engine
evaluates. A data-driven catalogue could drift from what an adapter
actually emits, and the symptom of that drift is a rule that silently never
fires: undetectable in a default-deny lane.

### The `rules` table, amended

```sql
CREATE TABLE rules (
  id         TEXT PRIMARY KEY,
  name       TEXT NOT NULL,
  event_kind TEXT,                -- NULL = any kind, conditions restricted to
                                  --   core fields. No CHECK: open registry.
  conditions TEXT NOT NULL,       -- JSON: [{field, op, value, negate}], ANDed
  severity   TEXT NOT NULL,       -- stamped on the alert this rule mints;
                                  --   unused for mints:false kinds
  tier       TEXT NOT NULL CHECK (tier IN ('urgent','normal')),
  enabled    INTEGER NOT NULL DEFAULT 1,
  updated_at INTEGER NOT NULL,
  version    INTEGER NOT NULL
);
```

A **NULL `event_kind` means any kind** — the "everything is rule-eligible"
guarantee expressed structurally. Default-deny governs *ringing*; it never
makes anything default-*ineligible* to match.

### The typed field catalogue

`field` is drawn from a typed catalogue, never a free string. Types:
`string`, `string_list`, `number`, `bool`, `timestamp`, `date`. Operators
are gated by type, and an unknown field is **rejected at save** — and a
rule whose kind later stops declaring a field it names is **flagged invalid
in the UI at load**, never silently non-firing. ADR-0014 extends the same
flag to a rule naming a `source` that has since been retired by a namespace
bump, for the same reason: in a default-deny lane a rule that stopped
matching is indistinguishable from a quiet week.

The catalogue is what makes the ADR-0012 promise buildable: "each condition
renders as one row in the config UI, phone-editable." A field dropdown, then
a dropdown of only the *legal* operators, then the right value widget.

**`email`** (sources: `gmail/v1`, `m365-mail/v1`)

| Field | Type | Note |
| --- | --- | --- |
| `from` | string | raw header, so `contains` hits address *or* display name |
| `to`, `cc` | string_list | |
| `subject` | string | |
| `body` | string | plain text, first 64 KB |
| `labels` | string_list | Gmail labels; Graph categories map here |
| `received_at` | timestamp | |
| `has_attachment` | bool | |

**`calendar_event`**

| Field | Type |
| --- | --- |
| `calendar` | string |
| `title`, `organizer`, `location` | string |
| `attendees` | string_list |
| `starts_at`, `ends_at` | timestamp |
| `is_all_day` | bool |
| `response` | string — `accepted` / `declined` / `tentative` / `needs_action` |

**`item_threshold`** — the DO alarm sweeping items already in the authority.

| Field | Type | Note |
| --- | --- | --- |
| `deadline` | timestamp | day-only values resolve to 23:59 local |
| `scheduled_date` | date | `d` units only |
| `title`, `stage`, `size`, `energy`, `context` | string | |
| `priority` | number | 0–4 |
| `project` | string | stores the id, UI renders the name — a project rename must not silently break a rule |

Archived items are excluded from evaluation entirely rather than exposed as
a field.

**`snapshot_change`** — ADR-0009 rule 2's material changes.

| Field | Type | Note |
| --- | --- | --- |
| `key` | string | the metric within the source |
| `changed_at` | timestamp | |
| `value`, `previous` | **declared per key at wiring time**: `number` \| `date` \| `timestamp` \| `string` | ops narrow once source and key are picked |

Value types are declared per key, **not fixed to numeric**. The flagship
scenario — a holiday sliding trash pickup Monday → Tuesday — is a *date*
change; a numeric-only constraint would have made it rule-ineligible.

That scenario needs **no value condition at all**: `source eq 'city-waste/v2'`
*(the source's live version — `v1` is retired, [ADR-0014](0014-occurrence-identity-and-the-source-key-conventions.md))*,
tier urgent. ADR-0009 rule 2 already places the materiality judgment in the
source's wiring ("what counts as material is defined per source, at wiring
time — a weekly 'next date' rolling forward on cadence is not a change"). For
`snapshot_change` a rule's job is routing and tiering; value conditions stay
available for the cases that want them (`value gt 0.8`).

**`alert_raised`** (`mints: false`) — pushed sources at `/api/alerts`. Core
fields carry it entirely; ingest sources may declare more.

### The operator set

| Type | Ops |
| --- | --- |
| `string` | `eq`, `contains` |
| `string_list` | `eq`, `contains` — **any element** satisfies it |
| `number` | `eq`, `gt`, `lt` |
| `bool` | `is` |
| `timestamp` | `within_next`, `within_last` (`m` / `h` / `d`) |
| `date` | `within_next`, `within_last` (`d` only) |

- **A list `value` means any-of.** `subject contains ["urgent","asap","eod"]`
  is one condition, rendering as a chip input. OR *within one field*; OR
  across fields is still a second rule (ADR-0012).
- **List fields take the same two string ops**, applied to any element:
  `to contains '@twinion.net'` matches if any recipient's address contains
  it; `labels eq 'alert-high'` matches if some label is exactly that. A
  single word meaning "substring" on a string and "exact membership" on a
  list would be a silent surprise; this way both mean "apply the string op,
  to any element."
- **Negation is a `negate: bool` flag**, not mirrored `not_*` operators.
  Half the vocabulary, and one "not" toggle in the condition row.
- **All string comparison is case-insensitive.** A `Matt@` vs `matt@`
  mismatch has no legitimate use and its failure is invisible.
- **No regex, no `starts_with` / `ends_with`.** `contains '@twinion.net'`
  already covers domain matching, and it survives the `Name <a@b.com>`
  display-name format that `ends_with` breaks on.
- **No word boundaries.** `contains 'invoice'` also matches "invoiced". If
  false positives demonstrate, a `word` op is a later vocabulary extension.

### Relative time

**Two operators, direction stated, never inferred from the field name.**
`deadline within_next '2h'`, `received_at within_last '10m'`,
`starts_at within_next '15m'`. Inference is invisible magic in a UI whose
point is that each row reads as a sentence; signed durations (`'-10m'`)
fail the phone-screen test.

- **Duration literal:** integer + unit, units `m` / `h` / `d`. No compounds
  (`1d2h`). Parsed in `domain`, rejected at save if malformed.
- **`within_next D` means `t ≤ now + D` — unbounded on the past side.** An
  overdue item still matches. The bounded reading (`now ≤ t ≤ now + D`) is a
  silent-failure machine: the rule quits exactly when the thing became most
  urgent. "It matches forever" is not a leak — the alert is minted once and
  dedupe absorbs every later tick (ADR-0012), so a permanently-overdue item
  produces one alert that sits live until acked or resolved.
- **Accepted cost: first-tick storm.** Saving a new `item_threshold` rule
  matches every already-overdue item on the next alarm. Mitigated by
  ADR-0012's backtest action, which for this kind is a query against the
  authority itself — no source re-fetch.
- **`date` fields take `d` units only.** `m` and `h` are rejected at save;
  the field has no such precision and a rule that quietly rounds is the
  silent-failure mode again.
- **One timezone, Worker-configured.** "Today" must be the operator's today;
  the Worker runs UTC. Not per-rule, not per-device.
- **The DO alarm interval is the precision floor.** `within_next '5m'` on a
  15-minute tick fires up to 15 minutes late. The rules UI warns when a
  duration is under the tick; it does not reject, since the interval can
  change.

### Evaluation rules

1. **Conditions are ANDed** (ADR-0012). OR is a second rule.
2. **A missing field makes its condition false — even when negated.**
   Otherwise `labels not-has 'x'` fires on every event that has no labels at
   all, and a default-deny lane leaks through absence.
3. **`calendar_busy` is the one exception**, and it is a *default value*
   rather than an absent field: missing or stale busy state resolves to
   `false` (not busy). Under rule 2, a failing calendar poller would make
   every calendar-conditioned rule silently stop firing — and in a
   default-deny lane silence is indistinguishable from correctness. Failing
   toward "not busy" costs extra rings, never missed ones. The staleness
   surfaces on its own: "staleness is its own alarm" (ADR-0002, `CONTEXT.md`).

### Calendar-busy: a snapshot, not a stream

ADR-0012 said calendar state is available as a condition field on any rule.
ADR-0011 said non-matching stream data is never written. Between them,
busy-ness had nowhere to come from. Resolved:

**The calendar poller writes one `context_snapshots` row**
(`google-calendar/v1` / `busy_now`) holding the **current busy window's
boundaries**, replaced wholesale each poll. This is the existing snapshot lane doing exactly what
ADR-0011 says it is for: "snapshots are state that is *kept*, streams are
events that are *judged*." Rules read it from DO storage — no API call in
the evaluation hot path.

Storing the *window* rather than a boolean is the point: a 15-minute-old
snapshot still yields a correct answer, because the engine compares `now`
against the boundaries instead of trusting a stale flag.

What counts as busy:

| Signal | Busy? |
| --- | --- |
| Timed event in progress (`start ≤ now < end`, matching the existing exclusive-end rule in `client/core/src/calendar/query.rs`) | **yes** |
| Marked free / transparent | no |
| Declined by the operator | no |
| All-day event | **never** |
| Any polled calendar | counts |

Every "no" exists to prevent **over-suppression** — silence the operator
never asked for. All-day is the deliberate one: all-day entries are mostly
day-labels (birthdays, holidays), and one of them silently muting a day of
urgent notifications is that failure exactly. Wanting quiet for a week is a
*global mute* — a different feature, reached for deliberately — not a side
effect of a calendar entry.

It is a **field, not a distinct condition type.** Once busy-ness is a
snapshot it is just data with a value, listed in the catalogue for every
kind. A tagged-union condition shape would buy nothing and cost a
discriminator in the JSON plus a second row renderer in the UI — and every
future cross-cutting predicate would then want its own type too.

### What ADR-0009 must change: deadlines gain a time

`item_threshold`'s motivating example — "due within 2h" — was unbuildable:
`items.due_date` is an ISO **date**, with no hour to compare against. The
field is renamed and widened rather than the example being narrowed.

- **`items.due_date` → `items.deadline`**, accepting `2026-08-15` *or*
  `2026-08-15T14:30`. One column, no new column, no `SCHEMA_VERSION` bump:
  ISO-8601 sorts lexicographically, so the existing raw-string sort in
  `client/core/src/task/query.rs` stays correct **across** days.
- **Within one day the raw sort is wrong and must resolve first.** A
  date-only value sorts lexicographically *before* any timed value on the
  same day, while a day-grained deadline means end of day (23:59, below) —
  so `by_priority_then_due` would rank "sometime Saturday" above "Saturday
  14:30". The comparison resolves day-only to 23:59 before comparing, the
  same rule the conditions use. One comparison key in `domain`, shared by
  the sort and the evaluator; still no schema change. Left raw, the display
  order would contradict the evaluation order on the same pair of rows.
- **Minute precision, no seconds.** A deadline to the second is fiction.
- **Naive local time, no offset suffix**, resolved in the Worker-configured
  timezone. An offset suffix would break the lexicographic sort the moment
  two zones mix.
- **Format validation added in `domain`**, rejecting anything else. New
  behaviour — today any string is accepted — and required, because an
  unparseable deadline makes a rule silently never fire. Free now: nothing
  is deployed and no rows exist.
- **Day-only deadlines resolve to 23:59 local** when an hour-grained
  condition tests them, so `deadline within_next '2h'` is well-defined for
  every row.
- The rename is a **wire field rename**, also free only now. `scheduled_date`
  keeps its name: it genuinely stays a date. The glossary term becomes
  **Deadline**, which is the word `CONTEXT.md` was already using in the
  definition of the old one.

## Rejected alternatives

- **Free-string field names, everything compared as text.** No dropdowns to
  render, and a typo becomes a rule that silently never fires — the worst
  failure mode in a lane whose value is that its silence is trustworthy.
- **A closed `event_kind` enum with hand-written per-kind fields**
  (ADR-0012 as written). Left pushed sources with no path to a delivery and
  made every new source a migration. The Event core replaces it.
- **A registry table instead of a `domain` artifact.** Config-driven
  extensibility whose failure mode is catalogue-vs-adapter drift, expressed
  as rules that never fire.
- **Regex, or `starts_with` / `ends_with`.** The DSL ADR-0012 rejected,
  arriving through the side door; `contains` covers the real cases and
  handles display-name formats that `ends_with` does not.
- **`has` as a distinct list-membership operator.** One word meaning
  substring on strings and exact membership on lists: `labels contains
  'alert'` matching `alert-low` would be a silent surprise.
- **Mirrored `not_*` operators.** Doubles the vocabulary to express what one
  toggle expresses.
- **Bounded `within_next` (`now ≤ t ≤ now + D`).** Overdue items stop
  matching — the rule quits precisely when it matters most.
- **Direction inferred from the field** ("due is future, received is past"),
  or signed durations. Both unreadable on a phone.
- **Keeping `due_date` as a date and narrowing the example to "due today."**
  Defensible, and rejected: a day-grained deadline cannot express a
  time-of-day consequence the operator actually has, and the rename was free
  exactly once.
- **A separate `due_time` column, or converting the deadline to an INTEGER
  timestamp.** Two columns for one concept invites the reader who forgets to
  combine them and silently gets midnight; an integer forces a time onto
  every deadline, destroying the deliberate day-grain ADR-0009 chose.
- **Live-querying the calendar API at evaluation time.** An API call in the
  poll's hot path, whose outage fails either toward spurious rings or —
  worse — toward silence.
- **Calendar-busy as a distinct condition type.** A discriminator and a
  second UI renderer, and a precedent every future cross-cutting predicate
  would follow.
- **All-day events marking the operator busy.** One "Sam's birthday" muting a
  day of urgent notifications.
- **JSON-path access into `snapshot_change` payloads** (`payload.foo.bar`).
  The rejected DSL wearing a different hat. Declared value types per key
  cover the demonstrated cases.

## Deferred, not rejected

- **Timezone-aware deadline stamps.** v1 stores naive local, resolved in one
  Worker-configured zone. Reopen when a deadline must survive the operator
  changing zones, or a second zone or person enters the system.
- **Word-boundary matching** (`word` op) — a vocabulary extension if
  substring false positives demonstrate.
- **Richer calendar fields** (`calendar_busy_title`, for "not while I'm with
  Matt"). The snapshot payload can carry it; adding the field later is a
  catalogue addition, not a migration.
- **Non-numeric, non-declared snapshot payload access** — see JSON-path
  above; reopen if a source's shape resists a declared value type.
