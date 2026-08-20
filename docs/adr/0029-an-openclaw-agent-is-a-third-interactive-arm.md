# ADR-0029: An OpenClaw agent is a third interactive arm and a distinct credential holder

**Status:** accepted · 2026-08-19
**Context:** #609, the OpenClaw-integration grilling session (that issue
holds the decision list). Extends the two-arm skill posture `docs/runner.md` records for
`microtask` (#272/#307/#317) and `grill-me` (#350) with a third arm, and
adds a row to the credential population CLAUDE.md's blast-radius section
governs. Does not touch [ADR-0018](0018-the-authority-proxies-the-skill-runner.md)'s
proxy lane — the new arm never calls it.

## The problem

The operator runs a personal OpenClaw gateway whose agents are reachable
from every machine and chat channel. None of them can see the task
authority. The one hummingbird-adjacent agent that exists — the
chief-of-staff holding `/deposit`'s dated narrative — is chartered to
*discard* status, so "what is on my plate, add this, break this down"
has no conversational home outside the app.

## Decision

1. **A new, dedicated agent** (`hummingbird-tasks`) on the gateway, with
   its own workspace and charter. The chief-of-staff and its deposits lane
   are untouched; the two agents' charters are deliberately disjoint
   (narrative vs. status).

   *Amended 2026-08-20 (#642): provisioning created the agent as
   **`hummingbird-agent`**, persona **Allen**; the chief-of-staff, which
   held the name Allen, is now **Rufous**. The three skills keep their own
   names — `hummingbird-tasks` is a skill here, not the agent. Nothing
   else in this decision changes.*

2. **Startup context is a fresh read, owned by the charter**: the agent
   runs `hb-tasks.sh sweep` first thing each session. No gateway
   session-start hook, no cron-refreshed file — the no-competing-clocks
   rule applies to context freshness too, and the charter is the one owner
   of this cadence.

3. **The task CLI is a repo-shipped OpenClaw skill**
   (`openclaw/hummingbird-tasks/`), verbs `sweep`/`add`/`edit`/`done`,
   items only. It follows the per-skill self-contained-script pattern the
   personal skills already use, and inherits `hb.sh`'s read/CAS/ref
   discipline verbatim.

4. **Grill-me and microtask run on the agent's own model, as a third
   interactive arm** — not through the hosted runner, and not by granting
   the agent the runner's bearer. The runner keeps the app's arm; a Claude
   session in this repo keeps the second; this is the third. What keeps
   three writers safe against each other is recipes, not routing:
   the microtask arm ships a **verbatim copy** of the skill's `hb.sh`
   (frozen id namespace `hummingbird-skill/microtask/v1`), pinned
   byte-identical by CI, so no arm can mint a step another arm already
   wrote.

5. **Accepted grill outcomes are recorded through `POST /api/grills`** —
   the route [ADR-0023](0023-the-grill-interview-is-a-native-typed-turn-contract.md)
   made the single applier of verdict→stage — via a one-verb, scope-guarded
   script with its own deterministic id space
   (`hummingbird-skill/grill-me/openclaw/v1`). Field edits are applied
   before the record, through the CLI; a declined proposal writes nothing.

6. **The agent holds a new `device` token, id `openclaw-agent`**, minted
   vault-first from the operator's terminal and placed at the default
   token path on the gateway machine. It joins the blast-radius table as a
   full write credential with a spend faucet, revocable alone.

7. **"Default" is charter-and-habit only.** No repo surface routes through
   the agent, prefers it, or degrades without it. Promoting the OpenClaw
   arm to a documented "preferred path" would be a later decision with
   evidence behind it, not this one.

   *Amended 2026-08-20 (#642): the operator's gateway front desk now does
   redirect task asks to the agent (and project questions to the chief of
   staff). That charter lives in the operator's Skills repo, so this
   decision's letter holds — no **repo** surface routes through the agent,
   and nothing here degrades without it — but the "no documented preferred
   path anywhere" reading no longer describes the gateway.*

## Rejected alternatives

- **Extending the chief-of-staff agent** — one agent for two charters
  whose content rules contradict (discard status vs. hold status).
- **The agent as a runner caller** (threading `grill-me` turns through
  `POST /api/skills/run`): keeps one implementation of the interview
  logic, but chains two model spends per turn (the agent's and the
  runner's), puts the conversational surface a network hop away from its
  own transcript, and was declined in the grilling in favour of the
  interactive-arm pattern that already exists for Claude sessions.
- **Folding step verbs into the task CLI** — breaks `hb.sh`'s "no verb
  touches an item" scope guard in reverse and puts the frozen id recipe in
  a second maintained implementation instead of a pinned copy.
- **A reduced-scope token** — nothing narrower than `device` exists to
  read the sweep; inventing a scope is authority work out of proportion to
  this slice, and the blast-radius section already has the vocabulary for
  holding a full token safely.

## Consequences

- Three arms mean three copies of `hb.sh`'s recipe surface (the skill's,
  the runner's JS port, the OpenClaw copy). The existing
  `runner/test/step-id.test.js` pins the second; the new
  `.github/workflows/openclaw.yml` pins the third (byte-identity of the
  whole script, which is stronger than pinning the recipe alone).
- The gateway machine joins the set of hosts holding a write credential.
  Rotation is `DELETE /api/admin/tokens/openclaw-agent` + a fresh
  vault-first mint + one file placement — `docs/openclaw.md`'s runbook.
- The OpenClaw arm's interview/breakdown quality is governed by the
  agent's own model choice, outside this repo's schemas — the same
  eyeball-a-few-runs posture `docs/runner.md` records for provider swaps.
- Nothing here is load-bearing for any client: if the agent, its gateway,
  or its machine disappears, capture/read/triage/sync and both other arms
  are unaffected.
