#!/usr/bin/env bash
# Local review candidates: build any revision of this repo (a PR head or
# main) into one of two persistent local slots, and run it beside the
# previous one. Each slot is a real authority (`wrangler dev`, local mode,
# its own Durable Object state) plus `vite preview` of the *production* web
# bundle with `/api` proxied to that slot's worker. **Local only: nothing
# here deploys, logs in to Cloudflare, mints against or calls the production
# authority, or reads the real device token.** `guard.sh` is the policy half
# and its header says what the guard does and does not protect against
# (short version: it catches configuration mistakes; it is not a sandbox,
# which is why PRs from forks are refused).
#
#   scripts/candidate/candidate.sh build <rev> [--slot a|b] [--fresh|--keep-state]
#   scripts/candidate/candidate.sh up <slot> [--keep-state]   start worker + preview, verify, print the URL
#   scripts/candidate/candidate.sh down <slot>        stop what `up` started, confirm the ports are free
#   scripts/candidate/candidate.sh status [--json]    both slots: sha, PR, build status, running, URL, token
#   scripts/candidate/candidate.sh mint <slot> [--copy]   a local device token into the slot's state
#   scripts/candidate/candidate.sh seed <slot>        post seed.json with the slot's token
#   scripts/candidate/candidate.sh pick <sha|ref>     print the slot `build` would use (no fetch)
#   scripts/candidate/guard.sh <slot-dir>             every static check, on its own
#
# Run it from a checkout of this repo that is not itself a slot. A typical
# side-by-side review:
#
#   candidate.sh build origin/main && candidate.sh up a    # main lands in a
#   candidate.sh build 827 && candidate.sh up b            # PR #827 lands in b
#   candidate.sh mint a --copy; candidate.sh seed a        # paste the token into Settings
#   open http://127.0.0.1:4801 http://127.0.0.1:4802
#
# ## Slots, ports, layout
#
# Root: `$HB_CANDIDATE_ROOT`, default `~/.local/state/hummingbird-candidate`.
# The guard refuses a root inside a git work tree or under the real token's
# config dir. Per slot `<root>/<a|b>/`:
#
#   hb-<slot>/      a detached git worktree of this repo, created `--lock`ed so
#                   `git worktree prune`/`remove` by other tools leave it alone;
#                   it keeps its own `node_modules`, `client/target`,
#                   `server/target`, `dist` and worker build between builds,
#                   which is what makes a rebuild warm. Never point
#                   CARGO_TARGET_DIR at a shared dir: cargo judges freshness
#                   by mtime, so a shared target can ship the other slot's code.
#   state/          persist/ (the Durable Object's SQLite), admin.env (a random
#                   local-only ADMIN_SECRET, mode 600), device-token (mode 600),
#                   wrangler.json and vite.preview.config.mjs (generated per
#                   build), home/ and xdg/ (the empty HOME and config dir the
#                   worker and preview run under), the pid files
#   logs/           build-<UTC>.log (every command exactly as it ran, so any
#                   step can be repeated by hand), wrangler.log, preview.log
#   manifest.json   rev, sha, PR, status (building|ok|failed), failed_step,
#                   built_at, build_ms, ports, url, paths, tool versions,
#                   target dir sizes; rewritten whole at each status change
#   events.jsonl    append-only timing: build.start, build.step (fetch,
#                   checkout, install, worker, web, config, guard; with ms),
#                   build.ok, build.failed, build.noop, up.ok, up.failed,
#                   down, mint, seed, fresh, guard.refused. A warm rebuild
#                   costs build.ok.ms + up.ok.ms.
#
#   slot  worker  inspector  web (open this)
#   a     8801    9231       http://127.0.0.1:4801
#   b     8802    9232       http://127.0.0.1:4802
#
# Always 127.0.0.1, never localhost: they are different browser origins.
# Each slot is its own origin, so its storage, service worker and token are
# its own. A slot reused for a new revision keeps its origin, though: the
# old cached app stays up behind the update banner until you accept it.
# Check the build string in the nav rail footer or Settings.
#
# ## build
#
# `<rev>` is either digits (a PR: `gh pr view` for its head, then
# `git fetch origin pull/<n>/head`, refused if the two disagree or the PR
# comes from a fork) or a git ref resolved after `git fetch origin` (use
# `origin/main` for main), refused unless some `origin/*` branch contains
# it: a local branch can hold a fork's code. The slot checks out the full
# SHA, detached.
#
# The slot (`pick`): one already holding this SHA with status ok, and then
# the build is a no-op (it says so; `--fresh` there only wipes state); else
# a failed or stale one (its worktree is reused, so no second checkout's
# worth of disk); else an unused slot, a before b; else the one with the
# older built_at. So the newest good candidate is never the default
# target and a failed build leaves it running. `--slot` overrides.
#
# Steps: preflight (tools, python3 >= 3.11 for `tomllib`, the env and root
# guard, the build lock), resolve (which fetches), pick, the state-carry
# check (below), free disk (900 MB for a cold slot or 300 MB warm, plus a
# 500 MB floor; tune with HB_CANDIDATE_{COLD,WARM,FLOOR}_MB), `down` the
# slot, then checkout (a tracked Cargo.lock an earlier build rewrote is
# restored first; any other local change refuses; then the file guard,
# before anything reads a committed env file),
# `pnpm install --frozen-lockfile`, `worker-build --release` (never
# `cargo install`), `pnpm build`, generate the two configs, and the full
# static guard. Every child runs from an allowlisted environment (`clean_env`
# below), never the caller's. A slot whose two target dirs pass
# HB_CANDIDATE_TARGET_MAX_MB (800) is wiped first; cargo never collects old
# builds. `build` never starts the slot: run `up`.
#
# ## State across revisions
#
# A slot's state (persist/, admin.env, device-token) carries over to the next
# revision built in it, so its token and browser session keep working. The
# one refusal: the SHA that last ran against persist/ is not an ancestor of
# the new one and `server/authority/src/schema.rs` or
# `server/worker/wrangler.toml` differ between them (old code on migrated
# SQLite). That SHA is `persist/.written-by`; it is not the manifest's,
# since a failed build names a SHA that never ran. `up` writes it just
# before starting the worker (whose first request migrates persist/, even
# if `up` then fails), and first runs the same test against it, so a
# persist/ copied in from the other slot is checked too. Then pass `build
# --fresh` (wipe persist/ and device-token; follow with `mint` and `seed`)
# or `--keep-state` (to `build`, which `up` then honours, or to `up`).
# Copying state across slots is by hand, both stopped:
#   down a; down b; rm -rf <root>/b/state/persist; cp -R <root>/a/state/persist <root>/b/state/; up b; mint b
#
# ## mint, seed
#
# `mint` posts `POST /api/admin/tokens` to `127.0.0.1:<slot worker port>`
# only, with the slot's own ADMIN_SECRET fed to curl on stdin, a fresh id
# per mint (`candidate-<slot>-<UTC>`), and writes the token to
# `state/device-token` (mode 600); it prints the path, never the token.
# It shares nothing with the operator mint scripts in `scripts/`.
#
# `seed` posts `seed.json` (beside this file; JSON carries no comments, so
# it is documented here): two projects, items across triage and ready with
# deadlines given as `deadline_in_min` from now (overdue, due now, soon),
# turned into local `YYYY-MM-DDTHH:MM` at seed time, and two grills on
# `cand-item-grilled` (one applied as proposed, one where applied and
# proposed differ) with `cand-item-never-grilled` left without one. Every id
# starts `cand-`; creates are idempotent by id, so a replay is safe. A seed
# failure (an older revision with another API shape) is reported and leaves
# the slot running.
#
# ## Teardown
#
#   for s in a b; do candidate.sh down $s; git worktree unlock <root>/$s/hb-$s; git worktree remove --force <root>/$s/hb-$s; done; rm -rf <root>
#
# (An `rm -rf` of the root alone leaves the locked worktrees as stale
# entries in `git worktree list`, which `prune` skips.)
#
# shellcheck disable=SC2016 # jq programs are single-quoted on purpose
set -euo pipefail

HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
# shellcheck source=/dev/null # guard.sh, linted on its own
. "$HERE/guard.sh"
set -e

DIED=0
die() { echo "candidate: $*" >&2; DIED=1; exit 1; }
say() { echo "candidate: $*"; }
now_iso() { date -u +%Y-%m-%dT%H:%M:%SZ; }
now_ms() { perl -MTime::HiRes=time -e 'printf "%d\n", time * 1000'; }

REPO=$(git -C "$HERE" rev-parse --show-toplevel)
ROOT_ARG=${HB_CANDIDATE_ROOT:-$HOME/.local/state/hummingbird-candidate}
ROOT=
SLOT='' SHA=''

init_root() {
  mkdir -p "$ROOT_ARG"
  ROOT=$(cd "$ROOT_ARG" && pwd -P)
  case "$REPO/" in
    "$ROOT/"*) die "run this from a checkout outside the slot root, not from $REPO" ;;
  esac
}

# Sets every per-slot path and port for slot $1.
slot_paths() {
  local p
  p=$(cand_ports "$1") || die "unknown slot '$1' (expected a or b)"
  read -r WORKER_PORT INSPECTOR_PORT WEB_PORT <<<"$p"
  SLOT=$1
  SLOT_DIR=$ROOT/$1
  WT=$SLOT_DIR/hb-$1
  STATE=$SLOT_DIR/state
  LOGS=$SLOT_DIR/logs
  MANIFEST=$SLOT_DIR/manifest.json
  EVENTS=$SLOT_DIR/events.jsonl
  URL=http://127.0.0.1:$WEB_PORT
  # shellcheck disable=SC2034 # read by guard.sh's guard_refuse
  GUARD_EVENTS=$EVENTS GUARD_SLOT=$1
}

# mget <slot> <jq path>: one manifest field, empty when absent.
mget() {
  jq -r "$2 // empty" "$ROOT/$1/manifest.json" 2>/dev/null || true
}

# manifest_update <jq filter> [jq args...] on the current slot's manifest.
manifest_update() {
  local filter=$1 tmp=$MANIFEST.tmp.$$
  shift
  jq "$@" "$filter" "$MANIFEST" >"$tmp"
  mv "$tmp" "$MANIFEST"
}

