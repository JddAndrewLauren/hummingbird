# The OpenClaw task agent

> **Status (2026-08-19): built, not yet provisioned.** Everything under
> `openclaw/` ships from this repo; the agent itself exists only once the
> operator runbook below has been run on the gateway. Decision record:
> [ADR-0029](adr/0029-an-openclaw-agent-is-a-third-interactive-arm.md),
> and the grilling session recorded in
> [#609](https://github.com/JddAndrewLauren/hummingbird/issues/609).

A dedicated agent (`hummingbird-tasks`) on the operator's OpenClaw gateway:
it starts every session by sweeping the live task list, adds and edits
items from chat, and is the operator's personal default way to run a grill
interview or a microtask breakdown. It is deliberately **not** the
hummingbird chief-of-staff agent (the `/deposit` target), whose charter
discards exactly the status this agent lives on.

## Shape

| File | What it is |
| --- | --- |
| `openclaw/hummingbird-tasks/` | The task CLI skill: `SKILL.md` + `scripts/hb-tasks.sh` (`sweep`/`add`/`edit`/`done`, items only, CAS writes). |
| `openclaw/microtask/` | The microtask arm: `SKILL.md` + `scripts/hb.sh`, a **verbatim copy** of `.claude/skills/microtask/scripts/hb.sh` — same frozen step-id recipe, CI-pinned against drift. |
| `openclaw/grill-me/` | The grill arm: `SKILL.md` + `scripts/grill-record.sh` (one verb: `POST /api/grills` for an accepted outcome). |
| `openclaw/agent/AGENTS.md` | The agent's charter template — session-start sweep, the default gestures, the boundaries. |
| `.github/workflows/openclaw.yml` | The gate: shellcheck over every script here, plus the step-id parity check between the two `hb.sh` copies. |

## The three-arm picture

Grill-me and microtask now have three arms, all writing to the same
authority under recipes that cannot double-mint:

| Arm | Model | Where the writes happen |
| --- | --- | --- |
| The app (phone/web) | the hosted runner's, via `POST /api/skills/run` (ADR-0018) | `runner/` — prepare/apply guards, #307/#312/#317 |
| A Claude session in this repo | the session's | `.claude/skills/*/scripts/` |
| The OpenClaw agent | the agent's own | `openclaw/*/scripts/` |

The OpenClaw arm never calls the runner and spends no metered runner
tokens; its interviews and breakdowns run on the agent's own model. The
step-id recipe (`sha256("hummingbird-skill/microtask/v1" + item + "/" +
body)`) is identical across all three arms — `runner/test/step-id.test.js`
pins the runner to the Claude arm, and this repo's openclaw workflow pins
the OpenClaw copy to the same file byte-for-byte. Grill records mint their
own id space (`hummingbird-skill/grill-me/openclaw/v1`).

The agent's being the *default* lives in its charter and the operator's
habit only. No repo surface routes through it, prefers it, or degrades
without it — this table is a description, not a ranking.

## Credential

The agent holds its own device token, id **`openclaw-agent`**, at
`~/.config/hummingbird/api-token` on the gateway machine. A `device` token
is write-everything, can cause runner spend via `POST /api/skills/run`, and
since #577 can mint a Google calendar token — so this file is treated as a
write credential with a spend faucet, exactly like every other device
token (CLAUDE.md's blast-radius section). It is revocable on its own:
`DELETE /api/admin/tokens/openclaw-agent`, no other device affected.

## Operator runbook (nothing below is run by an agent slice)

1. **Mint the token, vault-first** — the plaintext appears only in the
   original 201 (the route stores a hash and replays return metadata only),
   so the mint and the save happen in one pass, from your terminal:

   ```sh
   TOKEN=$(curl -sS -X POST https://hb.twinion.net/api/admin/tokens \
     -H "Authorization: Bearer $(op read op://dev/hummingbird-admin/secret)" \
     -H 'content-type: application/json' \
     -d '{"id":"openclaw-agent","name":"openclaw hummingbird-tasks agent","scope":"device"}' \
     | jq -er .token)
   printf '%s' "$TOKEN" | op item create --category password \
     --title hummingbird-openclaw-agent --vault dev password[password]=-
   ```

   (Adjust the `op read` path to wherever `ADMIN_SECRET` actually lives in
   the vault; the shape that matters is mint-and-store in one pipeline,
   nothing echoed. `jq -er` makes a replayed mint — 200, metadata, **no
   token** — fail loudly instead of storing the string `null`; on that
   failure revoke first, then re-mint: the id is burned, per
   `runner/scripts/mint-hb-token.sh`'s header.)

2. **Place it on the gateway machine** (`thinkypx13`), mode 600:

   ```sh
   ssh thinkypx13 'mkdir -p ~/.config/hummingbird && umask 177 && cat > ~/.config/hummingbird/api-token'
   # paste via: op read op://dev/hummingbird-openclaw-agent/password | ssh ... as above
   ```

3. **Clone this repo on the gateway machine** (anywhere stable; the skills
   install by path from it) and **create the agent**:

   ```sh
   openclaw agents add hummingbird-tasks
   ```

4. **Install the three skills** into that agent from the clone:

   ```sh
   openclaw skills install <clone>/openclaw/hummingbird-tasks --agent hummingbird-tasks
   openclaw skills install <clone>/openclaw/microtask        --agent hummingbird-tasks
   openclaw skills install <clone>/openclaw/grill-me         --agent hummingbird-tasks
   ```

5. **Install the charter**: copy `openclaw/agent/AGENTS.md` over the new
   agent workspace's `AGENTS.md`. Re-copy on change; the agent's own
   appended notes live below the marked line and are its to keep.

6. **Route channels** (optional): add routing rules so the chat surfaces
   you want reach this agent rather than `main` — `openclaw agents list
   --bindings` shows the current rules. The deposits lane
   (`agent:hummingbird:deposits`) stays pointed at the chief-of-staff.

7. **Smoke-test, in this order:**
   - `sweep` renders the live items in a fresh session (the charter makes
     this the first thing the agent does unprompted).
   - `add` a throwaway item from chat; `edit` its notes; `done` it.
   - One grill interview on a foggy item, accepted: the item's fields
     update, and the grill row shows in the sweep's `grills` afterwards. A
     replayed record returns the stored row (deterministic id), not a
     second grill.
   - One microtask breakdown on an item with no live plan: steps land,
     visible in the app; asking again for the same item reports the live
     plan instead of appending a second one (the SKILL.md live-plan rule).

## Update path

Skills change in this repo, land on `main`, and reach the agent by `git
pull` on the gateway clone + `openclaw skills update`/re-install. The
charter is a re-copy. Nothing self-updates.
