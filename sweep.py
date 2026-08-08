#!/usr/bin/env python3
"""One-way sweeper: capture sources -> Linear Triage issues.

One drain engine, isolated adapters (ADR-0002). Two capture adapters run per
sweep: Google Tasks (every incomplete item outside the denylist, fail-open)
and Gmail (only messages carrying the `hummingbird/capture` label,
fail-closed). Each adapter has its own frozen id namespace, its own
healthchecks.io check, and its own success/failure result; a failure in one
never stops the other's drain.

One-shot by design. supercronic fires it every 15 minutes in the container;
it is equally runnable locally (`./sweep.py --dry-run`) or by hand
(`fly ssh console -C /app/sweep`). Python 3 stdlib only, no dependencies.

Per item, in this order (the ordering is load-bearing):
  1. issueCreate in Linear with a client-supplied, deterministic id
  2. only then ack the item in its source (Tasks: PATCH to completed;
     Gmail: remove the capture label -- and nothing else)

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
  HEALTHCHECK_URL        (Google Tasks check; required live, unused in --dry-run)
  GMAIL_HEALTHCHECK_URL  (Gmail check; required live, unused in --dry-run)
  Each check is validated at its own adapter's boundary, so a missing one
  fails only that adapter.
  SWEEP_LOCK             (optional; defaults to /tmp/sweep.lock)

Exit codes: 0 = success, dry run, or lock contention. 1 = any failure.
"""

import argparse
import fcntl
import hashlib
import html
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
from email.header import decode_header, make_header
from pathlib import Path

# --- frozen constants --------------------------------------------------------

# NEVER CHANGE. Every Linear issue id an adapter has ever minted is
# sha256(namespace + source_key). Changing a namespace byte string re-mints
# every id in that source, which silently breaks idempotency and duplicates
# every open capture. One frozen namespace per source keeps the id spaces
# disjoint (ADR-0002); each is guarded by its own frozen test vector.
NAMESPACE = b"hummingbird-sweeper/google-tasks/v1"
GMAIL_NAMESPACE = b"hummingbird-sweeper/gmail/v1"

TEAM_ID = "84ab9e0b-f455-42d7-a48a-49e65da3b2e6"    # ION
STATE_ID = "35cec1f9-df46-4212-9bef-8905015ad539"   # Triage

LINEAR_URL = "https://api.linear.app/graphql"
GOOGLE_TOKEN_URL = "https://oauth2.googleapis.com/token"
TASKS_URL = "https://tasks.googleapis.com/tasks/v1"
GMAIL_URL = "https://gmail.googleapis.com/gmail/v1"

# The Gmail capture gesture. A message enters the drain only while it carries
# this label, and losing the label is the ack -- the only mutation the adapter
# ever makes. The label missing from the mailbox entirely fails the adapter
# closed rather than enumerating anything.
GMAIL_CAPTURE_LABEL = "hummingbird/capture"
GMAIL_THREAD_LINK = "https://mail.google.com/mail/u/0/#all/%s"

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
    "linear_api_key healthcheck_url denylist_path gmail_healthcheck_url",
    defaults=("",),
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


def deterministic_v4(source_key, namespace=NAMESPACE):
    """A UUID-v4-shaped id derived from a source item id.

    Linear's IssueCreateInput accepts a client-supplied id but validates it as
    version 4 specifically -- a genuine RFC-4122 v5 uuid is rejected with
    "id must be a UUID". So hash, then force the version and variant nibbles.

    The namespace defaults to Google Tasks, the original source; every other
    adapter passes its own frozen namespace so id spaces never collide.
    """
    digest = hashlib.sha256(namespace + source_key.encode("utf-8")).digest()
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


def _paginate(base_url, token, items_key="items"):
    items = []
    page_token = None
    while True:
        url = base_url
        if page_token:
            url += "&pageToken=" + urllib.parse.quote(page_token)
        payload = _google_get(url, token)
        items.extend(payload.get(items_key) or [])
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


# --- Gmail -------------------------------------------------------------------