# event <name> [key=value...]: one line on the current slot's events.jsonl.
# Integers, true, false and null stay JSON; everything else is a string.
event() {
  local name=$1 kv i=0 sha=${SHA:-}
  shift
  [ -n "$sha" ] || sha=$(mget "$SLOT" .sha)
  local args=(--arg ts "$(now_iso)" --arg slot "$SLOT" --arg sha "$sha" --arg event "$name")
  local obj='{ts:$ts, slot:$slot, sha:$sha, event:$event}'
  for kv in "$@"; do
    i=$((i + 1))
    if [[ ${kv#*=} =~ ^(-?[0-9]+|true|false|null)$ ]]; then
      args+=(--argjson "v$i" "${kv#*=}")
    else
      args+=(--arg "v$i" "${kv#*=}")
    fi
    obj+=" + {\"${kv%%=*}\": \$v$i}"
  done
  mkdir -p "$SLOT_DIR"
  jq -nc "${args[@]}" "$obj" >>"$EVENTS"
}

# The only environment any child process sees. Build steps keep the real
# HOME (cargo, rustup and the pnpm store live there); the running worker and
# preview get the slot's own empty HOME and XDG_CONFIG_HOME, so wrangler
# finds no login and neither process can read anything under the real HOME.
# CLOUDFLARE_INCLUDE_PROCESS_ENV=false keeps wrangler from copying this
# environment into the worker as secrets. Leave
# CLOUDFLARE_LOAD_DEV_VARS_FROM_DOT_ENV alone: false also stops `--env-file`
# loading, and the worker comes up with no ADMIN_SECRET (wrangler 4.120's
# getVarsForDev; with `--env-file` given it already skips `.dev.vars` and the
# default `.env` files).
clean_env() {
  CLEAN_ENV=(env -i "PATH=$PATH" "HOME=$HOME" "USER=${USER:-$(id -un)}"
    "LANG=${LANG:-en_US.UTF-8}" "TMPDIR=${TMPDIR:-/tmp}" "TERM=${TERM:-dumb}"
    CI=true WRANGLER_SEND_METRICS=false CLOUDFLARE_INCLUDE_PROCESS_ENV=false)
  RUN_ENV=("${CLEAN_ENV[@]}" "HOME=$STATE/home" "XDG_CONFIG_HOME=$STATE/xdg")
}

# run_in <dir> <cmd...>: print the command as it will run, then run it.
run_in() {
  local dir=$1
  shift
  printf '+ cd %q && %s\n' "$dir" "$(printf '%q ' "$@")"
  (cd "$dir" && "$@")
}

require_tools() {
  local t
  for t in "$@"; do
    command -v "$t" >/dev/null || die "$t is required and not on PATH"
  done
}

port_busy() {
  { lsof -nP -iTCP:"$1" -sTCP:LISTEN -t 2>/dev/null || true; } | head -1
}

http_code() {
  curl -s -o /dev/null -w '%{http_code}' --max-time "${2:-3}" "$1" 2>/dev/null || true
}

# --- the build lock ---------------------------------------------------------

LOCK_HELD=0
take_lock() {
  local lock=$ROOT/.build.lock pid
  if ! mkdir "$lock" 2>/dev/null; then
    pid=$(cat "$lock/pid" 2>/dev/null || true)
    if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
      die "another build (pid $pid) holds $lock"
    fi
    rm -rf "$lock"
    mkdir "$lock" || die "could not take $lock"
  fi
  echo $$ >"$lock/pid"
  LOCK_HELD=1
}

BUILD_ACTIVE=0 CUR_STEP=''
on_exit() {
  local rc=$?
  if [ "$BUILD_ACTIVE" = 1 ]; then
    BUILD_ACTIVE=0
    manifest_update '.status = "failed" | .failed_step = $s' --arg s "${CUR_STEP:-interrupted}" || true
    event build.failed step="${CUR_STEP:-interrupted}" || true
    echo "candidate: build of ${SHA:0:8} in slot $SLOT failed at step ${CUR_STEP:-interrupted}; last lines of $BUILD_LOG:" >&2
    tail -25 "$BUILD_LOG" >&2 || true
  elif [ "$rc" -ne 0 ] && [ "$DIED" = 0 ]; then
    echo "candidate: stopped unexpectedly (exit $rc); rerun under bash -x to see where" >&2
  fi
  if [ "$LOCK_HELD" = 1 ]; then rm -rf "$ROOT/.build.lock"; fi
  return $rc
}

# --- pick -------------------------------------------------------------------

pick_slot() {
  local sha=$1 s
  for s in a b; do
    if [ "$(mget "$s" .sha)" = "$sha" ] && [ "$(mget "$s" .status)" = ok ]; then
      echo "$s"
      return
    fi
  done
  for s in a b; do
    if [ -f "$ROOT/$s/manifest.json" ] && [ "$(mget "$s" .status)" != ok ]; then
      echo "$s"
      return
    fi
  done
  for s in a b; do
    [ -f "$ROOT/$s/manifest.json" ] || { echo "$s"; return; }
  done
  if [[ "$(mget b .built_at)" < "$(mget a .built_at)" ]]; then echo b; else echo a; fi
}

cmd_pick() {
  [ $# -eq 1 ] || die "usage: pick <sha|ref>"
  init_root
  local sha=$1
  if ! [[ $sha =~ ^[0-9a-f]{40}$ ]]; then
    sha=$(git -C "$REPO" rev-parse --verify --end-of-options "$1^{commit}" 2>/dev/null) ||
      die "pick takes a full SHA or a ref that resolves locally (it does not fetch)"
  fi
  pick_slot "$sha"
}

# --- running processes ------------------------------------------------------

# The pid in $1 if it is alive and is this slot's (its command names the
# slot's state dir); empty otherwise. A pid file can outlive its process
# (a crash, a reboot), and the number may since belong to anything.
our_pid() {
  local pid cmd
  [ -f "$1" ] || return 0
  pid=$(cat "$1")
  [[ $pid =~ ^[0-9]+$ ]] || return 0
  cmd=$(ps -p "$pid" -o command= 2>/dev/null || true)
  case "$cmd" in
    *"$STATE"*) echo "$pid" ;;
  esac
}

slot_running() {
  [ -n "$(our_pid "$STATE/wrangler.pid")" ] && [ -n "$(port_busy "$WORKER_PORT")" ]
}

stop_pidfile() {
  local file=$1 pid i
  pid=$(our_pid "$file")
  if [ -n "$pid" ]; then
    # `up` started it as a session leader, so its group is exactly its tree.
    kill -TERM -- "-$pid" 2>/dev/null || kill -TERM "$pid" 2>/dev/null || true
    for i in $(seq 1 20); do
      kill -0 "$pid" 2>/dev/null || break
      sleep 0.5
    done
    if kill -0 "$pid" 2>/dev/null; then
      kill -KILL -- "-$pid" 2>/dev/null || kill -KILL "$pid" 2>/dev/null || true
    fi
  elif [ -f "$file" ]; then
    say "$file names no live process of this slot; leaving that pid alone"
  fi
  rm -f "$file"
}

cmd_down() {
  [ $# -eq 1 ] || die "usage: down <slot>"
  init_root
  local SHA=
  slot_paths "$1"
  stop_pidfile "$STATE/preview.pid"
  stop_pidfile "$STATE/wrangler.pid"
  local p holder i
  for p in "$WEB_PORT" "$WORKER_PORT" "$INSPECTOR_PORT"; do
    for i in $(seq 1 10); do
      [ -z "$(port_busy "$p")" ] && break
      sleep 0.5
    done
    holder=$(port_busy "$p")
    [ -z "$holder" ] || die "port $p is still held by pid $holder ($(ps -p "$holder" -o command= | cut -c1-120)); not ours to kill"
  done
  event down
  say "slot $SLOT is down"
}

# --- build ------------------------------------------------------------------

# Sets SHA (and PR, PR_REF, PR_TITLE for a PR) from a build argument.
resolve_rev() {
  local rev=$1 info branches
  PR='' PR_REF='' PR_TITLE=''
  if [[ $rev =~ ^[0-9]+$ ]]; then
    require_tools gh
    info=$(cd "$REPO" && gh pr view "$rev" --json headRefOid,headRefName,title,isCrossRepository) ||
      die "gh pr view $rev failed"
    [ "$(jq -r .isCrossRepository <<<"$info")" = false ] ||
      die "PR #$rev comes from a fork; building it would run someone else's code as you"
    git -C "$REPO" fetch -q origin "pull/$rev/head"
    SHA=$(git -C "$REPO" rev-parse FETCH_HEAD)
    [ "$SHA" = "$(jq -r .headRefOid <<<"$info")" ] ||
      die "PR #$rev moved between gh pr view and the fetch; run again"
    PR=$rev
    PR_REF=$(jq -r .headRefName <<<"$info")
    PR_TITLE=$(jq -r .title <<<"$info")
  else
    git -C "$REPO" fetch -q origin
    SHA=$(git -C "$REPO" rev-parse --verify --end-of-options "$rev^{commit}" 2>/dev/null) ||
      die "$rev does not name a commit (use origin/main for main)"
    # A local ref can hold anyone's code (`gh pr checkout` of a fork PR makes
    # one); only a commit some origin branch contains was pushed here.
    # (Captured first: a `grep -q` in the pipe can SIGPIPE git under pipefail.)
    branches=$(git -C "$REPO" branch -r --contains "$SHA" --format='%(refname)')
    grep -q '^refs/remotes/origin/' <<<"$branches" ||
      die "${SHA:0:8} ($rev) is on no origin branch; build a PR by its number so its source is checked"
  fi
}

# Megabytes under the given paths; a missing path counts as 0.
dir_mb() {
  { du -sk "$@" 2>/dev/null || true; } | awk '{s += $1} END {print int(s / 1024)}'
}

step_checkout() {
  if [ "${WIPE_TARGETS:-0}" = 1 ]; then
    run_in "$WT" rm -rf client/target server/target
  fi
  if [ -d "$WT" ]; then
    local dirty f
    # cargo rewrites a stale tracked Cargo.lock during the build (CI does not
    # pass --locked either); that is build output, not a change to keep.
    for f in client/Cargo.lock server/Cargo.lock; do
      git -C "$WT" diff --quiet -- "$f" || run_in "$WT" git checkout -q -- "$f"
    done
    dirty=$(git -C "$WT" status --porcelain --untracked-files=no)
    [ -z "$dirty" ] || { echo "$WT has local changes to tracked files:"; echo "$dirty"; return 1; }
    run_in "$WT" git checkout -q --detach "$SHA"
  else
    run_in "$REPO" git worktree add -q --detach --lock --reason "hummingbird candidate slot" "$WT" "$SHA"
  fi
  [ "$(git -C "$WT" rev-parse HEAD)" = "$SHA" ]
  GUARD_FAILS=0
  guard_files "$SLOT_DIR"
  [ "$GUARD_FAILS" -eq 0 ]
}

step_install() {
  run_in "$WT/client/web" "${CLEAN_ENV[@]}" pnpm install --frozen-lockfile --prefer-offline
}

step_worker() {
  run_in "$WT/server/worker" "${CLEAN_ENV[@]}" worker-build --release
}

step_web() {
  run_in "$WT/client/web" "${CLEAN_ENV[@]}" pnpm build
}

# The worker config, from this revision's own wrangler.toml so its bindings
# and migrations carry over: an allowlist of keys, `main` pointed at the
# prebuilt shim (so wrangler runs no build and watches nothing), and the
# build/routes/observability keys dropped. Any key not named here fails the
# build: a revision that adds a binding needs a person to decide.
gen_wrangler_config() {
  echo "+ generate $STATE/wrangler.json from $WT/server/worker/wrangler.toml"
  python3 - "$WT/server/worker/wrangler.toml" "$WT/server/worker/build/worker/shim.mjs" <<'PY' >"$STATE/wrangler.json.tmp"
import json, sys, tomllib
c = tomllib.load(open(sys.argv[1], "rb"))
keep = ("name", "compatibility_date", "compatibility_flags", "durable_objects", "migrations")
dropped = {"main", "build", "routes", "route", "observability"}
unknown = sorted(set(c) - set(keep) - dropped)
if unknown:
    sys.exit("wrangler.toml has keys candidate.sh does not know how to run locally: "
             + ", ".join(unknown) + " (teach gen_wrangler_config and guard.sh, deliberately)")
out = {k: c[k] for k in keep if k in c}
out["main"] = sys.argv[2]
json.dump(out, sys.stdout, indent=2)
print()
PY
  mv "$STATE/wrangler.json.tmp" "$STATE/wrangler.json"
}

# The preview config. Generated rather than read from the revision's
# vite.config.ts, whose dev proxy is hard-wired to 8787: a plain object
# with no imports, so it loads from outside the project for any revision.
gen_preview_config() {
  echo "+ generate $STATE/vite.preview.config.mjs"
  {
    printf 'export default '
    jq -n --arg root "$WT/client/web" --argjson port "$WEB_PORT" \
      --arg target "http://127.0.0.1:$WORKER_PORT" \
      '{root: $root, preview: {host: "127.0.0.1", port: $port, strictPort: true,
        proxy: {"/api": {target: $target, changeOrigin: false}}}}'
    printf ';\n'
  } >"$STATE/vite.preview.config.mjs"
}

step_config() {
  gen_wrangler_config
  gen_preview_config
  if [ "$FRESH" = 1 ]; then
    run_in "$STATE" rm -rf persist device-token
  fi
}

step_guard() {
  guard_static "$SLOT_DIR"
}

step() {
  local name=$1 t0 rc ms
  shift
  CUR_STEP=$name
  t0=$(now_ms)
  say "slot $SLOT: $name"
  printf '\n### step %s (%s)\n' "$name" "$(now_iso)" >>"$BUILD_LOG"
  set +e
  (set -e; "$@") >>"$BUILD_LOG" 2>&1
  rc=$?
  set -e
  ms=$(($(now_ms) - t0))
  event build.step step="$name" ms="$ms" ok="$([ "$rc" -eq 0 ] && echo true || echo false)"
  [ "$rc" -eq 0 ] || exit 1
}

# Refuses to carry state backwards across a schema change (see the header).
check_state_carry() {
  local old
  old=$(cat "$STATE/persist/.written-by" 2>/dev/null || true)
  # A slot from before the marker: its manifest's SHA is the best guess.
  [ -n "$old" ] || old=$(mget "$SLOT" .sha)
  [ -n "$old" ] && [ "$old" != "$SHA" ] || return 0
  [ -d "$STATE/persist" ] && [ -n "$(ls -A "$STATE/persist" 2>/dev/null)" ] || return 0
  [ "$FRESH" = 1 ] || [ "$KEEP_STATE" = 1 ] && return 0
  git -C "$REPO" merge-base --is-ancestor "$old" "$SHA" 2>/dev/null && return 0
  git -C "$REPO" diff --quiet "$old" "$SHA" -- server/authority/src/schema.rs server/worker/wrangler.toml 2>/dev/null &&
    return 0
  die "slot $SLOT holds state written by ${old:0:8}; ${SHA:0:8} is not its descendant and changes the schema or worker config. Pass --keep-state to run on it anyway, or wipe it with build --fresh."
}

check_disk() {
  local free need floor=${HB_CANDIDATE_FLOOR_MB:-500} tmb=0
  free=$(df -Pk "$ROOT" | awk 'NR == 2 {print int($4 / 1024)}')
  if [ -d "$WT" ]; then need=${HB_CANDIDATE_WARM_MB:-300}; else need=${HB_CANDIDATE_COLD_MB:-900}; fi
  WIPE_TARGETS=0
  if [ -d "$WT" ]; then
    tmb=$(dir_mb "$WT/client/target" "$WT/server/target")
    if [ "$tmb" -gt "${HB_CANDIDATE_TARGET_MAX_MB:-800}" ]; then
      WIPE_TARGETS=1
      need=${HB_CANDIDATE_COLD_MB:-900}
      free=$((free + tmb))
    fi
  fi
  [ "$free" -ge $((need + floor)) ] ||
    die "only ${free} MB free on $ROOT's volume; this build needs ${need} MB plus a ${floor} MB floor"
}

cmd_build() {
  local rev='' want_slot='' t0 fetch_ms tools
  FRESH=0 KEEP_STATE=0
  while [ $# -gt 0 ]; do
    case "$1" in
      --slot) want_slot=${2:-}; shift 2 ;;
      --fresh) FRESH=1; shift ;;
      --keep-state) KEEP_STATE=1; shift ;;
      -*) die "unknown flag $1" ;;
      *) [ -z "$rev" ] || die "one revision per build"; rev=$1; shift ;;
    esac
  done
  [ -n "$rev" ] || die "usage: build <rev> [--slot a|b] [--fresh|--keep-state]"
  [ "$FRESH$KEEP_STATE" != 11 ] || die "--fresh and --keep-state contradict each other"

  require_tools jq curl git pnpm worker-build python3 openssl perl lsof
  python3 -c 'import tomllib' 2>/dev/null || die "python3 >= 3.11 is required (tomllib reads wrangler.toml)"
  init_root
  GUARD_FAILS=0
  guard_env
  guard_root "$ROOT"
  [ "$GUARD_FAILS" -eq 0 ] || die "refused before building (see REFUSED above)"
  take_lock

  t0=$(now_ms)
  resolve_rev "$rev"
  fetch_ms=$(($(now_ms) - t0))
  slot_paths "${want_slot:-$(pick_slot "$SHA")}"

  if [ "$(mget "$SLOT" .sha)" = "$SHA" ] && [ "$(mget "$SLOT" .status)" = ok ]; then
    if [ "$FRESH" = 1 ]; then
      clean_env
      slot_running && cmd_down "$SLOT"
      rm -rf "$STATE/persist" "$STATE/device-token"
      event fresh
      say "slot $SLOT already holds ${SHA:0:8}; wiped its state. Next: up $SLOT, mint $SLOT, seed $SLOT"
    else
      event build.noop
      say "already built in slot $SLOT (${SHA:0:8}); run: up $SLOT"
    fi
    return 0
  fi

  check_state_carry
  check_disk
  clean_env
  if slot_running || [ -f "$STATE/preview.pid" ]; then
    cmd_down "$SLOT"
  fi

  mkdir -p "$STATE/home" "$STATE/xdg" "$LOGS"
  chmod 700 "$STATE"
  BUILD_LOG=$LOGS/build-$(date -u +%Y%m%dT%H%M%SZ).log
  : >"$BUILD_LOG"
  tools=$(jq -n --arg wb "$(worker-build --version 2>/dev/null | awk '{print $NF}')" \
    --arg node "$(node --version 2>/dev/null)" --arg pnpm "$(cd "$HOME" && pnpm --version 2>/dev/null)" \
    '{worker_build: $wb, node: $node, pnpm: $pnpm}')
  jq -n --arg slot "$SLOT" --arg sha "$SHA" --arg rev "$rev" --arg pr "$PR" --arg pr_ref "$PR_REF" \
    --arg pr_title "$PR_TITLE" --arg started "$(now_iso)" --argjson fresh "$([ "$FRESH" = 1 ] && echo true || echo false)" \
    --argjson keep "$([ "$KEEP_STATE" = 1 ] && echo true || echo false)" \
    --argjson w "$WORKER_PORT" --argjson i "$INSPECTOR_PORT" --argjson web "$WEB_PORT" --arg url "$URL" \
    --arg wt "$WT" --arg state "$STATE" --arg log "$BUILD_LOG" --argjson tools "$tools" '
    {schema: 1, slot: $slot, sha: $sha, short: $sha[0:8], rev_arg: $rev,
     pr: (if $pr == "" then null else ($pr | tonumber) end),
     pr_head_ref: (if $pr_ref == "" then null else $pr_ref end),
     pr_title: (if $pr_title == "" then null else $pr_title end),
     status: "building", failed_step: null, started_at: $started, built_at: null, build_ms: null,
     fresh_state: $fresh, keep_state: $keep, ports: {worker: $w, inspector: $i, web: $web}, url: $url,
     worktree: $wt, state_dir: $state, build_log: $log, tools: $tools, target_mb: null}' \
    >"$MANIFEST.tmp.$$"
  mv "$MANIFEST.tmp.$$" "$MANIFEST"
  BUILD_ACTIVE=1
  event build.start rev="$rev"
  event build.step step=fetch ms="$fetch_ms" ok=true
  {
    echo "# candidate build: slot $SLOT, rev $rev, sha $SHA"
    echo "# child environment: $(printf '%q ' "${CLEAN_ENV[@]}")"
  } >>"$BUILD_LOG"

  step checkout step_checkout
  step install step_install
  step worker step_worker
  step web step_web
  step config step_config
  step guard step_guard

  local ms=$(($(now_ms) - t0)) tsizes
  tsizes=$(jq -n --argjson c "$(dir_mb "$WT/client/target")" --argjson s "$(dir_mb "$WT/server/target")" \
    '{client: $c, server: $s}')
  manifest_update '.status = "ok" | .built_at = $t | .build_ms = $ms | .target_mb = $tm' \
    --arg t "$(now_iso)" --argjson ms "$ms" --argjson tm "$tsizes"
  BUILD_ACTIVE=0
  event build.ok ms="$ms"
  say "built ${SHA:0:8} into slot $SLOT in $((ms / 1000)) s; next: up $SLOT"
}

