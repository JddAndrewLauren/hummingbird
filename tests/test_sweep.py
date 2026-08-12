"""Cred-free tests for the sweeper. `python3 -m unittest discover -s tests`.

Everything network-shaped goes through sweep.http_json, so these tests
monkeypatch exactly that one function and assert on the calls it receives.
"""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import sweep  # noqa: E402


class DeterministicV4Test(unittest.TestCase):
    # Frozen vector. If this fails, NAMESPACE or the derivation changed, and
    # every issue id the sweeper has ever minted just moved -- which means
    # every still-open capture would be recreated as a duplicate.
    VECTOR = ("cHJvamVjdC10YXNrLTE", "93a91b16-e37e-4c2d-9cc0-75d1d259d7a5")

    def test_frozen_vector(self):
        task_id, expected = self.VECTOR
        self.assertEqual(sweep.deterministic_v4(task_id), expected)

    def test_v4_shape(self):
        for task_id in ("a", "some-google-tasks-id", "éè", "x" * 200):
            value = sweep.deterministic_v4(task_id)
            self.assertRegex(
                value,
                r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$",
            )
            flat = value.replace("-", "")
            self.assertEqual(flat[12], "4", "version nibble")
            self.assertIn(flat[16], "89ab", "variant nibble")

    def test_stable_and_distinct(self):
        self.assertEqual(sweep.deterministic_v4("abc"), sweep.deterministic_v4("abc"))
        self.assertNotEqual(sweep.deterministic_v4("abc"), sweep.deterministic_v4("abd"))


class HttpStatusStampTest(unittest.TestCase):
    """`with_status` is opt-in, and that is load-bearing.

    Every Google helper here reads `"_status" in payload` as "this call
    failed". Stamping the status on every success would make each of them
    raise on a perfectly good response -- the sweeper would create items and
    then fail to ack a single one. Only the authority's create, where 201 and
    200 are both success and mean different things, asks for it.
    """

    class FakeResponse:
        status = 201

        def read(self):
            return b'{"id": "abc"}'

        def __enter__(self):
            return self

        def __exit__(self, *exc):
            return False

    def urlopen(self, request, timeout=None):
        return self.FakeResponse()

    def setUp(self):
        self.real_urlopen = sweep.urllib.request.urlopen
        sweep.urllib.request.urlopen = self.urlopen

    def tearDown(self):
        sweep.urllib.request.urlopen = self.real_urlopen

    def test_success_carries_no_status_by_default(self):
        self.assertEqual(sweep.http_json("https://example.test/x"), {"id": "abc"})

    def test_success_carries_the_status_when_asked(self):
        self.assertEqual(
            sweep.http_json("https://example.test/x", with_status=True),
            {"id": "abc", "_status": 201},
        )

    def test_a_google_ack_still_succeeds_on_an_unstamped_response(self):
        # The concrete failure the opt-in prevents, at the call site that
        # would have hit it first.
        sweep.gmail_remove_label("at", "msg-1", "Label_7")


class DeriveCaptureTest(unittest.TestCase):
    """#14's "title verbatim" rule, plus the two edges #24 found."""

    def test_title_passes_through_verbatim(self):
        self.assertEqual(
            sweep.derive_capture("  call the vet  ", " before Thursday "),
            ("  call the vet  ", "before Thursday"),
        )

    def test_empty_title_promotes_first_notes_line(self):
        # The plausible real case: a dictation that landed entirely in notes.
        title, description = sweep.derive_capture("", "\n\n  ring mum  \nabout the car\n")
        self.assertEqual(title, "ring mum")
        # Nothing is dropped -- the whole note still becomes the description.
        self.assertEqual(description, "ring mum  \nabout the car")

    def test_whitespace_only_title_counts_as_empty(self):
        self.assertEqual(sweep.derive_capture("   ", "buy milk"), ("buy milk", "buy milk"))

    def test_nothing_at_all_is_none(self):
        for title, notes in (("", ""), (None, None), ("  ", "\n \n"), (None, "")):
            self.assertIsNone(sweep.derive_capture(title, notes))


CFG = sweep.Config(
    google_client_id="cid",
    google_client_secret="secret",
    google_refresh_token="refresh",
    hb_api_token="hb_token",
    hb_api_base="https://hb.example",
    healthcheck_url="https://hc.example/ping",
    denylist_path="/nonexistent/denylist.json",  # fails open: sweep everything
)

ITEMS_URL = "https://hb.example/api/items"


