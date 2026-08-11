# `server/race-poll` fixtures

JSON carries no comments, so the headers these two files would otherwise
have live here.

## `jolpica-current.json`

**The real response, verbatim, and that is the opposite of
`server/city-waste/tests/fixtures/`'s policy — deliberately.** Those fixtures
are reduced and sanitised because the council's page carried the operator's
home address and ~95 KB of per-request CSRF tokens and signed JWTs, and this
repo is public. This is public sports data fetched from an unauthenticated
endpoint with no credential of any kind:

```
GET https://api.jolpi.ca/ergast/f1/current.json
```

captured 2026-08-11, 14 KB, the 2026 season, 23 rounds. **Do not sanitise,
trim or re-order it by cargo-cult** — the point of a verbatim capture is that
`schedule.rs` is tested against exactly what the feed sends, including the
two ladder shapes it publishes (conventional FP1/FP2/FP3/Qualifying ×17, and
sprint FP1/SprintQualifying/Sprint/Qualifying ×6, both present here) and the
fields this adapter deliberately drops.

Re-capturing it is a real decision, not a refresh: it moves
`golden-body.json` with it, and that file is a contract with another PR.

## `golden-body.json`

The exact `context_snapshots.payload` `race-schedule-poll` writes for
`jolpica-current.json` — ADR-0015's `{schema, polled_every_ms, body}`
envelope around the `race-schedule/v1` body — pinned byte for byte by
`tests/golden.rs`.

It exists because **the body's consumer does not exist yet**: #119's pane
parser is a separate slice, so nothing else could catch this shape drifting.
#119 writes its parser against this file and adds its own `contract.rs`
pointing at it, the same role `waste.ts`'s text plays for the which-cans
lane.

**Regenerating it to make a failing test pass is the one thing that defeats
it.** A diff here is a decision to take with that consumer.
