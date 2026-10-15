#!/usr/bin/env bash
# The production guard for the local review candidates. The component doc is
# `candidate.sh`'s header; this file is the policy half of it, kept apart so a
# person (or a test) can run every static check against a slot without
# building anything:
#
#   scripts/candidate/guard.sh <slot-dir>     e.g. ~/.local/state/hummingbird-candidate/a
#
# Exit 0 means every check ran and passed. Exit 1 prints one
# `REFUSED: <reason>` line per failed check. **A check that cannot run is a
# refusal** (no `dist/`, no `python3`, an unparseable config): this guard
# fails closed, never open.
#
# `candidate.sh` sources this file. That makes it the one home of the fixed
# port table (`cand_ports`) and of the environment names the guard refuses
# (`CAND_FORBIDDEN_ENV`), so neither is spelled twice.
#
# **What it is, and what it is not.** It catches *configuration* mistakes
# that would point a candidate at production: an exported API base or
# Cloudflare credential, a committed `.env`/`.dev.vars`, a generated wrangler
# config with a route, a remote binding or an extra binding, a preview proxy
# aimed anywhere but this slot's own local worker, and a production hostname
# baked into the built bundle or worker. It is **not a sandbox**. Building a
# revision runs that revision's code as you (pnpm lifecycle scripts,
# `build.rs`, `vite.config.ts`, `build-version.node.ts`) with your real
# `HOME`, and nothing static can stop such code reaching your real token or
# the production host. The defence there is upstream: `candidate.sh` builds
# only a same-repo PR's head or a commit some `origin/*` branch contains, so
# only code someone with push access to this repo put there is ever built.
#
# The checks, and why each exists:
#
#   env        the names in CAND_FORBIDDEN_ENV must be unset or empty. Every
#              child process starts from an allowlisted environment anyway
#              (`candidate.sh`'s `clean_env`); this is the loud error for
#              the person who exported one. A legacy `~/.wrangler/` is
#              refused too: wrangler prefers it over `XDG_CONFIG_HOME`, so
#              it would defeat the empty config dir the worker runs under.
#   root       the slot root is outside every git work tree and outside the
#              real token's config dir; the worktree sits under the root.
#   files      no `client/web/.env*` but `.env.example`, no
#              `server/worker/{.dev.vars*,.env*}` in the worktree, none of
#              those in `state/`; `admin.env` (when present) is mode 600 and
#              is exactly one `ADMIN_SECRET=<64 hex>` line; `device-token`
#              (when present) is mode 600. Modes are read in Python because
#              BSD and GNU `stat` disagree on flags.
#   wrangler   the generated `state/wrangler.json` holds only the allowlisted
#              keys, is named `hummingbird-authority` with an `Authority`
#              Durable Object (the persisted state is filed under that pair),
#              its `main` is inside this slot's worktree, nothing anywhere in
#              it says `remote: true`, and it contains no URL at all.
#   preview    the generated `state/vite.preview.config.mjs` is exactly
#              `export default <json>;`, binds 127.0.0.1 on this slot's web
#              port with strictPort, and the only URL in it is this slot's
#              own worker, `http://127.0.0.1:<worker port>`.
#   artifacts  nothing under `client/web/dist` or `server/worker/build`
#              (wasm searched as text) names twinion.net or workers.dev.
#              Should a legitimate hit ever appear, allowlist that exact
#              string here; do not loosen the pattern.
set -uo pipefail

# The fixed port table: worker, inspector, web. Clear of wrangler's 8787 and
# 9229 defaults, vite's 5173/4173, and anything else on the Mac today.
cand_ports() {
  case "$1" in
    a) echo "8801 9231 4801" ;;
    b) echo "8802 9232 4802" ;;
    *) return 1 ;;
  esac
}

CAND_FORBIDDEN_ENV=(
  VITE_API_BASE_URL HB_API_BASE HB_API_TOKEN HB_API_TOKEN_PATH
  CLOUDFLARE_API_TOKEN CLOUDFLARE_ACCOUNT_ID CLOUDFLARE_API_KEY CLOUDFLARE_EMAIL
  CLOUDFLARE_ENV CLOUDFLARE_API_BASE_URL CF_API_TOKEN CF_ACCOUNT_ID
)

GUARD_FAILS=0

# One refusal: the printed line, plus a `guard.refused` event when the caller
# (candidate.sh) has named an events file.
guard_refuse() {
  echo "REFUSED: $1" >&2
  GUARD_FAILS=$((GUARD_FAILS + 1))
  if [ -n "${GUARD_EVENTS:-}" ] && command -v jq >/dev/null; then
    jq -nc --arg ts "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --arg slot "${GUARD_SLOT:-}" \
      --arg reason "$1" '{ts:$ts, slot:$slot, event:"guard.refused", reason:$reason}' \
      >>"$GUARD_EVENTS" 2>/dev/null || true
  fi
}