def gmail_capture_label_id(token):
    """The id of the capture label, or a SweepError -- never a fallback.

    The label *is* the allowlist (ADR-0002: an inbox is a firehose, so email
    inverts to opt-in). If it is missing there is no gesture left to trust, so
    the adapter fails closed and visibly rather than enumerating anything.
    """
    payload = _google_get("%s/users/me/labels" % GMAIL_URL, token)
    for label in payload.get("labels") or []:
        if label.get("name") == GMAIL_CAPTURE_LABEL:
            return label["id"]
    raise SweepError(
        "capture label '%s' not found in the mailbox; failing closed"
        % GMAIL_CAPTURE_LABEL
    )


def gmail_list_message_ids(token, label_id):
    # includeSpamTrash is Gmail's default-false, and leaving it false would make
    # mailbox location a second admission rule behind the label's back: a
    # deliberately labelled message sitting in Spam or Trash would be dropped
    # while the adapter still reported success. The label is the whole gesture.
    url = "%s/users/me/messages?labelIds=%s&includeSpamTrash=true&maxResults=%d" % (
        GMAIL_URL,
        urllib.parse.quote(label_id, safe=""),
        PAGE_SIZE,
    )
    return [ref["id"] for ref in _paginate(url, token, items_key="messages") if ref.get("id")]


def gmail_get_message(token, message_id):
    url = (
        "%s/users/me/messages/%s?format=metadata"
        "&metadataHeaders=Subject&metadataHeaders=From&metadataHeaders=Date"
        % (GMAIL_URL, urllib.parse.quote(message_id, safe=""))
    )
    return _google_get(url, token)


def gmail_remove_label(token, message_id, label_id):
    """The whole ack: remove the capture label, touch nothing else.

    Deliberately no addLabelIds, no archive, no mark-read, no star, no delete.
    The message itself stays exactly where it was as the audit trail.
    """
    url = "%s/users/me/messages/%s/modify" % (
        GMAIL_URL,
        urllib.parse.quote(message_id, safe=""),
    )
    payload = http_json(
        url,
        "POST",
        {"Authorization": "Bearer " + token, "Content-Type": "application/json"},
        {"removeLabelIds": [label_id]},
    )
    if "_status" in payload:
        raise SweepError("POST %s -> %s" % (url, _brief(payload)))


def _decode_rfc2047(value):
    """A human-readable header value: RFC 2047 encoded-words decoded.

    An undecodable header falls back to its raw form -- ugly beats lost."""
    if not value:
        return ""
    try:
        return str(make_header(decode_header(value)))
    except Exception:
        return value


def _gmail_headers(message):
    headers = {}
    for header in (message.get("payload") or {}).get("headers") or []:
        headers[(header.get("name") or "").lower()] = header.get("value") or ""
    return headers


def _gmail_timestamp(message, headers):
    try:
        millis = int(message["internalDate"])
    except (KeyError, TypeError, ValueError):
        return headers.get("date") or "(unknown time)"
    stamp = datetime.fromtimestamp(millis / 1000.0, timezone.utc)
    return stamp.strftime("%Y-%m-%dT%H:%M:%SZ")


