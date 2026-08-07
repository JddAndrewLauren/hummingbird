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


CFG = sweep.Config(
    google_client_id="cid",
    google_client_secret="secret",
    google_refresh_token="refresh",
    linear_api_key="lin_key",
    healthcheck_url="https://hc.example/ping",
    denylist_path="/nonexistent/denylist.json",  # fails open: sweep everything
)

LISTS = {"items": [{"id": "list-1", "title": "My Tasks"}]}
TASKS = {
    "items": [
        {"id": "task-1", "title": "call the vet", "notes": ""},
        {"id": "task-2", "title": "book flights", "notes": "  before Thursday  "},
    ]
}


class FakeHttp:
    """Records every call; answers reads, and whatever the test scripts."""

    def __init__(self, linear_responses=None):
        self.calls = []
        self.linear_responses = list(linear_responses or [])

    def __call__(self, url, method="GET", headers=None, body=None):
        self.calls.append({"url": url, "method": method, "headers": headers, "body": body})
        if url == sweep.GOOGLE_TOKEN_URL:
            return {"access_token": "at"}
        if url == sweep.LINEAR_URL:
            if self.linear_responses:
                return self.linear_responses.pop(0)
            return {"data": {"issueCreate": {"success": True, "issue": {"id": "x"}}}}
        if "/users/@me/lists" in url:
            return dict(LISTS)
        if url.endswith("maxResults=100") and "/tasks?" in url:
            return dict(TASKS)
        if method == "PATCH":
            return {"status": "completed"}
        raise AssertionError("unexpected request: %s %s" % (method, url))

    def mutating(self):
        return [
            call
            for call in self.calls
            if call["url"] == sweep.LINEAR_URL or call["method"] == "PATCH"
        ]


class SweepFlowTest(unittest.TestCase):
    def setUp(self):
        self.real_http = sweep.http_json

    def tearDown(self):
        sweep.http_json = self.real_http

    def run_with(self, fake, dry_run=False):
        sweep.http_json = fake
        return sweep.run_sweep(CFG, dry_run)

    def test_dry_run_mutates_nothing(self):
        fake = FakeHttp()
        ok, failures = self.run_with(fake, dry_run=True)
        self.assertTrue(ok)
        self.assertEqual(failures, [])
        self.assertEqual(fake.mutating(), [])

    def test_live_run_creates_then_patches_per_item(self):
        fake = FakeHttp()
        ok, failures = self.run_with(fake)
        self.assertTrue(ok)
        self.assertEqual(failures, [])

        sequence = [(c["method"], c["url"]) for c in fake.mutating()]
        self.assertEqual(
            sequence,
            [
                ("POST", sweep.LINEAR_URL),
                ("PATCH", sequence[1][1]),
                ("POST", sweep.LINEAR_URL),
                ("PATCH", sequence[3][1]),
            ],
        )
        self.assertIn("task-1", sequence[1][1])
        self.assertIn("task-2", sequence[3][1])

        first = fake.mutating()[0]["body"]["variables"]["input"]
        self.assertEqual(first["id"], sweep.deterministic_v4("task-1"))
        self.assertEqual(first["title"], "call the vet")
        self.assertEqual(first["teamId"], sweep.TEAM_ID)
        self.assertEqual(first["stateId"], sweep.STATE_ID)
        self.assertNotIn("description", first)  # empty notes -> no description

        second = fake.mutating()[2]["body"]["variables"]["input"]
        self.assertEqual(second["description"], "before Thursday")

        self.assertEqual(fake.mutating()[0]["headers"]["Authorization"], "lin_key")

    def test_already_exists_is_success(self):
        exists = {
            "_status": 400,
            "errors": [
                {
                    "message": "Entity Issue with id abc already exists.",
                    "extensions": {
                        "code": "INPUT_ERROR",
                        "userPresentableMessage": "Entity Issue with id abc already exists.",
                    },
                }
            ],
        }
        fake = FakeHttp(linear_responses=[exists, exists])
        ok, failures = self.run_with(fake)
        self.assertTrue(ok)
        self.assertEqual(failures, [])
        self.assertEqual(len([c for c in fake.mutating() if c["method"] == "PATCH"]), 2)

    def test_other_error_skips_patch_and_fails_the_run(self):
        broken = {
            "_status": 400,
            "errors": [
                {
                    "message": "Argument Validation Error",
                    "extensions": {"code": "INPUT_ERROR", "userPresentableMessage": "nope"},
                }
            ],
        }
        good = {"data": {"issueCreate": {"success": True, "issue": {"id": "x"}}}}
        fake = FakeHttp(linear_responses=[broken, good])
        ok, failures = self.run_with(fake)

        self.assertFalse(ok)
        self.assertEqual(len(failures), 1)
        self.assertIn("task-1", failures[0])

        patches = [c for c in fake.mutating() if c["method"] == "PATCH"]
        self.assertEqual(len(patches), 1)  # task-1 stays incomplete for the retry
        self.assertIn("task-2", patches[0]["url"])

    def test_denylisted_list_is_skipped(self):
        import json
        import tempfile

        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as handle:
            json.dump({"list-1": "My Tasks"}, handle)
            path = handle.name
        cfg = CFG._replace(denylist_path=path)

        fake = FakeHttp()
        sweep.http_json = fake
        ok, failures = sweep.run_sweep(cfg, False)

        self.assertTrue(ok)
        self.assertEqual(failures, [])
        self.assertEqual(fake.mutating(), [])
        self.assertFalse(any("/tasks?" in call["url"] for call in fake.calls))


if __name__ == "__main__":
    unittest.main()