def created(item_id, **over):
    """A 201 answer: the authority returns the whole stored row."""
    row = {"id": item_id, "seq": 1, "title": "t", "stage": "triage", "_status": 201}
    row.update(over)
    return row


def existed(item_id):
    """A 200 answer: the idempotent replay of an id already stored."""
    return dict(created(item_id), _status=200)


def rejected(status, error, message):
    """The authority's error envelope (server/domain/src/api.rs ApiError)."""
    return {"error": error, "message": message, "_status": status}


# Scripted answer meaning "a normal 201 for whatever was posted". The row has
# to carry the posted id: `hb_create_item` refuses a 201 that is not the item,
# which is what stops the PWA's static shell being mistaken for a create.
ECHO = "echo-the-posted-row"

LISTS = {"items": [{"id": "list-1", "title": "My Tasks"}]}
TASKS = {
    "items": [
        {"id": "task-1", "title": "call the vet", "notes": ""},
        {"id": "task-2", "title": "book flights", "notes": "  before Thursday  "},
    ]
}


class FakeHttp:
    """Records every call; answers reads, and whatever the test scripts."""

    def __init__(self, hb_responses=None, tasks=None):
        self.calls = []
        self.hb_responses = list(hb_responses or [])
        self.tasks = tasks if tasks is not None else TASKS

    def __call__(self, url, method="GET", headers=None, body=None, with_status=False):
        self.calls.append({"url": url, "method": method, "headers": headers, "body": body})
        if url == sweep.GOOGLE_TOKEN_URL:
            return {"access_token": "at"}
        if url == ITEMS_URL:
            assert with_status, "the create must ask for the status: 201 and 200 differ"
            scripted = self.hb_responses.pop(0) if self.hb_responses else ECHO
            if scripted == ECHO:
                # What the real route does: answer with the whole stored row.
                return created(body["id"], title=body["title"])
            return scripted
        if "/users/@me/lists" in url:
            return dict(LISTS)
        if url.endswith("maxResults=100") and "/tasks?" in url:
            return dict(self.tasks)
        if method == "PATCH":
            return {"status": "completed"}
        raise AssertionError("unexpected request: %s %s" % (method, url))

    def creates(self):
        return [call for call in self.calls if call["url"] == ITEMS_URL]

    def mutating(self):
        return [
            call
            for call in self.calls
            if call["url"] == ITEMS_URL or call["method"] == "PATCH"
        ]


def run_tasks_adapter(cfg, dry_run):
    """The pre-seam `run_sweep(cfg, dry_run)` contract, for the Tasks adapter.

    These tests predate the adapter seam and assert Google Tasks behavior is
    unchanged by it -- so they drive just that adapter through the shared
    engine and keep their original (ok, failures, notes) shape."""
    result = sweep.run_adapter(sweep.GoogleTasksAdapter(cfg), dry_run)
    return result.ok, result.failures, result.notes


