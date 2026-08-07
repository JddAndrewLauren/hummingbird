#!/usr/bin/env python3
"""One-way sweeper: incomplete Google Tasks items -> Linear Triage issues.

One-shot by design. supercronic fires it every 15 minutes in the container;
it is equally runnable locally (`./sweep.py --dry-run`) or by hand
(`fly ssh console -C /app/sweep`). Python 3 stdlib only, no dependencies.

Per item, in this order (the ordering is load-bearing):
  1. issueCreate in Linear with a client-supplied, deterministic id
  2. only then PATCH the Tasks item to completed

A crash between the two can only produce a visible duplicate attempt on the
next sweep -- which the deterministic id turns into an "already exists"
success -- never a silently lost capture.

Failures split in two (#24). Transient ones -- 5xx, timeouts, a dead token --
leave the task incomplete, fail the run, and trip the alarm; the next sweep
retries. Terminal ones, where Linear refuses the capture's own content, are
quarantined instead: logged loudly, left visible in Tasks, counted on the
success ping, but never failing the run. Retrying those forever is what pinned
the dead-man's switch red, and an alarm that is always ringing is no alarm.

Environment:
  GOOGLE_CLIENT_ID, GOOGLE_CLIENT_SECRET, GOOGLE_REFRESH_TOKEN, LINEAR_API_KEY
  HEALTHCHECK_URL   (required for live runs; unused in --dry-run)
  SWEEP_LOCK        (optional; defaults to /tmp/sweep.lock)

Exit codes: 0 = success, dry run, or lock contention. 1 = any failure.
"""

import argparse
import fcntl
import hashlib
import json
import os
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from collections import namedtuple
from datetime import datetime, timezone
from pathlib import Path

# --- frozen constants --------------------------------------------------------

# NEVER CHANGE. Every Linear issue id the sweeper has ever minted is
# sha256(NAMESPACE + tasks_item_id). Changing this byte string re-mints every
# id, which silently breaks idempotency and duplicates every open capture.
NAMESPACE = b"hummingbird-sweeper/google-tasks/v1"

TEAM_ID = "84ab9e0b-f455-42d7-a48a-49e65da3b2e6"    # ION
STATE_ID = "35cec1f9-df46-4212-9bef-8905015ad539"   # Triage

LINEAR_URL = "https://api.linear.app/graphql"
GOOGLE_TOKEN_URL = "https://oauth2.googleapis.com/token"
TASKS_URL = "https://tasks.googleapis.com/tasks/v1"

HTTP_TIMEOUT = 30
PAGE_SIZE = 100

# Linear rejects some inputs permanently -- a blank capture is the case that
# found this (#24). Those are quarantined rather than retried, so one junk row
# cannot hold the dead-man's switch red forever. But quarantine is only safe
# while it stays rare: past this many in a single sweep, the classification is
# being trusted further than it has earned, so fail the run instead.
QUARANTINE_LIMIT = 10

# The only fields a *capture* can be rejected on. A validation error naming
# anything else (teamId, stateId) is a broken sweeper, not a bad row -- that
# stays a hard failure and rings the alarm.
CONTENT_FIELDS = frozenset(("title", "description"))

ISSUE_CREATE = (
    "mutation IssueCreate($input: IssueCreateInput!) {"
    " issueCreate(input: $input) { success issue { id identifier } } }"
)

# Linear's duplicate-id rejection, verified live against the workspace.
ALREADY_EXISTS = re.compile(r"^Entity Issue with id .* already exists\.$")

DEFAULT_DENYLIST = Path(__file__).resolve().parent / "denylist.json"

Config = namedtuple(
    "Config",
    "google_client_id google_client_secret google_refresh_token "
    "linear_api_key healthcheck_url denylist_path",
)


class SweepError(Exception):
    """Anything that should fail one item, or the run, without a traceback."""


class TerminalRejection(SweepError):
    """Linear refuses this input. Retrying is guaranteed to fail identically.

    Distinct from every other error precisely because the standard remedy --
    leave the task incomplete, fail the run, let the next sweep retry -- is
    wrong here. Retrying forever is what pinned the alarm red in #24.
    """