def gmail_derive_capture(message):
    """(title, description) for one labelled message. Never None.

    A message earned enumeration by carrying the capture label -- a deliberate
    human gesture -- so unlike a blank Tasks row there is always something to
    capture and nothing is ever silently dropped. The fallback chain for the
    title: decoded non-blank Subject verbatim, else the first non-blank line
    of the snippet, else "(no subject)".

    The description shape is stable: decoded sender, message timestamp, a
    Gmail thread deep link, then the snippet when present. The link is the
    road back to the full message; the message itself is the audit trail.
    """
    headers = _gmail_headers(message)
    subject = _decode_rfc2047(headers.get("subject"))
    snippet = html.unescape(message.get("snippet") or "").strip()

    if subject.strip():
        title = subject
    elif snippet:
        title = next(line.strip() for line in snippet.splitlines() if line.strip())
    else:
        title = "(no subject)"

    sender = _decode_rfc2047(headers.get("from")) or "(unknown sender)"
    thread_id = message.get("threadId") or message.get("id") or ""
    description = "From: %s\nDate: %s\nThread: %s" % (
        sender,
        _gmail_timestamp(message, headers),
        GMAIL_THREAD_LINK % thread_id,
    )
    if snippet:
        description += "\n\n" + snippet
    return title, description


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
    exception that has to earn itself. So *every* error has to earn it
    separately: one recognized `title` violation does not cover for a sibling
    error naming nothing, or the unexplained half would be quarantined too.

    A 5xx is never terminal whatever its body says, per the classification in
    docs/sweeper.md: the server failing to answer says nothing about the
    capture, and the next sweep might well succeed.
    """
    if payload.get("_status", 0) >= 500:
        return False

    errors = payload.get("errors") or []
    if not errors:
        return False

    for error in errors:
        code = (error.get("extensions") or {}).get("code")
        if code not in ("INVALID_INPUT", "INPUT_ERROR"):
            return False
        properties = set()
        _collect_properties(error, properties)
        if not properties or not properties <= CONTENT_FIELDS:
            return False
    return True


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


def ping_success(url, body=None, name=""):
    # A body is sent when the sweep set something aside. healthchecks.io keeps
    # it on the check page, so quarantined and skipped items stay visible
    # without the run having to fail to show them.
    _ping(url, "POST" if body else "GET", body, "success", name)


def ping_failure(url, body, name=""):
    _ping(url.rstrip("/") + "/fail", "POST", body, "fail", name)


def _ping(url, method, body, label, name=""):
    # Never log the url. Each ping url is a bearer secret -- anyone holding it
    # can forge a success ping and silence the dead-man's switch -- and these
    # lines go to stdout, which means the Fly log stream.
    suffix = " adapter=%s" % name if name else ""
    try:
        http_json(
            url,
            method,
            {"Content-Type": "text/plain"},
            body.encode("utf-8") if body else None,
        )
        log("healthcheck %s ping sent%s" % (label, suffix))
    except Exception as exc:  # a dead-man's switch must never kill the run
        log("WARN healthcheck %s ping failed%s: %s" % (label, suffix, _redact(exc)))


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


class GoogleTasksAdapter:
    """The original source: every incomplete item is presumptively capture,
    minus the fail-open denylist. Behavior is unchanged from the pre-seam
    sweeper; only its engine moved into run_adapter."""

    name = "google-tasks"
    namespace = NAMESPACE
    healthcheck_env = "HEALTHCHECK_URL"

    def __init__(self, cfg):
        self.cfg = cfg
        self.token = None

    @property
    def healthcheck_url(self):
        return self.cfg.healthcheck_url

    def enumerate(self, fail):
        denylist = load_denylist(self.cfg.denylist_path)
        self.token = google_access_token(self.cfg)
        for tasklist in list_tasklists(self.token):
            list_id = tasklist.get("id")
            # This line is what seeds denylist.json after the first dry run.
            log(
                "list id=%s title='%s'%s"
                % (
                    list_id,
                    tasklist.get("title", ""),
                    " SKIP denylisted" if list_id in denylist else "",
                )
            )
            if list_id in denylist:
                continue
            try:
                tasks = list_tasks(self.token, list_id)
            except Exception as exc:
                fail("list %s" % list_id, exc)
                continue
            for task in tasks:
                yield (list_id, task)

    def source_key(self, item):
        return item[1].get("id")

    def derive_capture(self, item):
        return derive_capture(item[1].get("title"), item[1].get("notes"))

    def describe(self, item):
        return "task %s in list %s" % (item[1].get("id"), item[0])

    def ack(self, item):
        complete_task(self.token, item[0], item[1].get("id"))


class GmailAdapter:
    """Opt-in capture from the mailbox: the `hummingbird/capture` label is the
    gesture, unlabelling is the ack, the message is the unit (ADR-0002).

    Everything fails closed. Only labelled messages are ever enumerated; a
    mailbox without the label fails the adapter visibly rather than guessing;
    and the ack removes exactly that one label from that one message -- never
    archive, mark-read, star, delete, or any other mutation."""

    name = "gmail"
    namespace = GMAIL_NAMESPACE
    healthcheck_env = "GMAIL_HEALTHCHECK_URL"

    def __init__(self, cfg):
        self.cfg = cfg
        self.token = None
        self.label_id = None

    @property
    def healthcheck_url(self):
        return self.cfg.gmail_healthcheck_url

    def enumerate(self, fail):
        self.token = google_access_token(self.cfg)
        self.label_id = gmail_capture_label_id(self.token)
        message_ids = gmail_list_message_ids(self.token, self.label_id)
        log("gmail label '%s' carries %d message(s)" % (GMAIL_CAPTURE_LABEL, len(message_ids)))
        for message_id in message_ids:
            try:
                message = gmail_get_message(self.token, message_id)
            except Exception as exc:
                fail("message %s" % message_id, exc)
                continue
            # Listing and retrieval are two calls, and the label can come off
            # between them. Only a *currently* labelled message may enter the
            # drain, so the retrieved labels are the authority, not the list.
            if self.label_id not in (message.get("labelIds") or []):
                log(
                    "message %s lost the capture label since listing; skipping"
                    % message_id
                )
                continue
            yield message

    def source_key(self, item):
        return item.get("id")

    def derive_capture(self, item):
        return gmail_derive_capture(item)

    def describe(self, item):
        return "message %s" % item.get("id")

    def ack(self, item):
        gmail_remove_label(self.token, item.get("id"), self.label_id)


AdapterResult = namedtuple("AdapterResult", "name healthcheck_url ok failures notes")


def run_adapter(adapter, dry_run):
    """Drain one adapter. Returns an AdapterResult; never raises for anything
    the adapter itself did.

    `notes` describes what the drain set aside -- quarantined and skipped
    items. They do not fail the run, so they ride along on the success ping
    instead: green means capture is working, and the check page still shows
    the junk accumulating. Quarantine nobody can see is just the bug #24
    fixed, pointed the other way.
    """
    started = time.time()
    failures = []
    quarantined = []
    stats = {"created": 0, "existed": 0, "completed": 0, "failed": 0,
             "skipped": 0, "quarantined": 0}
    log("sweep start adapter=%s dry_run=%d" % (adapter.name, int(dry_run)))

    def fail(ref, exc):
        stats["failed"] += 1
        failures.append("%s: %s" % (ref, exc))
        log("ERROR %s: %s" % (ref, exc))

    try:
        if not dry_run and not adapter.healthcheck_url:
            # This adapter's own config, checked at its own boundary so a
            # missing check for one source never stops another's drain. Not
            # draining is the safe half: capture with the dead-man's switch
            # unarmed is invisible, whereas a check that is never pinged goes
            # red on grace expiry, which is the switch doing its job.
            raise SweepError(
                "missing environment variable %s" % adapter.healthcheck_env
            )
        for item in adapter.enumerate(fail):
            # Preparation lives inside the per-item try with the create and the
            # ack: a malformed item must fail that item and let the drain go on,
            # never abort the adapter mid-list.
            ref = "unidentified %s item" % adapter.name
            title = ""
            try:
                ref = adapter.describe(item)
                capture = adapter.derive_capture(item)
                if capture is None:
                    # Nothing to capture, so nothing to lose. Left unacked on
                    # purpose: the row stays visible in its source for a human
                    # to delete, rather than being disposed of silently. It will
                    # re-warn every sweep until then, and never touches the alarm.
                    stats["skipped"] += 1
                    log("WARN skipping empty capture %s (no title, no notes)" % ref)
                    continue
                title, description = capture
                issue_id = deterministic_v4(adapter.source_key(item), adapter.namespace)
                if dry_run:
                    log(
                        "DRY-RUN would create %s title='%s' (%s)"
                        % (issue_id, title, ref)
                    )
                    log("DRY-RUN would ack %s" % ref)
                    continue
                outcome = linear_create_issue(
                    adapter.cfg.linear_api_key, issue_id, title, description
                )
                stats[outcome] += 1
                log("%s issue %s title='%s'" % (outcome, issue_id, title))
                adapter.ack(item)
                stats["completed"] += 1
                log("acked %s" % ref)
            except TerminalRejection as exc:
                # Retrying cannot help, so retrying is all cost: it would pin
                # the dead-man's switch red until a human intervened, and a
                # permanently-red alarm is indistinguishable from a working
                # one. Leave the item visible in its source; keep the run honest.
                stats["quarantined"] += 1
                quarantined.append("%s: %s" % (ref, exc))
                log("QUARANTINE %s title='%s': %s" % (ref, title, exc))
            except Exception as exc:
                # Leave the item unacked; the next sweep retries it.
                fail(ref, exc)
    except Exception as exc:
        # The adapter's own plumbing failed -- token exchange, a missing
        # capture label, enumeration itself. Fail this adapter loudly and let
        # the other adapters drain on regardless.
        fail("adapter %s" % adapter.name, exc)

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
        "sweep finish adapter=%s ok=%d created=%d existed=%d completed=%d "
        "failed=%d skipped=%d quarantined=%d duration=%.1fs"
        % (
            adapter.name,
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
    return AdapterResult(adapter.name, adapter.healthcheck_url, ok, failures, set_aside)


def report_result(result):
    """Ping one adapter's own check. Each check is that adapter's dead-man's
    switch and nobody else's (ADR-0002), so this takes a single result and
    never looks at the others."""
    if not result.healthcheck_url:
        return
    if result.ok:
        ping_success(
            result.healthcheck_url,
            "\n".join(result.notes) if result.notes else None,
            name=result.name,
        )
    else:
        ping_failure(
            result.healthcheck_url, "\n".join(result.failures), name=result.name
        )


def run_sweep(cfg, dry_run, on_result=None):
    """Run every capture adapter in one sweep. Returns [AdapterResult, ...].

    Isolation is the point (ADR-0002): each adapter gets its own result, its
    own healthcheck ping, and its own failure list, and a failure in one never
    prevents another from draining and reporting.

    `on_result` is called with each result the moment that adapter finishes,
    before the next one starts. Reporting is isolated in *time* as well as in
    routing: a later adapter grinding through its timeouts must not hold an
    earlier adapter's ping past its grace period and turn a healthy drain red.
    """
    results = []
    for adapter in (GoogleTasksAdapter(cfg), GmailAdapter(cfg)):
        try:
            result = run_adapter(adapter, dry_run)
        except Exception as exc:  # belt for the braces inside run_adapter
            log("ERROR adapter %s aborted: %s" % (adapter.name, exc))
            result = AdapterResult(
                adapter.name,
                adapter.healthcheck_url,
                False,
                ["adapter %s aborted: %s" % (adapter.name, exc)],
                [],
            )
        results.append(result)
        if on_result is not None:
            on_result(result)
    return results


def config_from_env(env):
    def required(name):
        value = env.get(name)
        if not value:
            raise SweepError("missing environment variable %s" % name)
        return value

    # Deliberately not required here. A check url belongs to one adapter, so
    # demanding it before the sweep starts would let a missing Gmail check stop
    # the Google Tasks drain and vice versa -- exactly the coupling ADR-0002
    # forbids. Each adapter validates its own check at its own boundary in
    # run_adapter instead.
    healthcheck = env.get("HEALTHCHECK_URL") or ""
    gmail_healthcheck = env.get("GMAIL_HEALTHCHECK_URL") or ""

    return Config(
        google_client_id=required("GOOGLE_CLIENT_ID"),
        google_client_secret=required("GOOGLE_CLIENT_SECRET"),
        google_refresh_token=required("GOOGLE_REFRESH_TOKEN"),
        linear_api_key=required("LINEAR_API_KEY"),
        healthcheck_url=healthcheck,
        denylist_path=env.get("SWEEP_DENYLIST") or DEFAULT_DENYLIST,
        gmail_healthcheck_url=gmail_healthcheck,
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

    # One ping per adapter, each to its own check, fired the moment that
    # adapter finishes: a shared check held red by one broken drain would hide
    # the health of the others, and a ping deferred until every adapter has
    # finished would let a slow one hold a healthy adapter's check past its
    # grace period (ADR-0002).
    on_result = None if args.dry_run else report_result

    try:
        cfg = config_from_env(os.environ)
        results = run_sweep(cfg, args.dry_run, on_result)
    except Exception as exc:
        # A failure before any adapter ran -- config, usually. Nothing swept,
        # so every adapter's alarm must trip. Fall back to the raw env vars so
        # a config error still reaches whichever checks exist.
        log("ERROR sweep aborted: %s" % exc)
        aborted = "sweep aborted: %s" % exc
        results = [
            AdapterResult("google-tasks", os.environ.get("HEALTHCHECK_URL", ""),
                          False, [aborted], []),
            AdapterResult("gmail", os.environ.get("GMAIL_HEALTHCHECK_URL", ""),
                          False, [aborted], []),
        ]
        # Nothing reported yet on this path -- the abort happened before any
        # adapter finished -- so report both here.
        if on_result is not None:
            for result in results:
                on_result(result)

    lock.close()
    return 0 if all(result.ok for result in results) else 1


if __name__ == "__main__":
    sys.exit(main())
