# HANDOFF — land the #533 pane probe

**Written 2026-08-18. Branch `issue-533`, pushed to origin, no PR yet.**

## Goal

Get M4's gating probe (#533) reviewed and merged, which means answering the one
question it was built to answer: **is the two-phase zone bridge tolerable for
the remaining seven panes?** #534 and the whole pane lane are blocked on that
verdict. The code is done and green; what is missing is a decision and a PR that
records it.

## State

- `3826f3e` — the probe itself (previous session): pane contract, cross-pane
  sort, zone bridge, waste pane sunk end to end, web rewired.
- Working tree carries **two uncommitted doc edits** from the post-review
  session (see below). They are not committed — commit them with a `docs:`
  message as part of opening the PR, or fold them into the probe commit.
- Verified green: `cargo clippy --all-targets -- -D warnings` and `cargo test`
  in `client/core` (596 + 22). Web suite (1,979) was green at review time and
  nothing since then touches executable code.
- **Do not run `cargo fmt --all`** — from `client/` it escapes into `server/`
  via a path dep. fmt is not a CI gate here; `clippy -D warnings` is.

## The missing decision (this is why it's a session, not a batch)

#533 carries an explicit **operator checkpoint on the zone bridge's
ergonomics**. Two phases — the core names every `(zone, civil-date)` fact it
needs, the host resolves them, the core decides — worked cleanly for waste. The
question is whether that holds across seven more panes, several of which are
harder (the race pane's per-subject reasoning, the weekend pane's two arms, the
reachability grace window over sync history).

The issue names the flip if the answer is no: **bring a tz library into the core
via an ADR-0025 amendment**. That is a real trade — it reverses the "core owns
no tzdb" position that ADR-0025 and `panes/mod.rs` both rest on, and it would
change what `server/`'s wasm32 thinness rule implies for the client core. It
needs you, not an agent.

Read before deciding: `client/core/src/decisions/panes/mod.rs` (header, then
`zone.rs`), and ADR-0025's `## The zone bridge, fixed by M4's probe`.

## Work items

1. **Open the PR** (`gh pr create --base main`). It must carry three things,
   all of which exist only in the previous sessions' context:
   - the ergonomics verdict above;
   - the AC deviation, drafted verbatim below;
   - the facts-seam growth cost (already written into `panes/mod.rs`'s header
     and appended to #534's body — the PR should reference, not restate).
2. **Commit the two working-tree doc edits** (or fold them into `3826f3e`).
3. Optional heavier gate before merge: `/code-review` — the probe commit came
   through no to-goal batch, so it carries no FINAL-GATE exemption.

## Drafted PR text — the AC deviation

> **Deviation from an acceptance criterion, stated rather than hidden.**
> "Existing web pane tests pass unchanged" does not hold for one assertion.
> `parseWasteBody` used to refuse `Mars/Olympus_Mons` by calling
> `zonedMidnightMs`; the core owns no tzdb and cannot, so its parser now answers
> about the payload's *shape* only and that case asserts `ok`. The refusal did
> not vanish — it moved to the answer level, with the same words, pinned by
> `waste.test.ts`'s "refuses a zone this runtime cannot resolve, with the same
> words as before". Keeping the old result in the helper was considered and
> refused: it would need `Intl` back in the parser (the host judgement this
> probe exists to remove) or a signature change, either of which breaks
> "unchanged" harder than the test edit does.

## What the post-review session already settled (don't relitigate)

A review raised three findings. Verified against HEAD: one agreed, two agreed on
the claim but rejected the remedy.

- **Agreed and applied** — ADR-0015 had an inline amendment block that
  `docs/adr/README.md` rule 1 puts in the amending ADR. Moved to a Status-header
  pointer at `ADR-0025#the-zone-bridge-fixed-by-m4s-probe`.
- **Rejected remedy** — "add a tagged `PaneFacts` union to `RankedPaneRecord`".
  `rank_panes`/`rankPanesFromCore` have no production caller, `SUNK` has one
  arm, and the AC as written ("structured facts, no sentences") is met. Shaping
  a union from a single arm is the trap a probe avoids. Handed to #534 instead,
  which has real arms to shape it from.
- **Rejected remedy** — "restore `parseWasteBody`'s zone gap". Barred by the
  same issue's "an unresolvable zone is handled by a core decision, not a host
  fallback". Recorded as the deviation text above.

## Files to load

- `client/core/src/decisions/panes/{mod.rs,zone.rs,waste.rs,contract.rs}`
- `client/web/src/screens/waste-pane/waste.ts`,
  `client/web/src/screens/questions/zone-bridge.ts`,
  `client/web/src/decisions/seam.ts`
- `docs/adr/0025-…md` (the zone-bridge section), `docs/adr/0015-…md` (header)
- Issues #533 (this probe), #534 (the seven panes), #527 (M4's governing rules)

## Loose thread

`rankPanesFromCore` and `parseWasteBody` have no production callers — reachable
only from tests. Expected mid-probe; already written into #534's body as
"consume or retire both". Not a blocker for merging #533.