class SweepFlowTest(unittest.TestCase):
    def setUp(self):
        self.real_http = sweep.http_json

    def tearDown(self):
        sweep.http_json = self.real_http

    def run_with(self, fake, dry_run=False):
        sweep.http_json = fake
        return run_tasks_adapter(CFG, dry_run)

    def patched_tasks(self, fake):
        return [c["url"] for c in fake.mutating() if c["method"] == "PATCH"]

    def test_dry_run_mutates_nothing(self):
        fake = FakeHttp()
        ok, failures, notes = self.run_with(fake, dry_run=True)
        self.assertTrue(ok)
        self.assertEqual(failures, [])
        self.assertEqual(notes, [])
        self.assertEqual(fake.mutating(), [])

    def test_live_run_creates_then_patches_per_item(self):
        fake = FakeHttp()
        ok, failures, _ = self.run_with(fake)
        self.assertTrue(ok)
        self.assertEqual(failures, [])

        sequence = [(c["method"], c["url"]) for c in fake.mutating()]
        self.assertEqual(
            sequence,
            [
                ("POST", ITEMS_URL),
                ("PATCH", sequence[1][1]),
                ("POST", ITEMS_URL),
                ("PATCH", sequence[3][1]),
            ],
        )
        self.assertIn("task-1", sequence[1][1])
        self.assertIn("task-2", sequence[3][1])

        first = fake.creates()[0]["body"]
        self.assertEqual(first["id"], sweep.deterministic_v4("task-1"))
        self.assertEqual(first["title"], "call the vet")
        self.assertNotIn("description", first)  # empty notes -> no description
        # The landing stage is the route's own default; saying it here would
        # be a second copy of a fact the server owns.
        self.assertNotIn("stage", first)

        second = fake.creates()[1]["body"]
        self.assertEqual(second["description"], "before Thursday")

        self.assertEqual(
            fake.creates()[0]["headers"]["Authorization"], "Bearer hb_token"
        )

    def test_the_create_carries_its_own_provenance(self):
        # The columns the owned schema reserved for exactly this
        # (server/domain/src/item.rs). source_key is the raw task id -- the
        # same string hashed into the deterministic id, kept legible.
        fake = FakeHttp()
        self.run_with(fake)
        body = fake.creates()[0]["body"]
        self.assertEqual(body["source"], "google-tasks/v1")
        self.assertEqual(body["source_key"], "task-1")
        # A Tasks row has no addressable url, so the field is simply absent
        # rather than sent empty.
        self.assertNotIn("source_url", body)

    def test_already_exists_is_success(self):
        # The authority answers a replay of a known id with 200 and the stored
        # row -- no write, no version bump. That is the crash-window retry.
        fake = FakeHttp(hb_responses=[existed("a"), existed("b")])
        ok, failures, _ = self.run_with(fake)
        self.assertTrue(ok)
        self.assertEqual(failures, [])
        self.assertEqual(len(self.patched_tasks(fake)), 2)

    def test_a_409_is_treated_as_already_exists(self):
        # Unreachable on this route today -- 409 belongs to PATCH's stale
        # expected_version -- but on a pure create path it could only ever
        # mean already-exists, so it must never cost a capture.
        fake = FakeHttp(hb_responses=[dict(existed("a"), _status=409), existed("b")])
        ok, failures, _ = self.run_with(fake)
        self.assertTrue(ok)
        self.assertEqual(failures, [])
        self.assertEqual(len(self.patched_tasks(fake)), 2)

    def test_a_dead_token_is_transient_and_rings(self):
        # 401 answers with no body at all, so there is nothing to classify --
        # which is exactly why the default has to be transient.
        fake = FakeHttp(hb_responses=[{"_status": 401}, ECHO])
        ok, failures, _ = self.run_with(fake)
        self.assertFalse(ok)
        self.assertEqual(len(failures), 1)
        patches = self.patched_tasks(fake)
        self.assertEqual(len(patches), 1)  # task-1 stays incomplete for the retry

    def test_a_201_without_the_item_row_fails_rather_than_acking(self):
        # The authority shares an origin with the PWA, so a misrouted request
        # gets the static shell back with a 200/201. Acking on that would
        # discard the capture; it has to ring instead.
        shell = {"_status": 201, "_raw": "<!doctype html>"}
        fake = FakeHttp(hb_responses=[shell, ECHO])
        ok, failures, _ = self.run_with(fake)
        self.assertFalse(ok)
        self.assertIn("misrouted", failures[0])
        self.assertEqual(len(self.patched_tasks(fake)), 1)

    def test_unparseable_error_skips_patch_and_fails_the_run(self):
        # Deliberately still a hard failure. This payload is not a validation
        # error at all, so the sweeper cannot tell a junk row from a broken
        # sweeper -- and an unrecognized shape must fail loud rather than
        # quietly quarantine. Quarantine has to earn itself; see #24.
        broken = rejected(400, "bad_json", "expected value at line 1 column 1")
        good = ECHO
        fake = FakeHttp(hb_responses=[broken, good])
        ok, failures, _ = self.run_with(fake)

        self.assertFalse(ok)
        self.assertEqual(len(failures), 1)
        self.assertIn("task-1", failures[0])

        patches = self.patched_tasks(fake)
        self.assertEqual(len(patches), 1)  # task-1 stays incomplete for the retry
        self.assertIn("task-2", patches[0])

    def test_a_malformed_item_fails_only_itself(self):
        # A row the adapter cannot even describe must fail that row and let the
        # drain continue -- preparation is inside the per-item try, so it can
        # never abort the rest of the adapter's list.
        tasks = {"items": ["not-a-task", {"id": "task-9", "title": "call the vet"}]}
        fake = FakeHttp(tasks=tasks)
        ok, failures, _ = self.run_with(fake)

        self.assertFalse(ok)
        self.assertEqual(len(failures), 1)
        self.assertIn("unidentified google-tasks item", failures[0])

        patches = self.patched_tasks(fake)
        self.assertEqual(len(patches), 1)  # the good row still swept and acked
        self.assertIn("task-9", patches[0])

    def test_denylisted_list_is_skipped(self):
        import json
        import tempfile

        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as handle:
            json.dump({"list-1": "My Tasks"}, handle)
            path = handle.name
        cfg = CFG._replace(denylist_path=path)

        fake = FakeHttp()
        sweep.http_json = fake
        ok, failures, _ = run_tasks_adapter(cfg, False)

        self.assertTrue(ok)
        self.assertEqual(failures, [])
        self.assertEqual(fake.mutating(), [])
        self.assertFalse(any("/tasks?" in call["url"] for call in fake.calls))

    def test_unknown_denylist_id_fails_open(self):
        """A stale or unknown key skips nothing -- noise in Triage, never a lost
        capture. The `_comment` key the real denylist.json carries is itself such
        a key, so this is the shape in production, not a hypothetical. Inverting
        this to fail closed would silently drop every capture in the list."""
        import json
        import tempfile

        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as handle:
            json.dump({"_comment": "not a list id", "list-gone": "Renamed away"}, handle)
            path = handle.name
        cfg = CFG._replace(denylist_path=path)

        fake = FakeHttp()
        sweep.http_json = fake
        ok, failures, _ = run_tasks_adapter(cfg, False)

        self.assertTrue(ok)
        self.assertEqual(failures, [])
        self.assertTrue(any("/tasks?" in call["url"] for call in fake.calls))
        self.assertEqual(len(fake.creates()), 2)  # both TASKS items swept