# --- plumbing ----------------------------------------------------------------


def log(message):
    stamp = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    print("%s %s" % (stamp, message), flush=True)


def http_json(url, method="GET", headers=None, body=None):
    """The single HTTP choke point. Tests monkeypatch exactly this.

    `body` may be a dict (JSON-encoded) or bytes (sent verbatim). Returns the
    decoded response body as a dict; on a non-2xx response the decoded error
    body is returned with an extra `_status` key rather than raised, because
    Linear reports "already exists" as an HTTP 400 that means success.
    """
    data = None
    hdrs = dict(headers or {})
    if body is not None:
        if isinstance(body, (bytes, bytearray)):
            data = bytes(body)
        else:
            data = json.dumps(body).encode("utf-8")
            hdrs.setdefault("Content-Type", "application/json")
    request = urllib.request.Request(url, data=data, headers=hdrs, method=method)
    try:
        with urllib.request.urlopen(request, timeout=HTTP_TIMEOUT) as response:
            return _decode(response.read())
    except urllib.error.HTTPError as exc:
        payload = _decode(exc.read())
        payload["_status"] = exc.code
        return payload


def _decode(raw):
    if not raw:
        return {}
    try:
        parsed = json.loads(raw)
    except ValueError:
        return {"_raw": raw.decode("utf-8", "replace")}
    if not isinstance(parsed, dict):
        return {"_raw": parsed}
    return parsed


def deterministic_v4(task_id):
    """A UUID-v4-shaped id derived from a Google Tasks item id.

    Linear's IssueCreateInput accepts a client-supplied id but validates it as
    version 4 specifically -- a genuine RFC-4122 v5 uuid is rejected with
    "id must be a UUID". So hash, then force the version and variant nibbles.
    """
    digest = hashlib.sha256(NAMESPACE + task_id.encode("utf-8")).digest()
    b = bytearray(digest[:16])
    b[6] = (b[6] & 0x0F) | 0x40
    b[8] = (b[8] & 0x3F) | 0x80
    hexed = b.hex()
    return "-".join(
        (hexed[0:8], hexed[8:12], hexed[12:16], hexed[16:20], hexed[20:32])
    )


def derive_capture(title, notes):
    """(title, description) for one capture, or None if it carries nothing.

    The rule from #14 is unchanged for every real capture: title verbatim, no
    cleanup, no truncation, no prefix. #24 added the two edges it never
    contemplated, because Linear rejects an empty title outright:

    - title empty, notes present -- the plausible case is a dictation that
      landed entirely in the notes, so its first non-blank line becomes the
      handle. The full notes still become the description; nothing is dropped.
    - both empty -- a row made by pressing Enter in the Tasks app. There is no
      information to lose, so it is skipped rather than disposed of.
    """
    title = title or ""
    notes = (notes or "").strip()
    if title.strip():
        return title, notes  # verbatim: only the emptiness test strips
    if notes:
        first = next(line.strip() for line in notes.splitlines() if line.strip())
        return first, notes
    return None


# --- Google Tasks ------------------------------------------------------------


def google_access_token(cfg):
    body = urllib.parse.urlencode(
        {
            "client_id": cfg.google_client_id,
            "client_secret": cfg.google_client_secret,
            "refresh_token": cfg.google_refresh_token,
            "grant_type": "refresh_token",
        }
    ).encode("utf-8")
    payload = http_json(
        GOOGLE_TOKEN_URL,
        "POST",
        {"Content-Type": "application/x-www-form-urlencoded"},
        body,
    )
    token = payload.get("access_token")
    if not token:
        raise SweepError("token exchange failed: %s" % _brief(payload))
    return token


def _google_get(url, token):
    payload = http_json(url, "GET", {"Authorization": "Bearer " + token})
    if "_status" in payload:
        raise SweepError("GET %s -> %s" % (url, _brief(payload)))
    return payload


def _paginate(base_url, token):
    items = []
    page_token = None
    while True:
        url = base_url
        if page_token:
            url += "&pageToken=" + urllib.parse.quote(page_token)
        payload = _google_get(url, token)
        items.extend(payload.get("items") or [])
        page_token = payload.get("nextPageToken")
        if not page_token:
            return items


