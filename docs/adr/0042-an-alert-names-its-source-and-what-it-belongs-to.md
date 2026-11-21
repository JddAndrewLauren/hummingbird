# ADR-0042: An alert names its source and what it belongs to

**Status:** proposed · 2026-11-21, accepted when the PR that adds it merges
**Context:** an alert from another app, Auk first, reaches the phone with a
title and a body and nothing else. Nothing on it says which app sent it or
what it belongs to, every alert wears the same bell, and two alerts about one
thing sit as two unrelated entries. The in-app list shows the raw source
string and key (`auk/v1 · outcome%2Fout-412%2Fstate`). The alert lane is
[ADR-0012](0012-the-notification-lane.md) (the data-only push) and the source
registry is [ADR-0014](0014-occurrence-identity-and-the-source-key-conventions.md)'s;
this ADR adds to both and amends neither's decisions. `CONTEXT.md` gains
**Context line** and **Thread**. The operator approved the plan in the
2026-11-21 session that opened issue #922, with JddAndrewLauren/Auk#480 as its
first consumer.

## The decision

1. **The registry holds the source's display name and icon.** Each
   `SourceEntry` gains `display_name` and `icon` (a slug), both non-empty
   (`auk/v1` is "Auk" with icon `auk`, `perch/v1` is "Perch" with `perch`).
   They live in the registry, not in the payload, so **no token can claim
   another source's name** and a source cannot upload an icon. A client
   draws the slug it has a drawable for and the bell for any it does not, so
   a new slug can ship server-side before the phone knows it. A source the
   registry does not know shows its raw source string and the bell.

2. **Two optional ingest fields, both source-owned.** `context` is one short
   line saying where the alert belongs (Auk sends "<project> · <Outcome id>", such as "hummingbird · out-412"); `thread_key`
   groups a source's alerts (Auk sends "out-412"). Each is non-empty when
   present. Both are stored on the alert (`alerts.context`,
   `alerts.thread_key`, schema 17) and, like every other field the source
   owns ([ADR-0009](0009-the-owned-schema-and-context-lanes.md) rule 3), set
   absolutely on every write: a write that omits one clears it. A payload
   that sends neither is accepted unchanged.

3. **Caps on the two new fields, counted in characters.** `context` at most
   80 and `thread_key` at most 120. Past one is a 400 naming the field, and
   nothing is written. The lengths bound what a notification header and a
   group key can sensibly carry. `title` and `body` get **no** cap: they hold
   third-party text (a mail subject, a calendar title), and refusing a long
   one would lose the alert. The caps are on the ingest door (`POST
   /api/alerts` and the Healthchecks receiver, which enters through it).

4. **The thread is scoped to the source.** The authority's thread value is
   `<source>:<thread_key>` (`auk/v1:out-412`): two sources that use the same
   key never merge. A registered source string carries no `:`, so the join is
   unambiguous. The thread is derived (`Alert::thread`) and never stored.

5. **The push carries four more strings.** The data map gains `source_name`
   and `source_icon` (always, from the registry) and `context` and `thread`
   (only when the alert has them, absent rather than empty, as `body` is).
   This is ADR-0012's field set as amended by ADR-0027, plus these four.

6. **The Android notification reads them.**
   - The header subtext is "<source name> · <context>", or the source name
     alone with no context.
   - The small icon is the source's drawable, the bell for an unknown slug.
   - Alerts sharing a thread stack under one group summary, keyed by the
     thread. The summary goes away when the last alert of its thread is
     cancelled or acked, so it never outlives its children.
   - An alert with no thread is a notification of its own, as before.
   The watch inherits all of it: its notifications are bridged from the
   phone's, which carry the group, the subtext and the icon.

7. **The alert list and detail name the source by its display name and show
   the context line**, in place of the raw source string and key. The
   client reads the name from the registry it already links
   (`hummingbird_domain::find_source`), so it needs no new synced field for
   it.

## Considered and not taken

- **A source sending its own display name or icon.** A token could then
  present itself as another source, which is exactly what a bound token
  exists to prevent. Not taken, and out of scope per the issue.
- **One field for both the line and the thread.** The line is prose a person
  reads and changes as an alert's state does; the thread is an identity that
  must stay put. Merging them would make every edit to the wording split the
  stack.
- **A client-built thread** (grouping on `source_key`, or on a prefix of it).
  `source_key` is occurrence identity and is never parsed
  ([ADR-0015](0015-the-standing-question-read-contract.md)); a thread is a different grouping
  and only the source knows it.

## Consequences

- Hummingbird deploys before Auk. A payload with neither new field is
  unchanged on every path, apart from the visible name and icon.
- Deploy: the Fly runner needs nothing; the authority is a Cloudflare Worker
  deploy (its schema grows by two `ALTER TABLE`s on first boot), then an
  Android release.