# --- up ---------------------------------------------------------------------

# Start "$@" detached (its own session, so a Ctrl-C or a terminal that kills
# its process tree leaves the slot running), output to $1; sets DETACHED_PID.
detach() {
  local log=$1
  shift
  printf '+ %s\n' "$(printf '%q ' "$@")" >>"$log"
  perl -MPOSIX -e 'POSIX::setsid() or warn "setsid: $!\n"; exec @ARGV or die "exec $ARGV[0]: $!\n"' \
    "$@" </dev/null >>"$log" 2>&1 &
  DETACHED_PID=$!
}

up_fail() {
  local why
  event up.failed reason="$1"
  # In a subshell: cmd_down's own die (a port still held) must not end this
  # script before the reason is printed.
  why=$( (cmd_down "$SLOT") 2>&1 >/dev/null) ||
    echo "candidate: teardown after the failure did not finish: ${why#candidate: }" >&2
  die "up $SLOT failed: $1"
}

# The binding table wrangler prints at startup: exactly AUTHORITY (a Durable
# Object) and ADMIN_SECRET (from admin.env), every row local. A missing
# table fails too.
check_bindings() {
  python3 - "$LOGS/wrangler.log" <<'PY'
import re, sys
text = re.sub(r"\x1b\[[0-9;]*m", "", open(sys.argv[1], errors="replace").read())
m = re.search(r"following bindings:\n(.*?)(?:\n\s*\n|\Z)", text, re.S)
if not m:
    sys.exit("no binding table in wrangler.log")
rows = [l.split() for l in m.group(1).splitlines() if l.strip().startswith("env.")]
names = {r[0].split(".", 1)[1] for r in rows}
bad = [" ".join(r) for r in rows if r[-1] != "local"]
if bad:
    sys.exit("binding not local: " + "; ".join(bad))
for need in ("AUTHORITY", "ADMIN_SECRET"):
    if need not in names:
        sys.exit(f"no {need} binding")
if names - {"AUTHORITY", "ADMIN_SECRET"}:
    sys.exit("unexpected bindings: " + ", ".join(sorted(names - {"AUTHORITY", "ADMIN_SECRET"})))
PY
}