def validation_error(message):
    """The authority's validation envelope (handlers/items.rs).

    Prose, not a structured `property` -- so the field a rejection names is
    the message's first word, and that is the whole basis on which a capture
    can earn quarantine.
    """
    return rejected(400, "validation", message)


class TerminalFailureTest(unittest.TestCase):
    """#24: no single item may hold the dead-man's switch red forever.

    A permanently-red alarm is indistinguishable from a working one, so a
    failure that can never clear must not be retried into one.
    """

    GOOD = ECHO
    BLANK_TITLE = "title must be non-empty"

    def setUp(self):
        self.real_http = sweep.http_json

    def tearDown(self):
        sweep.http_json = self.real_http

    def run_with(self, fake):
        sweep.http_json = fake
        return run_tasks_adapter(CFG, False)

    def patched_tasks(self, fake):
        return [c["url"] for c in fake.mutating() if c["method"] == "PATCH"]

    def test_blank_row_is_skipped_before_the_authority_is_called(self):
        # The literal #24 incident: rows made by pressing Enter in the Tasks
        # app. They carry no information, so there is nothing to lose.
        fake = FakeHttp(
            tasks={
                "items": [
                    {"id": "blank-1", "title": "", "notes": None},
                    {"id": "task-1", "title": "call the vet", "notes": ""},
                ]
            }
        )
        ok, failures, notes = self.run_with(fake)

        self.assertTrue(ok, "a blank row must never fail the run")
        self.assertEqual(failures, [])
        self.assertIn("1 empty captures skipped (no title, no notes)", notes)

        # Never offered to the authority, and never disposed of in Tasks
        # either -- it stays visible for a human to delete.
        creates = fake.creates()
        self.assertEqual(len(creates), 1)
        self.assertEqual(creates[0]["body"]["title"], "call the vet")
        patches = self.patched_tasks(fake)
        self.assertEqual(len(patches), 1)
        self.assertIn("task-1", patches[0])

    def test_notes_only_row_is_captured_under_its_first_line(self):
        fake = FakeHttp(
            tasks={"items": [{"id": "n-1", "title": "", "notes": "ring mum\nabout the car"}]}
        )
        ok, failures, _ = self.run_with(fake)

        self.assertTrue(ok)
        self.assertEqual(failures, [])
        posted = fake.creates()[0]["body"]
        self.assertEqual(posted["title"], "ring mum")
        self.assertEqual(posted["description"], "ring mum\nabout the car")
        self.assertEqual(len(self.patched_tasks(fake)), 1)  # and disposed of normally

    def test_content_rejection_is_quarantined_not_retried(self):
        # Belt to the skip's braces: whatever else the authority one day
        # refuses on title or description gets set aside rather than wedging
        # the alarm.
        fake = FakeHttp(hb_responses=[validation_error(self.BLANK_TITLE), self.GOOD])
        ok, failures, notes = self.run_with(fake)

        self.assertTrue(ok, "a content rejection must not fail the run")
        self.assertEqual(failures, [])
        self.assertTrue(any("1 quarantined" in line for line in notes))
        self.assertTrue(any("task-1" in line for line in notes))

        patches = self.patched_tasks(fake)
        self.assertEqual(len(patches), 1, "the quarantined row stays visible in Tasks")
        self.assertIn("task-2", patches[0])

    def test_rejection_on_a_non_content_field_still_fails_the_run(self):
        # The systematic case. These names moved with the retarget -- it used
        # to be a wrong teamId or stateId -- but the property is identical: a
        # rejection naming a field the *sweeper* supplied breaks every capture
        # alike and must keep ringing the alarm.
        for message in (
            "id must be non-empty",
            "priority must be between 0 and 4",
            "unknown project_id",
            "deadline must be YYYY-MM-DD or YYYY-MM-DDTHH:MM",
        ):
            with self.subTest(message=message):
                fake = FakeHttp(hb_responses=[validation_error(message), self.GOOD])
                ok, failures, _ = self.run_with(fake)

                self.assertFalse(ok)
                self.assertEqual(len(failures), 1)
                self.assertIn("task-1", failures[0])

    def test_a_non_validation_rejection_stays_transient(self):
        # `bad_json` means the sweeper built a body the route would not parse
        # -- a broken sweeper, not a bad row. Only `validation` can ever earn
        # quarantine, and only then on a content field.
        fake = FakeHttp(
            hb_responses=[rejected(400, "bad_json", "unknown field `titel`"), self.GOOD]
        )
        ok, failures, _ = self.run_with(fake)

        self.assertFalse(ok)
        self.assertEqual(len(failures), 1)
        self.assertIn("task-1", failures[0])

    def test_a_5xx_is_transient_whatever_its_body_says(self):
        # The server failing to answer says nothing about the capture, so the
        # status is checked before the body is believed.
        rejection = validation_error(self.BLANK_TITLE)
        rejection["_status"] = 503
        fake = FakeHttp(hb_responses=[rejection, self.GOOD])
        ok, failures, _ = self.run_with(fake)

        self.assertFalse(ok, "a 5xx must stay retryable, not be quarantined")
        self.assertEqual(len(failures), 1)
        self.assertIn("task-1", failures[0])

    def test_quarantine_limit_is_a_backstop(self):
        count = sweep.QUARANTINE_LIMIT + 1
        fake = FakeHttp(
            hb_responses=[validation_error(self.BLANK_TITLE)] * count,
            tasks={
                "items": [
                    {"id": "t-%d" % n, "title": "junk %d" % n, "notes": ""}
                    for n in range(count)
                ]
            },
        )
        ok, failures, _ = self.run_with(fake)

        self.assertFalse(ok, "wholesale quarantine is a broken sweeper, not bad input")
        self.assertTrue(any("systematic" in line for line in failures))


