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

**Where each step runs.** The gateway (ThinkyPX13 / WSL) **allows no SSH —
port 22 is refused**, and its permission classifier blocks gateway-admin
instructions arriving as agent messages. So every provisioning step below
is a local shell *at the gateway host*, sitting at the PC. That is also why
steps 1–2 are done there: minting the token on the gateway host means the
plaintext never has to travel between machines at all. Steps 9–10 are
Skills-repo edits and can be done from any machine.

The conventions this runbook follows — agent creation, personas, Telegram
topic routing, the client-side verification rules — are the operator's own,
recorded in the Skills repo at `openclaw/README.md`, `telegram-routing.md`
and `openclaw-bootstrap/SKILL.md`. This page does not restate them; it
names the step and points there.

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

   `ADMIN_SECRET` rides on curl's argv here, where `ps` can see it, and
   that is a deliberate exception rather than an oversight. The rule the
   rest of this lane follows is the opposite — the agent's charter says
   never pass the device token as an argument, and `hb-tasks.sh` writes its
   header into a mode-600 temp file to obey that. The exception holds
   because this is a one-shot interactive command on the single-user
   gateway host, run at the keyboard, and because
   `runner/scripts/mint-hb-token.sh` is the repo's own reviewed admin mint
   against this same route and secret and does the same thing; a temp-file
   header here would diverge from it and cost the runbook its
   copy-pasteable block. **Anything long-lived or unattended that reaches
   `/api/admin/tokens` uses the header-file pattern instead**
   (`hb-tasks.sh`'s `request()`).

2. **Place it, mode 600** — same shell, so nothing crosses a machine
   boundary:

   ```sh
   mkdir -p ~/.config/hummingbird
   (umask 177 && op read op://dev/hummingbird-openclaw-agent/password \
      > ~/.config/hummingbird/api-token)
   ```

3. **Clone this repo on the gateway host** (anywhere stable; the skills
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
   agent workspace's `AGENTS.md` — a plain `cp` at the gateway host, or,
   from a client, the `ship-charter.sh` gesture (a `--message-file` telling
   the agent to write the file verbatim and report back its byte size and
   first line — verify that report).

   **Overwrite, don't append.** The Skills repo's `ship-charter.sh` appends
   a `## Chief-of-Staff Charter` section and drops `CHARTER.md` +
   `PROJECT-CARD.md` at the workspace root. That is the *other* agent's
   convention: `ship-charter.sh` is never run against `hummingbird-tasks`,
   which is exactly why a whole-file copy is right here.

   **Re-copying preserves the tail.** The charter ends in a marker
   (`<!-- Agent-appended notes below this line. -->`) and everything under it
   is the agent's own. A flat `cp` on update would erase it, so an update is
   copy-the-head, keep-the-tail:

   ```sh
   marker='<!-- Agent-appended notes below this line. -->'
   awk -v m="$marker" 'index($0,m){found=1} found' <workspace>/AGENTS.md \
     | tail -n +2 >/tmp/agent-notes
   cp <clone>/openclaw/agent/AGENTS.md <workspace>/AGENTS.md
   cat /tmp/agent-notes >><workspace>/AGENTS.md && rm -f /tmp/agent-notes
   ```

   A first install has no tail and is the plain `cp` alone.

6. **Give it a persona and prove it is really there.** Pick a name (every
   agent has one — Allen, Ralph, Athena, Forbin) and set it with a
   `set-identity` message, per `openclaw-bootstrap/SKILL.md` Phase 5.

   Verify by *messaging the agent*, never by reading a client-side listing:

   ```sh
   openclaw agent --session-key "agent:hummingbird-tasks:bootstrap" \
     --message "Reply with the single word READY."
   ```

   No `--timeout`: the default is 600 and gateway failover alone takes 90s,
   so a lowered one turns a healthy bootstrap into a reported failure — see
   "Talking to the agent" below.

   The Skills README records why: local `openclaw` admin subcommands
   (`agents list`, `--agent`, `memory *`) **fabricate plausible output for
   any agent id**, so a listing that shows the agent proves nothing. An
   unpaired device also falls back silently to an embedded local agent that
   then fails on a missing provider key — the READY round-trip is the only
   thing that distinguishes these.

7. **Route its Telegram topic — optional** (#609's operator-gate list says
   so, and nothing below depends on it: step 6's terminal round-trip is the
   whole functional path, and a topic only adds a phone-side surface).
   If you want one, follow "Add a topic for a new agent" in
   the Skills repo's `openclaw/telegram-routing.md` verbatim: create the
   forum topic in the OpenClaw group (`-1004418189396`), read the id off
   the inbound line in `openclaw logs --follow`, then

   ```sh
   openclaw config set 'channels.telegram.groups.-1004418189396.topics.NN.agentId' hummingbird-tasks
   openclaw gateway restart
   ```

   Verify by messaging the topic: the logged session key must begin
   `agent:hummingbird-tasks:`. If it reads `agent:main:`, the mapping did
   not apply. The deposits lane (`agent:hummingbird:deposits`) and the
   Hummingbird topic both stay pointed at the chief-of-staff.

8. **Smoke-test, in this order**, each step driven as a message on a real
   session key (same reason as step 6):
   - `sweep` renders the live items in a fresh session (the charter makes
     this the first thing the agent does unprompted).
   - `add` a throwaway item from chat; `edit` its notes, then its `--size`
     / `--energy` / `--context`; `done` it.
   - One grill interview on a foggy item, accepted: the item's fields
     update, and the grill row shows in the sweep's `grills` afterwards. A
     replayed record returns the stored row (deterministic id), not a
     second grill.
   - One microtask breakdown on an item with no live plan: steps land,
     visible in the app; asking again for the same item reports the live
     plan instead of appending a second one (the SKILL.md live-plan rule).

## Registering it on the Skills side

The gateway's conventions live in the operator's Skills repo, and an agent
that exists only here is an agent the next bootstrap session will not know
about. Two edits there, neither of which this repo can make for itself:

9. `openclaw/README.md` — the paragraph recording that hummingbird has a
   second, non-chief-of-staff agent whose charter and skills are authored
   *here*, and that `ship-charter.sh` must never target it. It takes no
   `next-up` card and no line in the SessionStart hook's allowlist — that
   allowlist is keyed on the project name (`hummingbird`, already present),
   not on an agent id, and the hook's own briefing lane
   (`agent:hummingbird:claude-<session>`) is unaffected by this agent.
10. `openclaw/telegram-routing.md` — the routing-table row, if step 7 has
    produced a topic id.

**What is deliberately not on this list** is a front-desk change:
`openclaw/CHARTER-MAIN.md` §1 stays as it is, and Forbin keeps sending a
*task* ask wherever it sends one today. Adding this agent to that redirect
list is exactly the "documented preferred path" ADR-0029 decision 7 (#609
decision 9) reserved for a later, evidence-backed decision — the default is
charter and habit only. Doing it here would also contradict this page's own
three-arm section: nothing routes through the agent, prefers it, or degrades
without it.

## Talking to the agent

Reaching it from a terminal is `openclaw agent --session-key
"agent:hummingbird-tasks:<key>" --message-file <path>`. Two rules, both
learned the expensive way and recorded in the Skills repo's
`deposit/SKILL.md`:

- **Do not lower `--timeout`.** OpenClaw's own default is 600 and gateway
  failover alone takes 90s. A short timeout turns a working call into an
  ambiguous one.
- **`CLI exceeded timeout (Ns) and was terminated` is not a failure.** The
  turn may well have landed. Resolve it with a **read-only `sweep`**, never
  by resending — `add` is the one non-idempotent verb here (a random v4
  item id, by design: two same-titled adds are two items), so a retry is
  how you get a duplicate. Steps and grill records are deterministic and
  replay safely; item adds are not.

The one genuinely unrecoverable shape is `EMBEDDED FALLBACK: Gateway agent
timed out` together with `ProviderAuthError: No API key found for provider
…` — that is the unpaired-device fallback from step 6, not a hummingbird
problem.

## Update path

Skills change in this repo, land on `main`, and reach the agent by `git
pull` on the gateway clone followed by a re-install. That re-install needs
`--force` — `openclaw skills install` refuses to overwrite an existing
workspace skill without it — and it covers all three skills: they install
independently and none of them updates the others, so it is all three or
the agent runs a mixed set.

```sh
for s in hummingbird-tasks microtask grill-me; do
  openclaw skills install "<clone>/openclaw/$s" --agent hummingbird-tasks --force
done
```

(`--as <slug>` pins the installed slug if a local directory name ever
diverges from the skill name.) The charter is a re-copy, head-only, per step
5. Nothing self-updates, and the gateway clone is the only thing that has to
be pulled.