guard_env() {
  local name
  for name in "${CAND_FORBIDDEN_ENV[@]}"; do
    if [ -n "${!name:-}" ]; then
      guard_refuse "$name is set in the environment; unset it (candidates are local-only)"
    fi
  done
  if [ -d "$HOME/.wrangler" ]; then
    guard_refuse "legacy $HOME/.wrangler exists; wrangler would prefer its login over the slot's empty config dir"
  fi
}

# Physical path of an existing directory, or empty.
guard_realdir() {
  (cd "$1" 2>/dev/null && pwd -P)
}

guard_root() {
  local root
  root=$(guard_realdir "$1" || true)
  if [ -z "$root" ]; then
    guard_refuse "slot root $1 does not exist"
    return
  fi
  if [ "$(git -C "$root" rev-parse --is-inside-work-tree 2>/dev/null)" = "true" ]; then
    guard_refuse "slot root $root is inside a git work tree"
  fi
  local cfg
  cfg=$(guard_realdir "$HOME/.config/hummingbird" || true)
  case "$root/" in
    "${cfg:-/nonexistent}/"*) guard_refuse "slot root $root is under $HOME/.config/hummingbird" ;;
  esac
}

# The Python half: every check that parses JSON or reads a file mode. Prints
# one reason per line and exits 0; any other exit means the check itself
# could not run, which the caller turns into a refusal.
guard_py() {
  python3 - "$@" <<'PY'
import json, os, re, stat, sys

check, slot_dir, slot, wt, state = sys.argv[1:6]
worker_port, web_port = sys.argv[6], sys.argv[7]
out = []
URL = re.compile(r"[A-Za-z][A-Za-z0-9+.-]*://[^\s\"'`,)}\]]*")

def mode_600(path, label):
    m = stat.S_IMODE(os.stat(path).st_mode)
    if m != 0o600:
        out.append(f"{label} is mode {m:o}, not 600")

def walk(v):
    if isinstance(v, dict):
        for k, x in v.items():
            yield k, x
            yield from walk(x)
    elif isinstance(v, list):
        for x in v:
            yield from walk(x)

def strings(v):
    if isinstance(v, str):
        yield v
    elif isinstance(v, dict):
        for k, x in v.items():
            yield k
            yield from strings(x)
    elif isinstance(v, list):
        for x in v:
            yield from strings(x)

if check == "files":
    for d, pats in ((os.path.join(wt, "client/web"), (".env",)),
                    (os.path.join(wt, "server/worker"), (".env", ".dev.vars")),
                    (state, (".env", ".dev.vars"))):
        if not os.path.isdir(d):
            continue
        for name in sorted(os.listdir(d)):
            if name == ".env.example" and d.endswith("client/web"):
                continue
            if name.startswith(pats):
                out.append(f"{os.path.join(d, name)} exists; candidates load no env/dev-vars files")
    admin = os.path.join(state, "admin.env")
    if os.path.lexists(admin):
        mode_600(admin, admin)
        lines = open(admin).read().splitlines()
        if len(lines) != 1 or not re.fullmatch(r"ADMIN_SECRET=[0-9a-f]{64}", lines[0]):
            out.append(f"{admin} must be exactly one ADMIN_SECRET=<64 hex> line")
    token = os.path.join(state, "device-token")
    if os.path.lexists(token):
        mode_600(token, token)

elif check == "wrangler":
    path = os.path.join(state, "wrangler.json")
    c = json.load(open(path))
    allowed = {"name", "main", "compatibility_date", "compatibility_flags",
               "durable_objects", "migrations"}
    extra = sorted(set(c) - allowed)
    if extra:
        out.append(f"wrangler.json has keys outside the allowlist: {', '.join(extra)}")
    if c.get("name") != "hummingbird-authority":
        out.append(f"wrangler.json name is {c.get('name')!r}, not 'hummingbird-authority'")
    bindings = (c.get("durable_objects") or {}).get("bindings") or []
    if not any(b.get("class_name") == "Authority" for b in bindings):
        out.append("wrangler.json has no Durable Object binding with class_name 'Authority'")
    for b in bindings:
        if set(b) - {"name", "class_name"}:
            out.append(f"wrangler.json Durable Object binding has extra keys: {sorted(set(b) - {'name', 'class_name'})}")
    main = c.get("main")
    real_wt = os.path.realpath(wt)
    if not (isinstance(main, str) and os.path.isabs(main)
            and os.path.realpath(main).startswith(real_wt + os.sep)
            and os.path.isfile(main)):
        out.append(f"wrangler.json main {main!r} is not an existing absolute path inside {wt}")
    for k, v in walk(c):
        if k == "remote" and v is not False:
            out.append("wrangler.json sets remote somewhere")
    for s in strings(c):
        if re.search(r"https?://", s):
            out.append(f"wrangler.json contains a URL: {s}")

elif check == "preview":
    path = os.path.join(state, "vite.preview.config.mjs")
    text = open(path).read()
    target = f"http://127.0.0.1:{worker_port}"
    for u in URL.findall(text):
        if u != target:
            out.append(f"preview config names {u}; the only URL allowed is {target}")
    m = re.fullmatch(r"export default (\{.*\})\s*;\s*", text, re.S)
    if not m:
        out.append("preview config is not exactly `export default <json>;`")
    else:
        c = json.loads(m.group(1))
        if set(c) != {"root", "preview"}:
            out.append(f"preview config keys are {sorted(c)}, not ['preview', 'root']")
        if os.path.realpath(c.get("root", "")) != os.path.realpath(os.path.join(wt, "client/web")):
            out.append(f"preview root {c.get('root')!r} is not this slot's client/web")
        p = c.get("preview") or {}
        if p.get("host") != "127.0.0.1":
            out.append(f"preview host is {p.get('host')!r}, not '127.0.0.1'")
        if p.get("port") != int(web_port):
            out.append(f"preview port is {p.get('port')!r}, not {web_port}")
        if p.get("strictPort") is not True:
            out.append("preview strictPort is not true")
        proxy = p.get("proxy") or {}
        if set(proxy) != {"/api"} or (proxy.get("/api") or {}).get("target") != target:
            out.append(f"preview proxy must be exactly /api -> {target}")

else:
    sys.exit(f"unknown check {check}")

for line in dict.fromkeys(out):
    print(line)
PY
}