class PingSecrecyTest(unittest.TestCase):
    """HEALTHCHECK_URL is a bearer secret: holding it lets anyone forge a
    success ping and silence the dead-man's switch. It must never reach stdout,
    which on Fly is the log stream."""

    SECRET = "https://hc-ping.com/6bd23d5f-1c8e-43a6-8dd4-697c9db72ce7"

    def setUp(self):
        self.real_http = sweep.http_json

    def tearDown(self):
        sweep.http_json = self.real_http

    def _capture(self, call):
        import contextlib
        import io

        buffer = io.StringIO()
        with contextlib.redirect_stdout(buffer):
            call()
        return buffer.getvalue()

    def test_success_ping_logs_no_url(self):
        sweep.http_json = lambda *a, **k: {}
        output = self._capture(lambda: sweep.ping_success(self.SECRET))
        self.assertNotIn("hc-ping.com", output)
        self.assertNotIn("6bd23d5f", output)
        self.assertIn("healthcheck success ping sent", output)

    def test_failed_ping_logs_no_url(self):
        def boom(*args, **kwargs):
            raise RuntimeError("connection refused to %s/fail" % self.SECRET)

        sweep.http_json = boom
        output = self._capture(lambda: sweep.ping_failure(self.SECRET, "why"))
        self.assertNotIn("6bd23d5f", output)
        self.assertIn("<redacted>", output)

    def test_ping_failure_never_raises(self):
        def boom(*args, **kwargs):
            raise RuntimeError("down")

        sweep.http_json = boom
        self._capture(lambda: sweep.ping_failure(self.SECRET, "why"))  # must not raise


if __name__ == "__main__":
    unittest.main()
