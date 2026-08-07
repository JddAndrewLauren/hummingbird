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
        raise SweepError("issueCreate %s -> %s" % (issue_id, _brief(payload)))
    result = (payload.get("data") or {}).get("issueCreate") or {}
    if not result.get("success"):
        raise SweepError("issueCreate %s not successful: %s" % (issue_id, _brief(payload)))
    return "created"


def _brief(payload):
    text = json.dumps(payload, default=str)
    return text if len(text) <= 500 else text[:500] + "..."


# --- healthchecks.io ---------------------------------------------------------


def ping_success(url):
    _ping(url, "GET", None)


def ping_failure(url, body):
    _ping(url.rstrip("/") + "/fail", "POST", body)


def _ping(url, method, body):
    try:
        http_json(
            url,
            method,
            {"Content-Type": "text/plain"},
            body.encode("utf-8") if body else None,
        )
        log("healthcheck ping %s" % url)
    except Exception as exc:  # a dead-man's switch must never kill the run
        log("WARN healthcheck ping failed: %s" % exc)


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
    """Run one sweep. Returns (ok, failure_lines)."""
    started = time.time()
    failures = []
    stats = {"created": 0, "existed": 0, "completed": 0, "failed": 0}
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
            title = task.get("title") or ""
            notes = (task.get("notes") or "").strip()
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
            except Exception as exc:
                # Leave the task incomplete; the next sweep retries it.
                stats["failed"] += 1
                failures.append("task %s in list %s: %s" % (task_id, list_id, exc))
                log("ERROR task %s in list %s: %s" % (task_id, list_id, exc))

    ok = not failures
    log(
        "sweep finish ok=%d created=%d existed=%d completed=%d failed=%d duration=%.1fs"
        % (
            int(ok),
            stats["created"],
            stats["existed"],
            stats["completed"],
            stats["failed"],
            time.time() - started,
        )
    )
    return ok, failures


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
    try:
        cfg = config_from_env(os.environ, args.dry_run)
        ok, failures = run_sweep(cfg, args.dry_run)
    except Exception as exc:
        ok = False
        failures = failures or []
        failures.append("sweep aborted: %s" % exc)
        log("ERROR sweep aborted: %s" % exc)

    # Fall back to the raw env var so a config error still trips the alarm.
    healthcheck = cfg.healthcheck_url if cfg else os.environ.get("HEALTHCHECK_URL", "")
    if not args.dry_run and healthcheck:
        if ok:
            ping_success(healthcheck)
        else:
            ping_failure(healthcheck, "\n".join(failures))

    lock.close()
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