# The file checks alone, for `candidate.sh build` to run straight after
# checkout: before install and build read any committed env file.
guard_files() {
  guard_py_run files "$1" "${1##*/}"
}

guard_py_run() {
  local check=$1 slot_dir=$2 slot=$3 ports reasons rc line
  ports=$(cand_ports "$slot") || { guard_refuse "unknown slot '$slot' (expected a or b)"; return; }
  # shellcheck disable=SC2086
  set -- "$check" "$slot_dir" "$slot" "$slot_dir/hb-$slot" "$slot_dir/state" $ports
  if ! command -v python3 >/dev/null; then
    guard_refuse "python3 is not on PATH; the $check check cannot run"
    return
  fi
  rc=0
  reasons=$(guard_py "$1" "$2" "$3" "$4" "$5" "$6" "$8") || rc=$?
  if [ "$rc" -ne 0 ]; then
    guard_refuse "the $check check could not run (exit $rc)"
    return
  fi
  while IFS= read -r line; do
    if [ -n "$line" ]; then guard_refuse "$line"; fi
  done <<<"$reasons"
}

guard_artifacts() {
  local wt=$1 dir hits
  for dir in "$wt/client/web/dist" "$wt/server/worker/build"; do
    if [ ! -d "$dir" ]; then
      guard_refuse "$dir is missing; the artifact check cannot run"
      continue
    fi
    hits=$(grep -rlaE 'twinion\.net|workers\.dev' "$dir" 2>/dev/null || true)
    if [ -n "$hits" ]; then
      guard_refuse "production hostname in built artifacts: $(echo "$hits" | tr '\n' ' ')"
    fi
  done
  [ -f "$wt/client/web/dist/index.html" ] || guard_refuse "$wt/client/web/dist/index.html is missing"
  [ -f "$wt/server/worker/build/worker/shim.mjs" ] || guard_refuse "$wt/server/worker/build/worker/shim.mjs is missing"
}

# Every static check against one slot. Returns 1 if anything was refused.
guard_static() {
  local slot_dir slot root
  slot_dir=$(guard_realdir "$1" || true)
  if [ -z "$slot_dir" ]; then
    guard_refuse "slot dir $1 does not exist"
    return 1
  fi
  slot=${slot_dir##*/}
  root=${slot_dir%/*}
  GUARD_FAILS=0
  guard_env
  guard_root "$root"
  case "$(guard_realdir "$slot_dir/hb-$slot" || true)/" in
    "$root/"*) ;;
    *) guard_refuse "worktree $slot_dir/hb-$slot is missing or outside the slot root" ;;
  esac
  guard_py_run files "$slot_dir" "$slot"
  if [ -f "$slot_dir/state/wrangler.json" ]; then
    guard_py_run wrangler "$slot_dir" "$slot"
  else
    guard_refuse "$slot_dir/state/wrangler.json is missing"
  fi
  if [ -f "$slot_dir/state/vite.preview.config.mjs" ]; then
    guard_py_run preview "$slot_dir" "$slot"
  else
    guard_refuse "$slot_dir/state/vite.preview.config.mjs is missing"
  fi
  guard_artifacts "$slot_dir/hb-$slot"
  [ "$GUARD_FAILS" -eq 0 ]
}

if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  if [ $# -ne 1 ]; then
    echo "usage: $0 <slot-dir>" >&2
    exit 2
  fi
  guard_static "$1" || exit 1
  echo "guard: $1 passed every static check"
fi
