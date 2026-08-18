# hummingbird

Personal GTD-style task system. The task authority **is** the app-owned server
(ADR-0008), live at `hb.twinion.net` since 2026-08-10 (#237). Every skill here
targets it with a `device` token from `~/.config/hummingbird/api-token`;
**nothing in this repo calls Linear any more** — `sweep.py` was the last one,
retargeted to `POST /api/items` with a `sweeper`-scope `HB_API_TOKEN` in #123.

## How to read this repo

**This file is a map, not a description.** Every component below documents
itself at its point of use — a module header, an ADR, or an area doc — and
those are canonical. Restating any of it here creates a second copy that
drifts; the section on a thing is one or two lines and a path.

Before writing code: `CONTEXT.md` is the domain glossary (its terms adjudicate
design questions — check work against them), and `docs/adr/` holds the
decisions (its `README.md` states how an accepted ADR is amended). Read the module header of whatever you are about to change before
grepping it.

## The map

| Component | Where it lives | Read first |
| --- | --- | --- |
| The capture sweeper (live since 2026-08-12) | `sweep.py`, `crontab`, `fly.toml` | `docs/sweeper.md` |
| The skill runner | `runner/` | `docs/runner.md` |
| The owned schema + wire DTOs | `server/domain/` | `src/lib.rs`, ADR-0009 |
| The authority server | `server/authority/` (+ `worker/` wasm32 shim) | `src/lib.rs`; DDL and its migration traps in `src/schema.rs` |
| Rule evaluation | `server/rules-engine/` | `src/lib.rs`, ADR-0013 |
| The notification lane | `authority/src/{delivery,sweep,fcm}.rs`, `worker/src/fcm.rs` | `delivery.rs`, ADR-0012/0014 |
| Grill (interview + the immutable per-item attachment) | `domain/src/grill.rs`, `authority/src/handlers/grills.rs` | `grill.rs`, ADR-0023 |
| The which-cans poller | `server/city-waste/` | `src/lib.rs`, then `src/judge.rs` |
| Evaluated-stream pollers | `server/{gmail-poll,calendar-poll,graph-poll}/` | each `src/lib.rs`, ADR-0011 |
| The race lane (2 binaries) | `server/race-poll/` | `src/lib.rs` |
| The client sync engine | `client/core/src/sync/` | `sync/mod.rs`, then `sync/cycle.rs`, ADR-0007/0008 |
| The one client API | `client/core/src/lib.rs` | its `Core` docs — seven mutation entry points |
| Ranking / freshness / panes / bindings | `client/core/src/{rank,freshness,pane,bindings}.rs` | each header; ADR-0015 for the Rust/TS carve-out, **as redrawn by ADR-0025** |
| The decisions every client shares (#141/M1) | `client/core/src/decisions/`, `client/ffi-web/src/decisions.rs`, `client/web/src/decisions/seam.ts` | `decisions/mod.rs`, then `seam.ts`; ADR-0025 |
| The `/next-up-hb` seam | `client/next-up/` | `src/lib.rs` |
| The wasm seams | `client/ffi-web/src/{task_host,calendar_host}.rs` | those headers |
| The mobile seam + the Android app (#141, through the item-detail slice) | `client/ffi-mobile/`, `client/android/` | `ffi-mobile/src/lib.rs`, `android/README.md`, ADR-0025 |
| Item detail, and where a tapped notification lands | `client/core/src/{item_detail.rs,decisions/notification.rs}`, `client/android/.../ItemDetail{Screen,ViewModel}.kt` | `item_detail.rs`, then `decisions/notification.rs`; ADR-0027 |
| The rules surface (#141/M4) — the sink, both seams, the Compose screen | `client/core/src/decisions/rules/`, `client/ffi-mobile/src/lib.rs`, `client/android/.../Rules{Screen,ViewModel}.kt` | `decisions/rules/mod.rs`, then `backtest.rs`; ADR-0013/0025 |
| Android's notification lane (#141/M2) | `client/android/app/src/main/kotlin/net/twinion/hummingbird/{notify,push}/` | `notify/NotificationChannels.kt`, `push/AckRunner.kt`; ADR-0012/0014 |
| The web app | `client/web/` | `client/web/README.md` |
| The SharedWorker layer | `client/web/src/worker/` | `core.worker.ts` (note its top-level-`await` invariant), ADR-0010 |
| The standing-question panes | `client/web/src/screens/questions/` + `*-pane/` | `questions/contract.ts`, ADR-0015 |
| The Status screen (second surface of the ranked region) | `client/web/src/screens/StatusScreen.tsx` (#311) | `questions/contract.ts`, ADR-0017 |
| Now's centre column — the frontier in columns | `client/web/src/screens/{FrontierColumns.tsx,frontier-columns.ts,frontier-facets.ts,frontier-prefs.ts}` (#399) | `FrontierColumns.tsx`, ADR-0021 |
| Local dictation into capture (#379) | `client/web/src/speech/local-dictation.ts`, `client/web/src/screens/capture-dictation.ts` | those headers, then ADR-0022 |
| The responsive layer and the two nav forms | `client/web/src/shell/{breakpoints.ts,responsive.css,useIsPhone.ts,NavBar.tsx,nav-bar.ts}` | `responsive.css` (why classes vs. a hook), then `nav-bar.ts` |
| Surfaces registry (visual gate) | `client/web/visual/` | `docs/SURFACES.md` |

Agent skills live in `.claude/skills/`, each with its own `SKILL.md`:
`/next-up-hb` (select and delegate), `/to-actions` (project → actions),
`/microtask` (item → steps), `/hummingbird-design` (the brand), and
`parse-capture` and `grill-me` (both runner-only). The three personal skills
each own a self-contained script that speaks the owned API — the duplication
between them is deliberate and each script's header says why; `.github/workflows/skills.yml`
gates them.

Working docs: `docs/agents/issue-tracker.md` (issues are GitHub issues driven
via `gh`; the wayfinder map is issue #1), `docs/agents/triage-labels.md`,
`docs/agents/domain.md`.

## Repo-wide rules

These belong to no single file, and other files cite this one for them.

**Credential blast radius.** A credential goes in GitHub Actions secrets only
when its worst-case abuse is bounded and local — `CITY_WASTE_INGEST_TOKEN`
reaches three routes for one source and the worst it can do is a wrong bin
day. `ADMIN_SECRET` (mints every other token) and `FCM_SERVICE_ACCOUNT`
(carries an RSA private key) stay out of Actions, set from the operator's
terminal. `RUNNER_BEARER_TOKEN` sits between them: a holder can run any
registered skill — spending metered model tokens — and reach the runner's own
`HB_API_TOKEN`, so it is **transitively a write credential plus a spend
faucet**. Cloudflare Worker secret plus a Fly secret, both set from the
operator's terminal, never Actions; since #273 rotating it is a two-place
operation (ADR-0018). A `device` token is the authority's only read-capable
scope and is write-everything, so anything holding one is treated as a write
credential however read-only it looks — and since #273 it can also *cause
spend*, via `POST /api/skills/run`. See ADR-0011 for the per-source table.

**No competing clocks.** Exactly one thing owns each cadence: supercronic owns
the sweeper's, the Durable Object's `alarm()` owns the sweep tick's. A second
cron for either is banned (issue #8). Actions `schedule:` is otherwise a
*scoped exception* rather than a drift — each poller workflow's own header
states its case, starting with `.github/workflows/city-waste.yml`.

**The wasm32 worker build stays thin.** Nothing reachable from
`hummingbird-authority-worker` may take an HTTP client, a tzdb or an HTML
parser as a dependency. The pollers are out-of-process crates for this reason,
and for a second one: `server/worker` has no test harness, so anything
expressed there is untested by construction — keep every decidable thing in a
natively-tested lib and leave only `fetch`/`crypto.subtle` in the shim.

**The design system.** The UI brand is the "Hummingbird Design System" project
on claude.ai/design, mirrored at `.claude/skills/hummingbird-design/`. **All
frontend/UI work must use it: invoke `/hummingbird-design` before styling
anything.** Tokens are copied into `client/web/src/design/`; when the design
project changes, re-pull the mirror first, then re-copy (that directory's
`github.md` has the record). Android hand-ports them into its Compose theme
files under a CI drift gate instead (ADR-0026).

**The build version.** `VERSION` at the repo root holds `major.minor.patch`;
the displayed patch adds the commits on `main` since that file was last
touched. **Editing `VERSION` in a PR is the whole override gesture** — no
tags, no release workflow, no bot commits to `main`. The scheme and its
consequences are in `client/web/src/shell/build-version.ts`.