def list_tasklists(token):
    return _paginate("%s/users/@me/lists?maxResults=%d" % (TASKS_URL, PAGE_SIZE), token)


def list_tasks(token, list_id):
    url = "%s/lists/%s/tasks?showCompleted=false&showHidden=false&maxResults=%d" % (
        TASKS_URL,
        urllib.parse.quote(list_id, safe=""),
        PAGE_SIZE,
    )
    return _paginate(url, token)


def complete_task(token, list_id, task_id):
    url = "%s/lists/%s/tasks/%s" % (
        TASKS_URL,
        urllib.parse.quote(list_id, safe=""),
        urllib.parse.quote(task_id, safe=""),
    )
    payload = http_json(
        url,
        "PATCH",
        {"Authorization": "Bearer " + token, "Content-Type": "application/json"},
        {"status": "completed"},
    )
    if "_status" in payload:
        raise SweepError("PATCH %s -> %s" % (url, _brief(payload)))


# --- Linear ------------------------------------------------------------------


def linear_create_issue(api_key, issue_id, title, description):
    """Create the issue. Returns "created" or "existed"; raises otherwise."""
    fields = {"id": issue_id, "teamId": TEAM_ID, "stateId": STATE_ID, "title": title}
    if description:
        fields["description"] = description
    payload = http_json(
        LINEAR_URL,
        "POST",
        # Raw key, not Bearer -- Linear personal API keys are sent unprefixed.
        {"Authorization": api_key, "Content-Type": "application/json"},
        {"query": ISSUE_CREATE, "variables": {"input": fields}},
    )

    for error in payload.get("errors") or []:
        extensions = error.get("extensions") or {}
        message = extensions.get("userPresentableMessage") or ""
        if extensions.get("code") == "INPUT_ERROR" and ALREADY_EXISTS.match(message):
            return "existed"

    if payload.get("errors") or "_status" in payload:
        if _is_terminal(payload):
            raise TerminalRejection(
                "issueCreate %s rejected on content -> %s" % (issue_id, _brief(payload))
            )
        raise SweepError("issueCreate %s -> %s" % (issue_id, _brief(payload)))
    result = (payload.get("data") or {}).get("issueCreate") or {}
    if not result.get("success"):
        raise SweepError("issueCreate %s not successful: %s" % (issue_id, _brief(payload)))
    return "created"


def _is_terminal(payload):
    """Would this rejection recur identically on every future sweep?

    Two conditions, both required. The error must be a validation error, and
    every field it names must be one this *capture* supplied. Linear reports
    the offending field ("property": "title" in the live #24 error), which is
    what separates a junk row from a broken sweeper: a bad capture can only be
    rejected on its own content, while a wrong teamId or stateId is rejected on
    that field and must keep ringing the alarm.

    Anything unrecognized -- an unparseable shape, an error naming no field at
    all -- returns False. Fail loud is the safe default; quarantine is the
    exception that has to earn itself.
    """
    codes = set()
    for error in payload.get("errors") or []:
        codes.add((error.get("extensions") or {}).get("code"))
    if not codes or not codes <= {"INVALID_INPUT", "INPUT_ERROR"}:
        return False

    properties = set()
    _collect_properties(payload.get("errors"), properties)
    return bool(properties) and properties <= CONTENT_FIELDS


def _collect_properties(node, found):
    """Every field named by a constraint violation, at any nesting depth.

    Linear nests them under an `input` wrapper whose own node carries no
    `constraints`, so requiring `constraints` picks the real leaves and skips
    the wrapper.
    """
    if isinstance(node, dict):
        name = node.get("property")
        if isinstance(name, str) and node.get("constraints"):
            found.add(name)
        for value in node.values():
            _collect_properties(value, found)
    elif isinstance(node, list):
        for value in node:
            _collect_properties(value, found)


def _brief(payload):
    text = json.dumps(payload, default=str)
    return text if len(text) <= 500 else text[:500] + "..."


def _redact(value):
    """Strip the path off any url in a string, keeping the host.

    urllib's exception text does not normally carry the url, but "normally" is
    not a property to bet a secret on: the ping url's path *is* the credential.
    """
    return re.sub(r"(https?://[^/\s]+)/\S*", r"\1/<redacted>", str(value))