cmd_up() {
  local slot='' keep=0
  for a in "$@"; do
    case "$a" in --keep-state) keep=1 ;; -*) die "unknown flag $a" ;; *) [ -z "$slot" ] || die "usage: up <slot> [--keep-state]"; slot=$a ;; esac
  done
  [ -n "$slot" ] || die "usage: up <slot> [--keep-state]"
  require_tools jq curl git python3 openssl perl lsof
  init_root
  slot_paths "$slot"
  clean_env
  local t0 p code i why
  t0=$(now_ms)
  [ "$(mget "$SLOT" .status)" = ok ] || die "slot $SLOT has no good build (status: $(mget "$SLOT" .status)); build first"
  slot_running && die "slot $SLOT is already running at $URL"
  # The same test as build's, against whatever persist/ holds now (a copy
  # from the other slot, say); a build given --keep-state carries it here.
  SHA=$(mget "$SLOT" .sha) FRESH=0 KEEP_STATE=$keep
  [ "$(mget "$SLOT" .keep_state)" != true ] || KEEP_STATE=1
  check_state_carry
  for p in "$WORKER_PORT" "$INSPECTOR_PORT" "$WEB_PORT"; do
    [ -z "$(port_busy "$p")" ] || die "port $p is already in use (pid $(port_busy "$p"))"
  done
  mkdir -p "$STATE/home" "$STATE/xdg" "$LOGS"
  if [ ! -f "$STATE/admin.env" ]; then
    (umask 077 && printf 'ADMIN_SECRET=%s\n' "$(openssl rand -hex 32)" >"$STATE/admin.env")
  fi
  guard_static "$SLOT_DIR" || up_fail guard
  # Before the worker starts: its first request (the probe below) migrates
  # persist/ to this revision's schema, whether or not `up` then succeeds.
  mkdir -p "$STATE/persist" && echo "$SHA" >"$STATE/persist/.written-by"

  : >"$LOGS/wrangler.log"
  detach "$LOGS/wrangler.log" "${RUN_ENV[@]}" "$WT/client/web/node_modules/.bin/wrangler" dev \
    -c "$STATE/wrangler.json" --local --ip 127.0.0.1 --port "$WORKER_PORT" \
    --inspector-ip 127.0.0.1 --inspector-port "$INSPECTOR_PORT" --local-protocol http \
    --local-upstream "127.0.0.1:$WORKER_PORT" --persist-to "$STATE/persist" \
    --env-file "$STATE/admin.env" --show-interactive-dev-session=false
  echo "$DETACHED_PID" >"$STATE/wrangler.pid"

  # Ready when an unauthenticated read is refused; a 200 means auth is off.
  code=
  for i in $(seq 1 60); do
    code=$(http_code "http://127.0.0.1:$WORKER_PORT/api/changes?since=0")
    [ "$code" = 401 ] && break
    [ "$code" = 200 ] && up_fail "the worker answered an unauthenticated read with 200 (auth is off)"
    kill -0 "$DETACHED_PID" 2>/dev/null || break
    sleep 1
  done
  [ "$code" = 401 ] || { tail -20 "$LOGS/wrangler.log" >&2; up_fail "worker not ready (last answer: ${code:-none}); see $LOGS/wrangler.log"; }
  why=$(check_bindings 2>&1) || up_fail "binding check: $why"

  : >"$LOGS/preview.log"
  detach "$LOGS/preview.log" "${RUN_ENV[@]}" "$WT/client/web/node_modules/.bin/vite" preview \
    --config "$STATE/vite.preview.config.mjs"
  echo "$DETACHED_PID" >"$STATE/preview.pid"
  code=
  for i in $(seq 1 30); do
    code=$(http_code "http://127.0.0.1:$WEB_PORT/api/changes?since=0")
    [ "$code" = 401 ] && break
    kill -0 "$DETACHED_PID" 2>/dev/null || break
    sleep 1
  done
  [ "$code" = 401 ] || { tail -20 "$LOGS/preview.log" >&2; up_fail "preview /api did not reach the worker (last answer: ${code:-none})"; }
  [ "$(http_code "$URL/")" = 200 ] || up_fail "preview did not serve /"

  event up.ok ms="$(($(now_ms) - t0))"
  local pr
  pr=$(mget "$SLOT" .pr)
  say "slot $SLOT is up: $URL ($(mget "$SLOT" .short)${pr:+, PR #$pr})"
  say "same origin as this slot's previous revision: check the build string in the nav rail footer or Settings, and accept the update banner if it is stale"
  [ -s "$STATE/device-token" ] || say "no token yet: mint $SLOT --copy, then paste it into Settings"
}

