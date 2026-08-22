# ADR-0032: Email-sourced standing questions are answered by the agent arm

**Status:** accepted · 2026-08-21
**Context:** the SCPS (Sierra Club Photo Section) grilling session of
2026-08-21 — a tenth standing question, "when is the next photo-section
event, and what is this month's Photo Quest", whose only reliable source is
the section's own emails (one sender, several emails per event, free text
that is *usually* but not always formatted alike). Extends
[ADR-0029](0029-an-openclaw-agent-is-a-third-interactive-arm.md) (the agent
gains a fifth skill and, for the first time, a mail ingress of its own) and
[ADR-0031](0031-calendar-write-is-gated-on-a-token-id-not-a-scope.md) (the
calendar write it minted becomes the write half of a standing-question
lane); amends [ADR-0015](0015-the-standing-question-read-contract.md) (the
question itself, written inline there under rule 2 by the slice that ships
the pane); narrows `docs/openclaw.md`'s "there is no Google credential on
this machine". Rejects, for this source, the poller shape every other
externally-fed question uses (ADR-0011 / `server/city-waste` /
`server/race-poll`), and says exactly when to reopen that.

## The problem

Every externally-fed standing question so far is answered by an
out-of-process poller that reads a **known format** against a saved fixture
— council HTML, Jolpica JSON, the Gmail API's own metadata — and writes a
`context_snapshots` row under a frozen source. The property every one of
those crates asserts in its header is that everything decidable is natively
tested and only the HTTP edge is not.

SCPS emails do not have a known format. "Next activity is the tide pools,
Saturday the 12th, we'll meet at 8 — oh, and it moved to the 19th" is
semantic extraction, and a month's Photo Quest ("Reflected Light, brought
to you by Kathryn") is a phrase, not a field. A model has to read them.

That forecloses the obvious shape. A GitHub Actions poller cannot reach a
model: CLAUDE.md's blast-radius rule keeps `RUNNER_BEARER_TOKEN` and every
`device` token out of Actions, so neither the runner (ADR-0018) nor the
authority's proxy is reachable from there, and an `ANTHROPIC_API_KEY` in
Actions would be a credential class this repo has never placed there. The
poller would also be the repo's first MIME-body decoder (`gmail-poll` keeps
only Gmail's `snippet`, by design) and its first crate whose correctness
cannot be fixture-tested.

Meanwhile the agent (ADR-0029) already *is* a model with a calendar write
(ADR-0031) and a device token, and OpenClaw ships `openclaw webhooks gmail`
(Gmail Pub/Sub push via gogcli) and `openclaw cron` — a mailbox of the
agent's own can wake it on each incoming mail with no operator gesture.

## Decision

**An email-sourced standing question is answered by the agent arm: the
operator auto-forwards the source's mail to a mailbox the agent owns, a
Gmail push hook wakes the agent, the agent extracts, and it writes the
answer through deterministic skill verbs into places the pane already
reads — the operator's Google Calendar for events, a stamped `settings`
binding for a phrase-valued fact. No poller, no new source, no ingest
token, no snapshot.** The pane is then ordinary ADR-0015 work over the
calendar arm plus one binding.

Seven parts, each decided in the session:

1. **The ingress is a mailbox the agent owns, not the operator's.** The
   operator's Gmail auto-forwards the sender's mail to it; `openclaw
   webhooks gmail` watches it and wakes the `hummingbird-agent` with the
   mail. The credential that reads it is a Google credential for *that
   mailbox only*, held on the gateway, and is the narrowing of ADR-0031's
   "no Google credential on this machine": it reads an inbox that only ever
   holds forwarded club mail. Its worst case is a wrong meeting date. The
   same mailbox is the intended ingress for every later email-sourced
   question — that is the reusable pattern this ADR exists to name.

2. **Events land on the operator's primary calendar under a title
   convention, which is the pane↔writer contract.** Three kinds, three
   prefixes: `SCPS Meeting: <topic>`, `SCPS Activity: <topic>`,
   `SCPS Happy Hour[: <topic>]`. Board meetings are not written. `TBD`
   entries create nothing. The pane selects by the `SCPS ` prefix on the
   calendars the device already polls. A dedicated calendar with a
   `trips-calendar`-style forced binding was offered and declined: the
   operator wants these on the calendar they look at, and the binding
   plumbing it would cost across three clients buys nothing the prefix
   doesn't.

3. **Dedupe is by month + kind (+ topic), never by day.** Activities move,
   sometimes more than once before they happen, so a same-day lookup forks
   the event exactly when it matters. The skill's verbs are `list YYYY-MM`
   (every `SCPS ` event that month, with ids), `add`, `update <id>`, and
   `quest`; the agent lists the months an email covers, updates an existing
   event of that kind/topic, and adds otherwise. Two activities in one
   month (`Activity #1` / `#2`) are two topics, so two events. Event ids
   come from `list` and nowhere else — `gcal.sh`'s own rule. The verbs
   **call** `gcal.sh`; the frozen event-id recipe is not copied.