# --- healthchecks.io ---------------------------------------------------------


def ping_success(url, body=None):
    # A body is sent when the sweep set something aside. healthchecks.io keeps
    # it on the check page, so quarantined and skipped items stay visible
    # without the run having to fail to show them.
    _ping(url, "POST" if body else "GET", body, "success")


def ping_failure(url, body):
    _ping(url.rstrip("/") + "/fail", "POST", body, "fail")


def _ping(url, method, body, label):
    # Never log the url. HEALTHCHECK_URL is a bearer secret -- anyone holding it
    # can forge a success ping and silence the dead-man's switch -- and these
    # lines go to stdout, which means the Fly log stream.
    try:
        http_json(
            url,
            method,
            {"Content-Type": "text/plain"},
            body.encode("utf-8") if body else None,
        )
        log("healthcheck %s ping sent" % label)
    except Exception as exc:  # a dead-man's switch must never kill the run
        log("WARN healthcheck %s ping failed: %s" % (label, _redact(exc)))


# --- the sweep ---------------------------------------------------------------


def load_denylist(path):
    path = Path(path)
    if not path.exists():
        log("WARN denylist %s missing; sweeping every list" % path)
        return {}
    with path.open(encoding="utf-8") as handle:
        data = json.load(handle)
    if not isinstance(data, dict):
        raise SweepError("denylist %s must be a JSON object keyed by list id" % path)
    return data


def run_sweep(cfg, dry_run):
    """Run one sweep. Returns (ok, failure_lines, notes_lines).

    `notes_lines` describes what the sweep set aside -- quarantined and
    skipped items. They do not fail the run, so they ride along on the success
    ping instead: green means capture is working, and the check page still
    shows the junk accumulating. Quarantine nobody can see is just the bug
    this fixed, pointed the other way.
    """
    started = time.time()
    failures = []
    quarantined = []
    stats = {"created": 0, "existed": 0, "completed": 0, "failed": 0,
             "skipped": 0, "quarantined": 0}
    log("sweep start dry_run=%d" % int(dry_run))

    denylist = load_denylist(cfg.denylist_path)
    token = google_access_token(cfg)

    for tasklist in list_tasklists(token):
        list_id = tasklist.get("id")
        # This line is what seeds denylist.json after the first dry run.
        log(
            "list id=%s title='%s'%s"
            % (list_id, tasklist.get("title", ""), " SKIP denylisted" if list_id in denylist else "")
        )
        if list_id in denylist:
            continue
        try:
            tasks = list_tasks(token, list_id)
        except Exception as exc:
            stats["failed"] += 1
            failures.append("list %s: %s" % (list_id, exc))
            log("ERROR list %s: %s" % (list_id, exc))
            continue

        for task in tasks:
            task_id = task.get("id")
            capture = derive_capture(task.get("title"), task.get("notes"))
            if capture is None:
                # Nothing to capture, so nothing to lose. Left incomplete on
                # purpose: the row stays visible in Tasks for a human to
                # delete, rather than being disposed of silently. It will
                # re-warn every sweep until then, and never touches the alarm.
                stats["skipped"] += 1
                log(
                    "WARN skipping empty capture task=%s list=%s (no title, no notes)"
                    % (task_id, list_id)
                )
                continue
            title, notes = capture
            issue_id = deterministic_v4(task_id)
            try:
                if dry_run:
                    log(
                        "DRY-RUN would create %s title='%s' (list=%s task=%s)"
                        % (issue_id, title, list_id, task_id)
                    )
                    log("DRY-RUN would complete task %s in list %s" % (task_id, list_id))
                    continue
                outcome = linear_create_issue(cfg.linear_api_key, issue_id, title, notes)
                stats[outcome] += 1
                log("%s issue %s title='%s'" % (outcome, issue_id, title))
                complete_task(token, list_id, task_id)
                stats["completed"] += 1
                log("completed task %s in list %s" % (task_id, list_id))
            except TerminalRejection as exc:
                # Retrying cannot help, so retrying is all cost: it would pin
                # the dead-man's switch red until a human intervened, and a
                # permanently-red alarm is indistinguishable from a working
                # one. Leave the row visible in Tasks and keep the run honest.
                stats["quarantined"] += 1
                quarantined.append("task %s in list %s: %s" % (task_id, list_id, exc))
                log(
                    "QUARANTINE task %s in list %s title='%s': %s"
                    % (task_id, list_id, title, exc)
                )
            except Exception as exc:
                # Leave the task incomplete; the next sweep retries it.
                stats["failed"] += 1
                failures.append("task %s in list %s: %s" % (task_id, list_id, exc))
                log("ERROR task %s in list %s: %s" % (task_id, list_id, exc))

    if stats["quarantined"] > QUARANTINE_LIMIT:
        # Junk rows arrive in ones and twos. This many at once is not bad
        # input, it is a sweeper whose classification has stopped being
        # trustworthy -- so stop trusting it and ring the alarm.
        failures.append(
            "%d items quarantined in one sweep (limit %d); treating as systematic"
            % (stats["quarantined"], QUARANTINE_LIMIT)
        )

    ok = not failures
    log(
        "sweep finish ok=%d created=%d existed=%d completed=%d failed=%d "
        "skipped=%d quarantined=%d duration=%.1fs"
        % (
            int(ok),
            stats["created"],
            stats["existed"],
            stats["completed"],
            stats["failed"],
            stats["skipped"],
            stats["quarantined"],
            time.time() - started,
        )
    )

    set_aside = []
    if stats["quarantined"]:
        set_aside.append(
            "%d quarantined (Linear refused the content):" % stats["quarantined"]
        )
        set_aside.extend(quarantined)
    if stats["skipped"]:
        set_aside.append(
            "%d empty captures skipped (no title, no notes)" % stats["skipped"]
        )
    return ok, failures, set_aside