# --- mint, seed -------------------------------------------------------------

worker_base_checked() {
  BASE=http://127.0.0.1:$WORKER_PORT
  local code
  code=$(http_code "$BASE/api/changes?since=0")
  [ "$code" = 401 ] || die "slot $SLOT's worker at $BASE is not answering as expected (got ${code:-nothing}); run up $SLOT"
}

cmd_mint() {
  local copy=0 slot=
  for a in "$@"; do
    case "$a" in --copy) copy=1 ;; *) slot=$a ;; esac
  done
  [ -n "$slot" ] || die "usage: mint <slot> [--copy]"
  require_tools jq curl
  init_root
  slot_paths "$slot"
  GUARD_FAILS=0
  guard_env
  [ "$GUARD_FAILS" -eq 0 ] || die "refused (see REFUSED above)"
  [ -f "$STATE/admin.env" ] || die "slot $SLOT has no admin.env; run up $SLOT"
  worker_base_checked
  local id resp code body
  id=candidate-$SLOT-$(date -u +%Y%m%d%H%M%S)
  body=$(jq -nc --arg id "$id" --arg name "candidate slot $SLOT" '{id: $id, name: $name, scope: "device"}')
  resp=$STATE/.mint-response
  (umask 077 && : >"$resp")
  # The secret goes from the mode-600 file to curl's stdin as a header line:
  # never an argument, never a shell variable.
  code=$(sed -n 's/^ADMIN_SECRET=/Authorization: Bearer /p' "$STATE/admin.env" |
    curl -s -o "$resp" -w '%{http_code}' --max-time 10 -X POST -H @- \
      -H 'content-type: application/json' --data-binary "$body" "$BASE/api/admin/tokens") || true
  if [ "$code" != 201 ]; then
    rm -f "$resp"
    die "mint answered ${code:-nothing}, not 201"
  fi
  (umask 077 && jq -r '.token // empty' "$resp" >"$STATE/device-token.tmp")
  rm -f "$resp"
  [ -s "$STATE/device-token.tmp" ] || { rm -f "$STATE/device-token.tmp"; die "the mint response carried no token"; }
  mv "$STATE/device-token.tmp" "$STATE/device-token"
  event mint token_id="$id"
  say "device token for slot $SLOT ($id): $STATE/device-token"
  if [ "$copy" = 1 ]; then
    local clip=()
    if command -v pbcopy >/dev/null; then clip=(pbcopy)
    elif command -v clip.exe >/dev/null; then clip=(clip.exe)
    elif command -v wl-copy >/dev/null; then clip=(wl-copy)
    elif command -v xclip >/dev/null; then clip=(xclip -selection clipboard)
    fi
    if [ ${#clip[@]} -gt 0 ] && tr -d '\n' <"$STATE/device-token" | "${clip[@]}"; then
      say "copied to the clipboard; paste it into $URL's Settings"
    else
      say "not copied (no working clipboard tool); the token is in $STATE/device-token"
    fi
  fi
}

# api <method> <path> [body]: one authenticated call to the slot's worker.
# Sets API_CODE and API_BODY. The token reaches curl on stdin, as a header.
api() {
  local out data=()
  [ $# -lt 3 ] || data=(--data-binary "$3")
  out=$(mktemp "$STATE/.api.XXXXXX")
  API_CODE=$(sed 's/^/Authorization: Bearer /' "$STATE/device-token" |
    curl -s -o "$out" -w '%{http_code}' --max-time 10 -X "$1" -H @- \
      -H 'content-type: application/json' ${data[@]+"${data[@]}"} "$BASE$2") || true
  API_BODY=$(cat "$out")
  rm -f "$out"
}

cmd_seed() {
  [ $# -eq 1 ] || die "usage: seed <slot>"
  require_tools jq curl
  init_root
  slot_paths "$1"
  [ -s "$STATE/device-token" ] || die "slot $SLOT has no device token; run mint $SLOT"
  worker_base_checked
  local seed=$HERE/seed.json fails=0 n kind obj id version
  for kind in projects items grills; do
    n=$(jq ".$kind | length" "$seed")
    for ((i = 0; i < n; i++)); do
      obj=$(jq -c --argjson i "$i" ".${kind}[\$i]" "$seed")
      id=$(jq -r .id <<<"$obj")
      if [ "$kind" = items ]; then
        obj=$(jq -c 'if has("deadline_in_min")
          then .deadline = ((now + .deadline_in_min * 60) | strflocaltime("%Y-%m-%dT%H:%M")) | del(.deadline_in_min)
          else . end' <<<"$obj")
      fi
      if [ "$kind" = grills ]; then
        api GET "/api/changes?since=0"
        version=$(jq --arg id "$(jq -r .item_id <<<"$obj")" '.items[] | select(.id == $id) | .version' <<<"$API_BODY" 2>/dev/null || true)
        [ -n "$version" ] || { say "seed: no item for grill $id"; fails=$((fails + 1)); continue; }
        obj=$(jq -c --argjson v "$version" '.expected_version = $v' <<<"$obj")
      fi
      api POST "/api/$kind" "$obj"
      case "$API_CODE" in
        200 | 201) ;;
        *) say "seed: POST /api/$kind $id answered $API_CODE: ${API_BODY:0:200}"; fails=$((fails + 1)) ;;
      esac
    done
  done
  event seed failures="$fails"
  [ "$fails" -eq 0 ] || die "seed finished with $fails failure(s); slot $SLOT is still running"
  say "seeded slot $SLOT from $seed"
}

# --- status -----------------------------------------------------------------

cmd_status() {
  init_root
  local json=0 s rows=() running token
  [ "${1:-}" = --json ] && json=1
  for s in a b; do
    slot_paths "$s"
    running=false
    slot_running && running=true
    token=false
    [ -s "$STATE/device-token" ] && token=true
    if [ -f "$MANIFEST" ]; then
      rows+=("$(jq -c --argjson r "$running" --argjson t "$token" '. + {running: $r, token_present: $t}' "$MANIFEST")")
    else
      rows+=("$(jq -nc --arg s "$s" --arg url "$URL" '{slot: $s, status: null, url: $url, running: false, token_present: false}')")
    fi
  done
  if [ "$json" = 1 ]; then
    printf '%s\n' "${rows[@]}" | jq -s .
    return
  fi
  printf '%s\n' "${rows[@]}" | jq -r '
    [.slot, (.short // "-"), (if .pr then "#\(.pr)" else (.rev_arg // "-") end), (.status // "empty"),
     (.built_at // "-"), (if .build_ms then "\(.build_ms / 1000 | floor)s" else "-" end),
     (if .running then "up" else "down" end), .url, (if .token_present then "token" else "no-token" end),
     (.pr_title // "")] | @tsv' | column -t -s $'\t'
}

# Sourced (tests/test_candidate_guard.py does), it only defines functions.
if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  case "${1:-}" in
    build | up | down | status | mint | seed | pick)
      cmd=$1
      shift
      trap on_exit EXIT
      "cmd_$cmd" "$@"
      ;;
    *)
      sed -n '/^#   scripts/p' "$0" >&2
      exit 2
      ;;
  esac
fi
