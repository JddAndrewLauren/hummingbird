# The OpenClaw task agent

> **Status (2026-08-20): provisioned and live** on the gateway, by walking
> the runbook below (#642). Everything under `openclaw/` ships from this
> repo. Decision record:
> [ADR-0029](adr/0029-an-openclaw-agent-is-a-third-interactive-arm.md),
> and the grilling session recorded in
> [#609](https://github.com/JddAndrewLauren/hummingbird/issues/609).

A dedicated agent (`hummingbird-agent`, persona **Allen**) on the operator's OpenClaw gateway:
it starts every session by sweeping the live task list, adds and edits
items from chat, and is the operator's personal default way to run a grill
interview or a microtask breakdown. It is deliberately **not** the
hummingbird chief-of-staff agent (`hummingbird`, persona **Rufous**, the
`/deposit` target), whose charter discards exactly the status this agent
lives on. Rufous held the name Allen until 2026-08-20, when the two
personas were reassigned; its own stored deposits still say Allen.

`hummingbird-tasks` names a **skill** here, never the agent — the agent id
is `hummingbird-agent`. ADR-0029 decision 1 was accepted with the agent
under the skill's name and carries the amendment.

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
instructions arriving as agent messages. So steps 2–8 are a local shell *at
the gateway host*, sitting at the PC. Step 1 is vault-first and can run from
any machine with `op` and `ADMIN_SECRET` — the plaintext's home is the vault
either way, and step 2 reads it back out on the gateway. Steps 9–10 are
Skills-repo edits and can be done from any machine.

**Prerequisites on the gateway**, none of which a fresh WSL install has:
`bash`, `curl` and **`jq`** are the dependency floor (#609 decision 3) and
all three skill scripts refuse without them — the first sweep of the
2026-08-20 provisioning run died on `hb-tasks.sh: jq is required`. Install
it (`sudo apt-get install -y jq`) before step 8. `uuidgen` is *not* needed:
`hb-tasks.sh` falls back to `/proc/sys/kernel/random/uuid` for exactly this
host. `git`, `gh` and the `openclaw` CLI are needed for steps 3–7.

**`op` on the gateway.** There may be none: on ThinkyPX13 the 1Password CLI
was absent from the Windows side (no WinGet package, nothing on the interop
PATH) and absent from WSL. It was solved with a **1Password service
account** signed in inside WSL, which works and survives rotations — but be
clear about what it is: a credential that reads the whole `dev` vault,
sitting on the gateway, a wider blast radius than the single device token it
fetches. Its own storage location deserves the same care this page gives the
device token. The alternative, a one-time manual paste from the desktop app,
costs the runbook its "plaintext never travels" property and returns at
every rotation.

The conventions this runbook follows — agent creation, personas, Telegram
topic routing, the client-side verification rules — are the operator's own,
recorded in the Skills repo at `openclaw/README.md`, `telegram-routing.md`
and `openclaw-bootstrap/SKILL.md`. This page does not restate them; it
names the step and points there.

1. **Mint the token, vault-first** — `scripts/mint-device-token.sh`, the
   repo's own device mint (#635), from any machine with `op`:

   ```sh
   ./scripts/mint-device-token.sh openclaw-agent \
     "openclaw hummingbird task agent" hummingbird-openclaw-agent
   ```

   Read that script's header before editing anything about this step. The
   plaintext exists only in the original 201; a replay answers 200 with
   metadata and no token, and revoke is a soft delete that does **not** free
   the id — so a lost plaintext burns that id permanently. The script
   refuses before minting if the vault title is taken, checks the token's
   shape, and reads it back byte-for-byte before declaring success, because
   **the store is the part that fails**: provisioning the Mac on 2026-08-20
   burnt `device-mac` and `device-mac-2` back to back on the hand-rolled
   `op item create … password[password]=-` form this step used to carry.

   Two read-only prechecks are worth a minute, since the failure is
   unrecoverable — confirm the id is absent from `GET /api/admin/tokens` and
   that `op item get <title>` misses.

2. **Place it, mode 600**, on the gateway:

   ```sh
   mkdir -p ~/.config/hummingbird
   (umask 177 && op read op://dev/hummingbird-openclaw-agent/password \
      | tr -d '\r\n' > ~/.config/hummingbird/api-token)
   chmod 600 ~/.config/hummingbird/api-token
   wc -c < ~/.config/hummingbird/api-token   # 67 = hb_ + 64 hex
   ```

   The explicit `chmod` is because `umask` governs only *newly created*
   files, and a failed earlier attempt leaves a 0-byte file behind — the
   redirect truncates before `op` runs. `tr -d '\r\n'` matters if `op` is a
   Windows binary called from WSL: a stray `\r` sends a silently wrong
   credential, presenting as a 401 that reads like a bad token rather than a
   bad copy. **Check before writing**: on a host that already holds another
   device's token at this path, overwriting it repoints that device's tools
   and makes `last_seen` useless for telling the two apart.

   Then prove it authenticates before building on it — `GET /api/changes`
   with the token in a `curl --config -` header file (never `-H`, where `ps`
   can read it) should answer 200.

3. **Clone this repo on the gateway host** (anywhere stable; the skills
   install by path from it) and **create the agent**:

   ```sh
   openclaw agents add hummingbird-agent
   ```

4. **Install the three skills** into that agent from the clone:

   ```sh
   openclaw skills install <clone>/openclaw/hummingbird-tasks --agent hummingbird-agent
   openclaw skills install <clone>/openclaw/microtask        --agent hummingbird-agent
   openclaw skills install <clone>/openclaw/grill-me         --agent hummingbird-agent
   ```

5. **Install the charter**: copy `openclaw/agent/AGENTS.md` over the new
   agent workspace's `AGENTS.md` — a plain `cp` at the gateway host, or,
   from a client, the `ship-charter.sh` gesture (a `--message-file` telling
   the agent to write the file verbatim and report back its byte size and
   first line — verify that report).

   **Overwrite, don't append.** The Skills repo's `ship-charter.sh` appends
   a `## Chief-of-Staff Charter` section and drops `CHARTER.md` +
   `PROJECT-CARD.md` at the workspace root. That is the *other* agent's
   convention: `ship-charter.sh` is never run against `hummingbird-agent`,
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
   agent has one — Allen, Rufous, Ralph, Athena, Forbin) and set it with
   the CLI, per `openclaw-bootstrap/SKILL.md` Phase 5:

   ```sh
   openclaw agents set-identity --agent hummingbird-agent --name "Allen"
   ```

   **A message asking the agent to rename itself is not enough.** It edits
   `IDENTITY.md` in the workspace, while the runtime reads the `identity`
   block in `~/.openclaw/openclaw.json` — so a message-only rename leaves
   the effective name stale, and the agent may answer with the new name from
   context while the gateway still holds the old one. Verify by reading that
   config's id/name pairs (a file read cannot fabricate), not by `agents
   list`. Never hand-edit `openclaw.json`: it holds every other agent's
   entry and the gateway token.

   Verify by *messaging the agent*, never by reading a client-side listing:

   ```sh
   openclaw agent --session-key "agent:hummingbird-agent:bootstrap" \
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
   openclaw config set 'channels.telegram.groups.-1004418189396.topics.NN.agentId' hummingbird-agent
   openclaw gateway restart
   ```

   Verify by messaging the topic: the logged session key must begin
   `agent:hummingbird-agent:`. If it reads `agent:main:`, the mapping did
   not apply. The deposits lane (`agent:hummingbird:deposits`) and the
   Hummingbird topic both stay pointed at the chief-of-staff.

   Done on 2026-08-20: topic **54**, "Hummingbird Tasks". The name is
   cosmetic — routing keys on the numeric id alone.

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

Both were done on 2026-08-20, along with a third the original list said
would *not* happen: **the front desk now redirects**. `CHARTER-MAIN.md` §1
tells Forbin to split by kind of ask — the task list goes to Allen, the
project's code and decisions and history go to Rufous — and it was shipped
with `./openclaw/ship-charter.sh main`. Until that ship runs, the front desk
holds the previous rules, which after the persona swap point task asks at
precisely the agent chartered to discard status.

The earlier version of this section reserved that redirect for a later,
evidence-backed decision, reading ADR-0029 decision 7 (#609 decision 9) as
covering it. The decision's actual scope is narrower and still holds: no
**repo** surface routes through the agent, prefers it, or degrades without
it, and the three-arm table above remains a description rather than a
ranking. A front-desk charter in the operator's own Skills repo is not a
repo surface — it is the operator's habit, written down where the habit
lives. ADR-0029 carries the amendment.

## Talking to the agent

Reaching it from a terminal is `openclaw agent --session-key
"agent:hummingbird-agent:<key>" --message-file <path>`. Two rules, both
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
  openclaw skills install "<clone>/openclaw/$s" --agent hummingbird-agent --force
done
```

(`--as <slug>` pins the installed slug if a local directory name ever
diverges from the skill name.) The charter is a re-copy, head-only, per step
5. Nothing self-updates, and the gateway clone is the only thing that has to
be pulled.