def config_from_env(env, dry_run):
    def required(name):
        value = env.get(name)
        if not value:
            raise SweepError("missing environment variable %s" % name)
        return value

    healthcheck = env.get("HEALTHCHECK_URL") or ""
    if not dry_run and not healthcheck:
        raise SweepError("missing environment variable HEALTHCHECK_URL")

    return Config(
        google_client_id=required("GOOGLE_CLIENT_ID"),
        google_client_secret=required("GOOGLE_CLIENT_SECRET"),
        google_refresh_token=required("GOOGLE_REFRESH_TOKEN"),
        linear_api_key=required("LINEAR_API_KEY"),
        healthcheck_url=healthcheck,
        denylist_path=env.get("SWEEP_DENYLIST") or DEFAULT_DENYLIST,
    )


def acquire_lock(path):
    """Exclusive lock held in-process, so it covers supercronic, `fly ssh
    console -C /app/sweep`, and local runs alike. Returns the open file (whose
    reference must outlive the sweep) or None if another run holds it."""
    handle = open(path, "w")
    try:
        fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except OSError:
        handle.close()
        return None
    return handle


def main(argv=None):
    parser = argparse.ArgumentParser(description="Sweep Google Tasks into Linear Triage.")
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="read everything, mutate nothing, and log what would happen",
    )
    args = parser.parse_args(argv)

    lock_path = os.environ.get("SWEEP_LOCK") or "/tmp/sweep.lock"
    lock = acquire_lock(lock_path)
    if lock is None:
        log("another sweep holds %s; skipping this run" % lock_path)
        return 0

    cfg = None
    failures = []
    notes = []
    try:
        cfg = config_from_env(os.environ, args.dry_run)
        ok, failures, notes = run_sweep(cfg, args.dry_run)
    except Exception as exc:
        ok = False
        failures = failures or []
        failures.append("sweep aborted: %s" % exc)
        log("ERROR sweep aborted: %s" % exc)

    # Fall back to the raw env var so a config error still trips the alarm.
    healthcheck = cfg.healthcheck_url if cfg else os.environ.get("HEALTHCHECK_URL", "")
    if not args.dry_run and healthcheck:
        if ok:
            ping_success(healthcheck, "\n".join(notes) if notes else None)
        else:
            ping_failure(healthcheck, "\n".join(failures))

    lock.close()
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
