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
| The notification lane | `authority/src/{delivery,sweep,fcm,google_oauth}.rs`, `worker/src/fcm.rs` | `delivery.rs`, ADR-0012/0014 |
| The authority mints the web host's Google calendar token (#577) | `server/authority/src/{google_oauth,google_calendar}.rs`, `server/authority/src/handlers/calendar_token.rs`, `server/worker/src/calendar.rs`, `client/web/src/calendar/authority-token-client.ts` | `google_calendar.rs`, then ADR-0028 |
| Grill (interview + the immutable per-item attachment) | `domain/src/grill.rs`, `authority/src/handlers/grills.rs` | `grill.rs`, ADR-0023 |
| The which-cans poller | `server/city-waste/` | `src/lib.rs`, then `src/judge.rs` |
| Evaluated-stream pollers | `server/{gmail-poll,calendar-poll,graph-poll}/` | each `src/lib.rs`, ADR-0011 |
| The race lane (2 binaries) | `server/race-poll/` | `src/lib.rs` |
| The client sync engine | `client/core/src/sync/` | `sync/mod.rs`, then `sync/cycle.rs`, ADR-0007/0008 |
| The one client API | `client/core/src/lib.rs` | its `Core` docs — every mutation entry point carries its own, including whether it overlays |
| Ranking / freshness / panes / bindings | `client/core/src/{rank,freshness,pane,bindings}.rs` | each header; ADR-0015 for the Rust/TS carve-out, **as redrawn by ADR-0025** |
| The decisions every client shares (#141/M1) | `client/core/src/decisions/`, `client/ffi-web/src/decisions.rs`, `client/web/src/decisions/seam.ts` | `decisions/mod.rs`, then `seam.ts`; ADR-0025 |
| The panes' decision half + the zone bridge (#533/M4) | `client/core/src/decisions/panes/`, `client/web/src/screens/questions/zone-bridge.ts` | `panes/mod.rs`, then `panes/zone.rs`; ADR-0025 |
| The standing-question roster (#714, ADR-0034 decision 4) — every question named once, with its label, its surface and the bindings that answer it; the spine of Settings' `Standing questions` section on the web | `client/core/src/decisions/questions.rs`, `client/ffi-web/src/decisions.rs`'s `question_roster_json`, `client/ffi-mobile/src/lib.rs`'s `question_roster`, `client/web/src/screens/{questions/roster.ts,bindings.ts,SettingsScreen.tsx}` | `decisions/questions.rs`, then ADR-0034 decision 4; ADR-0025 |
| The pane lane's mobile seam (#536/M4) — `paneZoneQueries`/`rankPanes`, applied results only | `client/ffi-mobile/src/lib.rs` (its panes (#536) section) | that section's own header; ADR-0025 |
| Android's calendar lane (#564) and the panes it unblocks (#621) — the authority mint, the calendar half behind its own lock, Settings' Calendar section, the weekend card's plan chips | `client/ffi-mobile/src/{calendar_token.rs,lib.rs}` (its calendar (#564) section), `client/core/src/calendar/{host.rs,selection.rs}`, `client/android/.../{CalendarPrefs.kt,SettingsScreen.kt,ui/panes/NowPanesExpanded.kt}` | `calendar_token.rs`, then `host.rs`; ADR-0028 as amended by #564, ADR-0025 |
| The skills runner lane, client side (#538/M4) | `client/core/src/decisions/skills/`, `client/web/src/skills/`, `client/android/.../hummingbird/skills/` | `decisions/skills/mod.rs`, then `SkillRunner.kt`; ADR-0025 as amended by #538 |
| The `/next-up-hb` seam | `client/next-up/` | `src/lib.rs` |
| The wasm seams | `client/ffi-web/src/{task_host,calendar_host}.rs` | those headers |
| The OpenClaw task agent (third interactive arm, ADR-0029) | `openclaw/` | `docs/openclaw.md` |
| The agent's calendar write (ADR-0031) — the id-gated mint route, and the skill that calls it | `server/authority/src/handlers/calendar_token.rs`, `server/worker/src/calendar.rs`, `openclaw/calendar/` | `calendar_token.rs`'s `write_verdict`, then `openclaw/calendar/scripts/gcal.sh`; ADR-0031 |
| The SCPS mail writer (ADR-0032) — the fifth skill, delegating every calendar call to `gcal.sh` and writing the monthly Photo Quest binding itself | `openclaw/scps/`, `openclaw/agent/AGENTS.md`'s "SCPS mail" section | `openclaw/scps/scripts/scps.sh`, then `docs/openclaw.md`'s "The SCPS mailbox" runbook section; ADR-0032 |
| The mobile seam + the Android app (#141, through the frontier-board slice) | `client/ffi-mobile/`, `client/android/` | `ffi-mobile/src/lib.rs`, `android/README.md`, ADR-0025 |
| The bottom nav + Done/Ledger roster sink (M3/#532) | `client/core/src/decisions/roster.rs`, `client/ffi-mobile/src/lib.rs`, `client/android/.../{MainActivity,Done,Ledger}{Screen,ViewModel}.kt` | `decisions/roster.rs`, then `android/README.md`'s "The bottom nav" section; ADR-0025 |
| Item detail, and where a tapped notification lands | `client/core/src/{item_detail.rs,decisions/notification.rs}`, `client/android/.../ItemDetail{Panel,Screen,ViewModel}.kt` (the panel is the body, with four hosts: the route, Now inline, the Recall overlay, and Triage in `PROMOTE` mode) | `item_detail.rs`, then `decisions/notification.rs`; ADR-0027 |
| The rules surface (#141/M4) — the sink, both seams, the Compose screen | `client/core/src/decisions/rules/`, `client/ffi-mobile/src/lib.rs`, `client/android/.../Rules{Screen,ViewModel}.kt` | `decisions/rules/mod.rs`, then `backtest.rs`; ADR-0013/0025 |
| A standing question can be switched off (#715, ADR-0034) — the second `settings` vocabulary, its Core doors, both seams, and the web's expandable roster rows | `client/core/src/question_switch.rs`, `client/core/src/lib.rs`'s `question_switches`/`set_question_enabled`, `client/core/src/decisions/panes/mod.rs`, `client/{ffi-web,ffi-mobile}/src/`, `client/web/src/screens/{question-prefs.ts,SettingsScreen.tsx}` | `question_switch.rs`, then ADR-0034 |
| The Settings screen (#141/M4) — bindings, token, sync, dead letters, theme | `client/core/src/decisions/settings.rs`, `client/ffi-mobile/src/lib.rs`, `client/android/.../Settings{Screen,ViewModel}.kt`, `theme/` | `decisions/settings.rs`, then `SettingsViewModel.kt`; ADR-0025 |
| The Grill takeover and the microtask affordance (#141/M4) — the affordance/review/backend-fallback sinks, both seams, the Android takeover and Settings' backend picker | `client/core/src/decisions/skills/{affordance,review,backend}.rs`, `client/ffi-mobile/src/lib.rs`, `client/android/.../{GrillTakeover,Microtask}{Screen,ViewModel}.kt`, `client/android/.../skills/{MicrotaskRunner,BackendPreference}.kt` | `decisions/skills/mod.rs`, then `affordance.rs`; ADR-0023/0025 |
| Recall on Android (#478/#542, M4's closer) — the mobile seam door, and the overlay it draws (a gesture over the NavHost since #634, not a route) | `client/core/src/search.rs`, `client/ffi-mobile/src/lib.rs`, `client/android/.../Recall{Overlay,ViewModel}.kt` | `search.rs`, then `android/README.md`'s "The Recall overlay" section; ADR-0025 |
| Android's notification lane (#141/M2) | `client/android/app/src/main/kotlin/net/twinion/hummingbird/{notify,push}/` | `notify/NotificationChannels.kt`, `push/AckRunner.kt`; ADR-0012/0014 |
| The web app | `client/web/` | `client/web/README.md` |
| The SharedWorker layer | `client/web/src/worker/` | `core.worker.ts` (note its top-level-`await` invariant), ADR-0010 |
| The standing-question panes | `client/web/src/screens/questions/` + `*-pane/` | `questions/contract.ts`, ADR-0015 |
| The Projects page and the project lane's client write (#624, the first slice off #449) — grid, create, dossier shell, the properties card (#625: `github_repo`/`default_context`), the links card (#626: `project_links`, ADR-0030 decision 4), the reading column's Route card (#627: `destination`/`notes`, ADR-0030 decision 1), the aside's archive card (#630: the timestamp-matched archive/unarchive cascade onto the project's live items, server-side, ADR-0030 decision 5), and — replacing the fog card and the ordered action list that #628/#629 put in the centre column — **the frontier board filtered to the open project** (membership is `project_id` alone; the fog and `project_pos` records stay server-side and `/to-actions` still writes them, but no web surface reads them any more) | `server/authority/src/handlers/{projects,items}.rs`'s `patch`/`cascade_archive_for_project`, `client/core/src/lib.rs`'s `create_project`/`patch_project`/`create_project_link`/`patch_project_link`/`route`/`patch_route`, `client/ffi-web/src/{task_host,lib}.rs`, `client/web/src/{shell/useProjectsWiring.ts,screens/ProjectsScreen.tsx,screens/FrontierBoard.tsx,screens/projects/}` | `ProjectsScreen.tsx`, then `Core::create_project`'s doc for the no-overlay contract; ADR-0030, decision 5 for the cascade's own mechanism |
| An item points at one note in the Obsidian vault (#771) — the `items.vault_path` column, the `obsidian-vault` binding, and the one place all the Obsidian vendor knowledge lives (the `&append` flag is load-bearing; its own header says why). Web only: the phone carries the column and draws nothing | `server/domain/src/item.rs`, `server/authority/src/{schema.rs,handlers/items.rs}`, `client/core/src/bindings.rs`, `client/web/src/obsidian/vault-uri.ts`, `client/web/src/components/domain/ItemPanel.tsx` | `client/web/src/obsidian/vault-uri.ts`, then ADR-0009's `items.vault_path` amendment; ADR-0002/0015 as amended |
| The Status screen (the web's tile board over the ranked panes) | `client/web/src/screens/{StatusScreen.tsx,status-board/}` (#311) | `status-board/StatusBoard.tsx`, then `questions/contract.ts`; ADR-0017 as amended by **ADR-0033** |
| The Status nav control tints itself — the worst band the Status surface answers, decided once in the core and painted per client (a gap stays silent) | `client/core/src/decisions/panes/alarm.rs`, `client/ffi-web/src/decisions.rs`'s `status_alarm_json`, `client/ffi-mobile/src/lib.rs`'s `status_alarm`, `client/web/src/shell/{nav-alarm.ts,NavBar.tsx,NavRail.tsx}`, `client/android/.../{NavAlarm,NavRail,MainActivity}.kt` | `alarm.rs`, then `nav-alarm.ts`; ADR-0025 |
| Android's Status screen — the quiet stack (#689), the status four, plus the phone's own persisted sync history (#536/M4) | `client/android/.../Status{Screen,ViewModel}.kt`, `client/android/.../ui/panes/{StatusQuietStack,StatusPartition}.kt`, `client/android/.../core/SyncHistoryStore.kt` | `StatusQuietStack.kt`, then `StatusScreen.kt`; ADR-0017 decision 1 (executed, not amended — the web's board is ADR-0033), ADR-0025 |
| The frontier board — the frontier in columns, on Now and on a project's dossier | `client/web/src/screens/{FrontierBoard.tsx,FrontierColumns.tsx,frontier-columns.ts,frontier-facets.ts,frontier-lanes.ts,frontier-prefs.ts}` (#399) | `FrontierBoard.tsx`, then `FrontierColumns.tsx`; ADR-0021 |
| Local dictation into capture (#379) | `client/web/src/speech/local-dictation.ts`, `client/web/src/screens/capture-dictation.ts` | those headers, then ADR-0022 |
| The responsive layer and the two nav forms | `client/web/src/shell/{breakpoints.ts,responsive.css,useIsPhone.ts,NavBar.tsx,nav-bar.ts}` | `responsive.css` (why classes vs. a hook), then `nav-bar.ts` |
| Surfaces registry (visual gate) | `client/web/visual/` | `docs/SURFACES.md` |
| The diagnostics lane (#705/#712) — the shared wire contract, the PWA journal/export, Android's recorder/export, and the authority's correlated request events | `server/domain/src/diagnostics.rs`, `client/core/src/diagnostics/`, `client/web/src/{worker/diagnostics-*.ts,shell/diagnostics-download.ts}`, `client/android/.../diagnostics/`, `server/authority/src/diagnostics.rs` | `docs/diagnostics.md`, then `server/domain/src/diagnostics.rs`'s module header |

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
credential however read-only it looks — since #273 it can also *cause
spend*, via `POST /api/skills/run`, and since #577 it can also mint a Google
`calendar.readonly` access token via `POST /api/google/calendar_token`,
against a server-held credential shared by every device (ADR-0028) —
**and since #564 the phone is one of the minters**, over that same route
with that same credential, so the sentence above is a statement about every
device token rather than about browsers. The
population of `device` tokens is: one per operator device, the runner's
(`runner`), and the OpenClaw agent's (`openclaw-agent`, on the gateway
machine — ADR-0029, minted and rotated per `docs/openclaw.md`).
**That population is no longer uniform in what it can reach.** Since
ADR-0031 one named member of it, `openclaw-agent`, can additionally mint a
`calendar.events` **write** bearer via `POST
/api/google/calendar_write_token` — a third server-held Google credential —
and so can change the operator's real calendar; every other device token,
including every browser's, is answered 403 there. That gate is an
allowed-holder list checked inside the handler, not a scope: a route gated
on a token **id** is a first here, and ADR-0031 states why a fourth `Scope`
was the wrong way to buy it. Since #711 every request the Durable Object
handles *and authenticates* is also named in the authority's own Workers
Logs by its acting token's non-secret `id` (`request.finished`'s `token_id`
field) — never the token value itself. Three kinds of request carry no
`token_id`, all for the same reason (no token was ever resolved): the admin
lane, which authenticates against `ADMIN_SECRET` and has no per-caller id
to name; a 401, which resolved no token at all; and a 500 raised by the
token lookup itself. A 500 raised by a handler *after* auth succeeded does
name its token. That is a new fact
about what a log line reveals, not a new capability of any token: nothing
that already held a token gained a new way to act with it, but an operator
reading those logs (or anyone who can, since Workers Logs is a platform
surface, not one this repo gates further) can now tell *which* device made
a given request. See ADR-0011 for the per-source table.

**An item is named to the operator by its title, never `HB-<seq>`.** That ref
is a client-side affordance: no route accepts it, `resolve_ref` maps it onto a
uuid off a fetched sweep, and **no client surface displays `seq`** — so an
`HB-42` in prose, a report or a commit message is a handle the operator cannot
look up in the app. It stays legitimate as script input and inside a skill's
own plumbing; it is not how any agent, skill or doc refers to an item when a
human is reading. Disambiguate same-titled items by stage, due date or
context.

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
