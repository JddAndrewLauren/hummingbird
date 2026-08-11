# Handoff — `page.rs`, the last slice of #120

## Goal

Make `server/city-waste/src/page.rs` read the council's collection page into
a `PageReading`. It is the **only** unbuilt piece of #120; everything around
it — the registry entry, the snapshot write path, the alert lane, the
poller's judgement, the workflow — is built, tested and merged onto
`issue-120`.

## Why this needs a fresh session with you in it

It is blocked on one input only you can supply — **the city page URL, or a
saved HTML sample** — and one of its three outcomes needs a decision that
reaches into the client. That makes it a phase boundary, not a task.

## Current state

Branch `issue-120`, 9 commits ahead of `origin/main`, clean tree, no PR open
yet.

Done and verified: 262 native tests green, `cargo clippy --workspace
--all-targets -- -D warnings` clean, the wasm32 worker build unaffected,
`actionlint` clean, and `server/scripts/smoke.sh` passing end-to-end against
a real `wrangler dev` Durable Object including the new routes.

`page::parse` returns `PageError::NotImplemented`, so `main.rs` exits 1
without writing anything. Its two fixture tests are `#[ignore]`d (they read
the sample at runtime rather than via `include_str!`, so a missing file does
not break the build).

**Not yet done, and outside this slice:** `CITY_WASTE_INGEST_TOKEN` has not
been minted. It is an `ingest`-scope token bound to `city-waste/v2`, minted
from your terminal against `ADMIN_SECRET`, then set as an Actions secret. The
workflow cannot run without it, and `admin_tokens::mint` will accept
`city-waste/v2` now that the registry carries it.

## The decision that has to be made first

`page.rs` needs a `Cadence` (an anchor date and a period in weeks), because
materiality is deviation from cadence and never a diff against the previous
poll. Which of three shapes the real page has decides the work:

1. **The page states the normal collection day** → observe it directly.
   Best case; nothing else changes.
2. **The page lists several upcoming dates** → derive anchor and period from
   their weekday and spacing, fixture-tested. Fine; nothing else changes.
3. **The page gives only the next date** → the cadence cannot come from the
   page at all and must come from a second binding value. That widens
   `city-waste-page` from a URL to a JSON object and reaches into
   `client/web/src/screens/bindings.ts`.
   **Escalate before building this one** — it would make the delivered client
   half a lie about being complete, and it is a design decision rather than
   an implementation detail.

## What to load

- `server/city-waste/src/page.rs` — the module header states the contract
  `parse` owes the rest of the crate, and the three shapes above.
- `server/city-waste/src/lib.rs` — the map: where the split falls, and why
  this crate is out of process and out of the wasm32 build.
- `server/city-waste/src/judge.rs` — why the cadence is needed at all. Read
  this before deciding what `page.rs` may return; the "no previous snapshot"
  property is the design's load-bearing choice.
- `.context/plans/120-the-server-half-of-the-which-cans-lane.md` — the
  original plan, whose steps 1–7 are all landed.

## Gotchas learned in the session that built the rest

- **Nothing may make an alert's `title` or `body` clock-dependent.** The
  authority restamps `raised_at` on any changed source-owned field, and
  `is_live` compares that stamp against `dismissed_at`, so a title that
  moves day to day undoes the reader's dismissal every morning. `alert::plan`
  therefore takes no clock at all. If `page.rs` ever wants to put "this week"
  into words, that belongs on the pane, not in the alert.
- **`page.rs` must not resolve a civil date against the runner's clock.**
  Use `Date::today_in_zone` / `Date::end_of_day_ms`; a naive
  `now_ms / 86_400_000` is the runner's UTC day and silently disagrees with
  the address in the local evening.
- **The body is a cross-language contract with no compiler between the two
  sides.** `server/city-waste/tests/contract.rs` asserts the literal
  snake_case keys against `waste.ts`'s own text. If `page.rs` grows a field
  the pane should read, add it there too or nothing will catch the drift.
- Adding an HTML-parsing dependency is fine here (native binary, workspace
  member) but the crate **must never** become a dependency of
  `hummingbird-authority-worker` — `lib.rs`'s own test asserts that from the
  worker's manifest.

## When it is done

Un-ignore the two fixture tests, commit the sample under
`server/city-waste/tests/fixtures/`, then run the plan's end-to-end check:
`wrangler dev` + `cargo run -p hummingbird-city-waste` with `HB_BASE_URL`
pointed at the local worker, and confirm the pane answers in `client/web`.