4. **The phrase-valued fact is a `settings` binding with the month in the
   value.** `scps-quest` holds `Text` of the shape `YYYY-MM <phrase>` —
   first whitespace-delimited token is the month, everything after it
   (trimmed, any characters but a newline) is the phrase. A pane sees a
   binding's value and nothing else (no `updated_at` reaches
   `BindingValueFact`), so the month *must* travel in the value for the
   pane to know the quest is this month's. It stays `Text` so the Settings
   binding editor can still hand-edit it; a JSON object would arrive as
   `Other` and be display-only. This is the first binding an agent writes
   rather than the operator — `PUT /api/settings/:key` under CAS, from the
   agent's device token.

5. **The pane's evidence of freshness is the calendar and the month token,
   not a `fetched_at`.** There is no snapshot, so ADR-0015's `2 × cadence`
   staleness rule has nothing to measure. The pane bands on the next `SCPS `
   event (in progress or today → `live`; tomorrow → `imminent`; within 7
   days → `near`; beyond → `distant`; none → `dormant`), and shows the quest
   only when its month is the current civil month — otherwise "none posted
   for <month>; last: <phrase> (<month>)". The quest never moves the band.
   A stalled lane therefore shows itself as an empty month, not as a stale
   badge: weaker than a red Actions run, and accepted.

6. **Start time is the official start**, with the get-together / set-up
   time in the notes; location → the event's `where`; default durations 2h
   (meeting), 3h (activity), 1h (happy hour) unless the mail says otherwise.
   Happy hours are *not* a standing recurring event, because they do not
   happen every month — one is written only when a mail announces one.

7. **The record is this ADR plus an inline ADR-0015 amendment** for the
   question (rule 2 of `docs/adr/README.md`, the way #675 did it), pointer
   amendments in ADR-0029 and ADR-0031, and a `docs/openclaw.md` runbook
   section for the mailbox, the forward rule and the hook.

## Rejected alternatives

- **A poller with a model call** (`server/scps-poll`, Gmail `from:` query,
  MIME decode, schema-constrained extraction, `scps/v1` snapshot). Fully
  automated, self-monitoring, natively tested around the model — and the
  shape the repo knows. Rejected on the credential: it needs either a
  model key in Actions (a new class there) or Fly-hosting the poller next
  to the sweeper; and it would be the first crate whose decidable core
  cannot be fixture-tested. **Reopen trigger:** if the gateway mail ingress
  cannot be kept alive unattended (Google expires a Pub/Sub watch in seven
  days; renewal is the hook's job, and it must be proven on the gateway
  before the pane ships), or if the operator withdraws the per-mailbox
  credential from the gateway, this is the fallback, and parts 2–6 above
  port to it unchanged — the snapshot body would carry the same kinds and
  the same month-token.
- **Forwarding each mail to the agent by hand** (Telegram). Automated only
  in name; a question whose freshness depends on the operator forwarding
  several emails per event is one the operator stops maintaining.
- **The club's website.** A known format, but the operator declined to put
  a bot crawl against a webmaster who objects to them.
- **The club's calendar.** Exists, is not kept scheduled.
- **A dedicated `SCPS` calendar + forced binding** (part 2's alternative),
  **a fixed title with the topic in notes** (would have made the event id
  `calendar + start` and let the day-keyed dedupe hold — declined for a
  topic in the title, which is why part 3 keys on month + kind instead),
  **the quest as an all-day first-of-month calendar event** (no plumbing,
  but a second title convention on the operator's calendar), **the quest as
  an item** (it is not work to do).
- **Amending ADR-0015 and ADR-0029 in place only.** The reusable pattern
  would have no home and the credential narrowing would be buried.

## Consequences

- **A second Google-credentialed surface on the gateway.** `docs/openclaw.md`
  gains the mailbox and its credential as a named, revocable thing beside
  the device token; its blast radius is one forwarded-mail inbox. CLAUDE.md's
  credential section is unchanged — this is not an authority credential —
  but the openclaw page's "one credential file on this gateway" sentence is
  no longer true and is rewritten by the slice that provisions it.
- **The pane is a calendar-arm pane with a binding**, in the weekend/vacation
  family, not the waste/race family: `sources: []`, a `calendarRequests`
  window over the standard horizon, `BindingKey::ScpsQuest`. It depends on
  the device polling its primary calendar — a device that unticks it sees
  the pane dormant, which is a true statement about that device.
- **Extraction correctness lives in a charter, not a test.** What is
  tested is everything around it: the verbs' argument validation, the
  month-token parse, the prefix match, the bands. The agent can ask the
  operator in chat when a mail is genuinely ambiguous — the one thing a
  poller could never do.
- **A second email-sourced question costs a charter section, a skill verb
  or two, and a pane.** That is the reuse the operator asked for, and the
  reason the mailbox is the agent's rather than a per-question address.
