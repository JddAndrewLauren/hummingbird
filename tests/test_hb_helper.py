"""Cred-free tests for the three skills' authority helper scripts (#115).

The Linear-era ancestor of this file (`test_linear_helper.py`) stubbed
`curl` on `PATH` and fed one canned GraphQL response to `linear.sh survey`,
because the survey's whole substance was a large jq program over that
response. The owned API moved that substance out of bash entirely -- the
selection and ranking live in `client/next-up` and are natively tested --
so what is left in these scripts is *plumbing*, and this file tests the
plumbing's own decisions:

- ``HB-<seq>`` resolution, which no route performs;
- the one-sweep-per-run cache, which is a file rather than a variable for a
  reason a variable version would silently violate;
- CAS with one safe bounded retry, including disjoint, already-applied and
  same-field conflicts;
- idempotence on writes whose value already holds;
- the ``blocked_by`` argument order, the one thing that used to be
  invertible;
- deterministic batch preparation, protected credentials and the archive
  seam;
- and the refusals: an unknown ref, invalid manifests, and ``move <ref> done``.

Everything network-shaped goes through a fake ``curl`` on ``PATH`` that
serves a fixture sweep and records every request, so no test needs a token,
a network, or a running authority.
"""

import json
import os
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SKILLS = ROOT / ".claude/skills"
MICROTASK = SKILLS / "microtask/scripts/hb.sh"
TO_ACTIONS = SKILLS / "to-actions/scripts/hb.sh"
NEXT_UP = SKILLS / "next-up-hb/scripts/next-up.sh"

ITEM_A = "aaaaaaaa-0000-4000-8000-000000000001"
ITEM_B = "bbbbbbbb-0000-4000-8000-000000000002"
STEP_1 = "11111111-0000-4000-8000-000000000001"
STEP_2 = "22222222-0000-4000-8000-000000000002"
STEP_GONE = "33333333-0000-4000-8000-000000000003"


def item(uuid, seq, title, **over):
    row = {
        "id": uuid, "seq": seq, "title": title, "description": None,
        "stage": "ready", "size": None, "energy": None, "context": None,
        "priority": 0, "project_id": None, "project_pos": None,
        "deadline": None, "scheduled_date": None,
        "source": None, "source_key": None, "source_url": None,
        "archived_at": None, "agent": False,
        "created_at": 1, "updated_at": 1, "version": 7,
    }
    row.update(over)
    return row


def project(uuid, name, **over):
    row = {
        "id": uuid, "name": name, "github_repo": None, "default_context": None,
        "archived_at": None, "created_at": 1, "updated_at": 1, "version": 1,
    }
    row.update(over)
    return row


def step(uuid, item_id, body, position, **over):
    row = {
        "id": uuid, "item_id": item_id, "body": body, "done": False,
        "position": position, "deleted_at": None, "version": 3,
    }
    row.update(over)
    return row


def sweep(items=None, steps=None, **over):
    payload = {
        "version": 7,
        "items": items if items is not None else [
            item(ITEM_A, 1, "the marked chore", agent=True),
            item(ITEM_B, 2, "the human's chore"),
        ],
        "projects": [], "routes": [], "fog": [],
        "steps": steps if steps is not None else [
            # Deliberately out of position order, and one soft-deleted, so
            # the sort and the filter are both actually exercised.
            step(STEP_2, ITEM_A, "second", 2),
            step(STEP_GONE, ITEM_A, "superseded", 1, deleted_at=1000),
            step(STEP_1, ITEM_A, "first", 1),
        ],
        "blocked_by": [], "settings": [], "alerts": [],
        "context_snapshots": [], "rules": [],
    }
    payload.update(over)
    return payload


# A fake `curl` matching the flag shape all three scripts use: `-sS -w
# '\n%{http_code}' -X METHOD [-H ...] [-d DATA] URL`. Header files passed as
# `-H @file` are read here so tests can assert the actual header without
# putting the secret in curl's argv. Every request is
# appended to $HB_FAKE_LOG as one JSON line; the response comes from
# $HB_FAKE_PLAN, a list of {match, status, body} consumed in order for
# writes, with GET /api/sweep always answering the fixture.
FAKE_CURL = r'''#!/usr/bin/env python3
import json, os, sys

argv = sys.argv[1:]
method, data, url, headers = "GET", None, None, []
i = 0
while i < len(argv):
    a = argv[i]
    if a == "-X":
        method = argv[i + 1]; i += 2
    elif a == "-d":
        data = argv[i + 1]; i += 2
    elif a == "-H":
        header = argv[i + 1]
        if header.startswith("@"):
            header = open(header[1:]).read().rstrip("\n")
        headers.append(header); i += 2
    elif a in ("-o", "-w", "--connect-timeout", "--max-time"):
        i += 2
    elif a.startswith("-"):
        i += 1
    else:
        url = a; i += 1

with open(os.environ["HB_FAKE_LOG"], "a") as log:
    log.write(json.dumps({
        "method": method, "url": url, "data": data,
        "headers": headers, "argv": argv,
    }) + "\n")

plan = json.load(open(os.environ["HB_FAKE_PLAN"]))
if method == "GET" and url.endswith("/api/sweep"):
    print(json.dumps(plan["sweep"]))
    print("200")
    sys.exit(0)

responses = plan.setdefault("responses", [])
state_path = os.environ["HB_FAKE_STATE"]
used = json.load(open(state_path)) if os.path.exists(state_path) else 0
if used >= len(responses):
    print(json.dumps({"error": "no scripted response", "url": url}))
    print("500")
    sys.exit(0)
resp = responses[used]
json.dump(used + 1, open(state_path, "w"))
print(json.dumps(resp["body"]))
print(str(resp["status"]))
'''


FAKE_SHA256SUM = r'''#!/usr/bin/env python3
import hashlib
import os
import sys

state_path = os.environ["HB_FAKE_STATE"]
if os.environ.get("HB_ASSERT_HASH_BEFORE_WRITES") and os.path.exists(state_path):
    print("hashing after the first write", file=sys.stderr)
    sys.exit(1)
digest = hashlib.sha256(sys.stdin.buffer.read()).hexdigest()
print(digest + "  -")
'''


class HelperTestCase(unittest.TestCase):
    """Runs a helper script with a fake `curl` and a fixture sweep."""

    def run_script(self, script, args, *, sweep_payload=None, responses=(),
                   check=True, token_text="hb_fake-token\n",
                   assert_batch_prepared=False):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            token = tmp_path / "api-token"
            if token_text is not None:
                token.write_text(token_text)

            plan = tmp_path / "plan.json"
            plan.write_text(json.dumps({
                "sweep": sweep_payload if sweep_payload is not None else sweep(),
                "responses": list(responses),
            }))

            fake = tmp_path / "curl"
            fake.write_text(FAKE_CURL)
            fake.chmod(fake.stat().st_mode | stat.S_IXUSR)

            if assert_batch_prepared:
                fake_hash = tmp_path / "sha256sum"
                fake_hash.write_text(FAKE_SHA256SUM)
                fake_hash.chmod(fake_hash.stat().st_mode | stat.S_IXUSR)

            log = tmp_path / "requests.log"
            log.write_text("")

            env = os.environ | {
                "PATH": f"{tmp_path}:{os.environ['PATH']}",
                "HB_API_BASE": "https://authority.test",
                "HB_API_TOKEN_PATH": str(token),
                "HB_FAKE_PLAN": str(plan),
                "HB_FAKE_LOG": str(log),
                "HB_FAKE_STATE": str(tmp_path / "state.json"),
                "HB_ASSERT_HASH_BEFORE_WRITES": "1" if assert_batch_prepared else "",
            }
            result = subprocess.run(
                [str(script), *args], cwd=ROOT, env=env,
                capture_output=True, text=True,
            )
            requests = [json.loads(line) for line in log.read_text().splitlines() if line]
            if check and result.returncode != 0:
                self.fail(f"{script.name} {args} exited {result.returncode}\n{result.stderr}")
            return result, requests


class RefResolutionTest(HelperTestCase):
    def test_hb_seq_resolves_to_a_uuid_off_the_sweep(self):
        # No route accepts or resolves `HB-<seq>` -- the mapping is entirely
        # this script's, off a payload it has already fetched.
        result, _ = self.run_script(MICROTASK, ["get", "HB-1"])
        self.assertEqual(ITEM_A, json.loads(result.stdout)["item"]["id"])

    def test_hb_seq_is_case_insensitive_and_a_uuid_passes_through(self):
        lower, _ = self.run_script(MICROTASK, ["get", "hb-1"])
        direct, _ = self.run_script(MICROTASK, ["get", ITEM_A])
        self.assertEqual(ITEM_A, json.loads(lower.stdout)["item"]["id"])
        self.assertEqual(ITEM_A, json.loads(direct.stdout)["item"]["id"])

    def test_an_unknown_ref_is_a_named_failure_not_an_empty_answer(self):
        # The failure mode worth pinning: silently answering "no steps" for
        # an item that does not exist would send the skill off writing a
        # checklist against nothing.
        result, requests = self.run_script(MICROTASK, ["steps", "HB-99"], check=False)
        self.assertNotEqual(0, result.returncode)
        self.assertIn("seq 99", result.stderr)
        self.assertEqual([], [r for r in requests if r["method"] != "GET"])


class SweepCacheTest(HelperTestCase):
    def test_one_run_fetches_the_sweep_exactly_once(self):
        # The cache is a FILE, not a variable, because `resolve_ref` is
        # called as `$(…)` and a variable assigned inside a command
        # substitution never reaches the parent. A variable cache passes
        # every other test in this file and fails only this one -- while
        # letting a single run reason over two different sweeps.
        _, requests = self.run_script(MICROTASK, ["get", "HB-1"])
        sweeps = [r for r in requests if r["url"].endswith("/api/sweep")]
        self.assertEqual(1, len(sweeps), f"fetched the sweep {len(sweeps)} times")

    def test_the_token_never_reaches_a_command_line_argument(self):
        cases = [
            (MICROTASK, ["get", "HB-1"]),
            (TO_ACTIONS, ["project-find", "chore"]),
            (NEXT_UP, ["get", "HB-1"]),
        ]
        for script, args in cases:
            _, requests = self.run_script(script, args)
            self.assertTrue(requests)
            for request in requests:
                self.assertNotIn("hb_fake-token", " ".join(request["argv"]))
                self.assertIn("Authorization: Bearer hb_fake-token", request["headers"])


class CredentialPreflightTest(HelperTestCase):
    def test_missing_or_empty_credentials_stop_before_curl(self):
        cases = [
            (MICROTASK, ["get", "HB-1"]),
            (TO_ACTIONS, ["project-find", "chore"]),
            (NEXT_UP, ["get", "HB-1"]),
        ]
        for token_text in (None, "\n"):
            for script, args in cases:
                result, requests = self.run_script(
                    script, args, token_text=token_text, check=False)
                self.assertNotEqual(0, result.returncode)
                self.assertIn("authority token", result.stderr)
                self.assertEqual([], requests, f"{script} made a request without credentials")


class StepReadTest(HelperTestCase):
    def test_steps_are_position_ordered_and_soft_deleted_rows_are_hidden(self):
        result, _ = self.run_script(MICROTASK, ["steps", "HB-1"])
        rows = json.loads(result.stdout)
        self.assertEqual(["first", "second"], [r["body"] for r in rows])
        self.assertEqual([1, 2], [r["position"] for r in rows])

    def test_add_steps_settles_the_entire_batch_before_the_first_post(self):
        created = [
            {"status": 201, "body": step(STEP_1, ITEM_A, "third", 3)},
            {"status": 201, "body": step(STEP_2, ITEM_A, "fourth", 4)},
        ]
        with tempfile.TemporaryDirectory() as tmp:
            step_file = Path(tmp) / "steps.txt"
            step_file.write_text("third\nfourth\n")
            result, requests = self.run_script(
                MICROTASK, ["add-steps", "HB-1", str(step_file)],
                responses=created, assert_batch_prepared=True)

        self.assertEqual(0, result.returncode)
        posts = [r for r in requests if r["method"] == "POST"]
        self.assertEqual(2, len(posts))
        self.assertEqual([3, 4], [json.loads(r["data"])["position"] for r in posts])


class CasTest(HelperTestCase):
    def test_a_disjoint_409_is_retried_once_against_the_carried_current_version(self):
        conflict = {"status": 409, "body": {
            "error": "version_conflict",
            # Another writer changed the body, not the done field this
            # operation touches, so the absolute tick may be rebased.
            "current": step(STEP_1, ITEM_A, "edited by human", 1, version=9),
        }}
        applied = {"status": 200, "body": step(STEP_1, ITEM_A, "edited by human", 1, done=True, version=10)}
        result, requests = self.run_script(
            MICROTASK, ["tick", STEP_1], responses=[conflict, applied])

        patches = [r for r in requests if r["method"] == "PATCH"]
        self.assertEqual(2, len(patches))
        # The first attempt uses the sweep's version; the retry uses the
        # one the 409 carried, never a re-read and never a blind increment.
        self.assertEqual(3, json.loads(patches[0]["data"])["expected_version"])
        self.assertEqual(9, json.loads(patches[1]["data"])["expected_version"])
        self.assertTrue(json.loads(result.stdout)["done"])

    def test_a_conflict_that_already_has_the_requested_value_is_success(self):
        conflict = {"status": 409, "body": {
            "error": "version_conflict",
            "current": step(STEP_1, ITEM_A, "first", 1, done=True, version=9),
        }}
        result, requests = self.run_script(
            MICROTASK, ["tick", STEP_1], responses=[conflict])

        patches = [r for r in requests if r["method"] == "PATCH"]
        self.assertEqual(1, len(patches), "an already-applied absolute set needs no retry")
        self.assertTrue(json.loads(result.stdout)["done"])

    def test_a_same_field_conflict_stops_without_a_retry(self):
        conflict = {"status": 409, "body": {
            "error": "version_conflict",
            "current": step(STEP_1, ITEM_A, "first", 1, deleted_at=9000, version=9),
        }}
        result, requests = self.run_script(
            MICROTASK, ["drop-step", STEP_1], responses=[conflict], check=False)

        self.assertNotEqual(0, result.returncode)
        self.assertEqual(1, len([r for r in requests if r["method"] == "PATCH"]))
        self.assertIn("same-field conflict", result.stderr)

    def test_a_second_conflict_stops_rather_than_grinding(self):
        conflict = {"status": 409, "body": {
            "error": "version_conflict",
            "current": step(STEP_1, ITEM_A, "first", 1, version=9),
        }}
        result, requests = self.run_script(
            MICROTASK, ["tick", STEP_1], responses=[conflict, conflict], check=False)

        self.assertNotEqual(0, result.returncode)
        self.assertEqual(2, len([r for r in requests if r["method"] == "PATCH"]),
                         "bounded at one retry, like write/adapter.rs's MAX_ATTEMPTS")
        self.assertIn("after one retry", result.stderr)

    def test_next_up_stops_when_a_findings_field_changed(self):
        conflict = {"status": 409, "body": {
            "error": "version_conflict",
            "current": item(ITEM_A, 1, "the marked chore",
                             description="human description", version=9),
        }}
        with tempfile.TemporaryDirectory() as tmp:
            findings = Path(tmp) / "findings.md"
            findings.write_text("new findings")
            result, requests = self.run_script(
                NEXT_UP, ["note", "HB-1", str(findings)],
                responses=[conflict], check=False)

        self.assertNotEqual(0, result.returncode)
        self.assertEqual(1, len([r for r in requests if r["method"] == "PATCH"]))
        self.assertIn("same-field conflict", result.stderr)

    def test_to_actions_rebases_a_route_write_when_another_field_changed(self):
        route = {
            "project_id": "project-1", "destination": None, "notes": "old notes",
            "updated_at": 1, "version": 7,
        }
        payload = sweep(
            projects=[{"id": "project-1", "name": "Project", "archived_at": None}],
            routes=[route],
        )
        conflict_route = dict(route, notes="human notes", version=9)
        applied_route = dict(conflict_route, destination="new destination", version=10)
        with tempfile.TemporaryDirectory() as tmp:
            destination = Path(tmp) / "destination.md"
            destination.write_text("new destination\n")
            result, requests = self.run_script(
                TO_ACTIONS,
                ["route-set", "project-1", "--destination", str(destination)],
                sweep_payload=payload,
                responses=[
                    {"status": 409, "body": {"error": "version_conflict", "current": conflict_route}},
                    {"status": 200, "body": applied_route},
                ])

        self.assertEqual(0, result.returncode)
        patches = [r for r in requests if r["method"] == "PATCH"]
        self.assertEqual(2, len(patches))
        self.assertEqual(7, json.loads(patches[0]["data"])["expected_version"])
        self.assertEqual(9, json.loads(patches[1]["data"])["expected_version"])


class IdempotenceTest(HelperTestCase):
    def test_ticking_an_already_done_step_makes_no_write(self):
        # A step the user ticked in the client is already done when the
        # walk-through gets there; that agreement is the point of Steps
        # being records, so it must not cost a write or a version bump.
        payload = sweep(steps=[step(STEP_1, ITEM_A, "first", 1, done=True)])
        result, requests = self.run_script(
            MICROTASK, ["tick", STEP_1], sweep_payload=payload)
        self.assertEqual([], [r for r in requests if r["method"] == "PATCH"])
        self.assertTrue(json.loads(result.stdout)["done"])

    def test_dropping_an_already_dropped_step_makes_no_write(self):
        payload = sweep(steps=[step(STEP_1, ITEM_A, "first", 1, deleted_at=1000)])
        _, requests = self.run_script(
            MICROTASK, ["drop-step", STEP_1], sweep_payload=payload)
        self.assertEqual([], [r for r in requests if r["method"] == "PATCH"])

    def test_unflagging_an_already_clear_axis_makes_no_write(self):
        # #10's finish step is re-runnable, so this is what makes a
        # half-finished finish safe to simply repeat.
        _, requests = self.run_script(NEXT_UP, ["unflag-agent", "HB-2"])
        self.assertEqual([], [r for r in requests if r["method"] == "PATCH"])


class DelegationTest(HelperTestCase):
    def test_unflag_agent_clears_the_axis_with_false_never_null(self):
        cleared = {"status": 200, "body": item(ITEM_A, 1, "the marked chore", version=8)}
        _, requests = self.run_script(NEXT_UP, ["unflag-agent", "HB-1"], responses=[cleared])
        patch = json.loads([r for r in requests if r["method"] == "PATCH"][0]["data"])
        # `agent` is NOT NULL, so an explicit null is a 400. Clearing it is
        # `false`, exactly as `ItemPatch`'s non-null shim requires.
        self.assertIs(False, patch["agent"])
        self.assertEqual(7, patch["expected_version"])

    def test_the_protocol_refuses_done_and_says_why(self):
        # #10: an agent chore advances a chore, it does not complete it.
        # Enforced in the script, not only asked for in SKILL.md.
        result, requests = self.run_script(
            NEXT_UP, ["move", "HB-1", "done"], check=False)
        self.assertNotEqual(0, result.returncode)
        self.assertIn("never moves an item to done", result.stderr)
        self.assertEqual([], [r for r in requests if r["method"] == "PATCH"])

    def test_an_unknown_stage_is_refused_before_the_seam(self):
        result, requests = self.run_script(
            NEXT_UP, ["move", "HB-1", "Ready"], check=False)
        self.assertNotEqual(0, result.returncode)
        self.assertEqual([], [r for r in requests if r["method"] == "PATCH"])

    def test_get_reports_open_blockers_and_ignores_shut_ones(self):
        shut = item("cccccccc-0000-4000-8000-000000000003", 3, "finished", stage="done")
        live = item("dddddddd-0000-4000-8000-000000000004", 4, "still open")
        payload = sweep(
            items=[item(ITEM_A, 1, "the marked chore", agent=True), shut, live],
            blocked_by=[
                {"item_id": ITEM_A, "blocker_id": shut["id"], "version": 1, "removed_at": None},
                {"item_id": ITEM_A, "blocker_id": live["id"], "version": 1, "removed_at": None},
                {"item_id": ITEM_A, "blocker_id": ITEM_B, "version": 1, "removed_at": 5},
            ],
        )
        result, _ = self.run_script(NEXT_UP, ["get", "HB-1"], sweep_payload=payload)
        blockers = json.loads(result.stdout)["blockers"]
        self.assertEqual([4], [b["seq"] for b in blockers],
                         "a done blocker and a removed edge are both not blocking")


class BlockedByTest(HelperTestCase):
    def test_block_sends_the_pair_in_the_order_it_is_written(self):
        # The one thing that used to be invertible. Under Linear, "A is
        # blocked by B" had to be created as "B blocks A", and reversing it
        # silently inverted the frontier. Here the body reads as written --
        # pinned so it stays that way.
        created = {"status": 201, "body": {
            "item_id": ITEM_A, "blocker_id": ITEM_B, "version": 8, "removed_at": None}}
        _, requests = self.run_script(
            TO_ACTIONS, ["block", "HB-1", "HB-2"], responses=[created])
        body = json.loads([r for r in requests if r["method"] == "POST"][0]["data"])
        self.assertEqual({"item_id": ITEM_A, "blocker_id": ITEM_B}, body)

    def test_an_item_cannot_block_itself(self):
        result, requests = self.run_script(
            TO_ACTIONS, ["block", "HB-1", "HB-1"], check=False)
        self.assertNotEqual(0, result.returncode)
        self.assertEqual([], [r for r in requests if r["method"] == "POST"])


class MintTest(HelperTestCase):
    def _manifest(self, tmp, entries):
        path = Path(tmp) / "manifest.json"
        path.write_text(json.dumps(entries))
        return path

    def test_every_id_is_derived_before_the_first_write_and_is_stable(self):
        # What makes a partial batch replayable: re-running the same
        # manifest re-derives the same ids, so the already-minted half
        # lands on the idempotent already-exists path instead of minting a
        # second copy.
        entries = [{"title": "first action"}, {"title": "second action"}]
        ok = [{"status": 201, "body": {"id": "x", "seq": 1}},
              {"status": 201, "body": {"id": "y", "seq": 2}}]
        with tempfile.TemporaryDirectory() as tmp:
            manifest = self._manifest(tmp, entries)
            _, first = self.run_script(TO_ACTIONS, ["mint", str(manifest)], responses=ok)
            _, second = self.run_script(TO_ACTIONS, ["mint", str(manifest)], responses=ok)

        ids = lambda rs: [json.loads(r["data"])["id"] for r in rs if r["method"] == "POST"]
        self.assertEqual(2, len(ids(first)))
        self.assertEqual(ids(first), ids(second), "the same manifest mints the same ids")
        for derived in ids(first):
            self.assertRegex(derived, r"^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
        self.assertNotEqual(ids(first)[0], ids(first)[1])

    def test_an_id_supplied_in_the_manifest_wins(self):
        pinned = "eeeeeeee-0000-4000-8000-00000000000e"
        ok = [{"status": 201, "body": {"id": pinned}}]
        with tempfile.TemporaryDirectory() as tmp:
            manifest = self._manifest(tmp, [{"id": pinned, "title": "pinned"}])
            _, requests = self.run_script(TO_ACTIONS, ["mint", str(manifest)], responses=ok)
        self.assertEqual(pinned, json.loads(
            [r for r in requests if r["method"] == "POST"][0]["data"])["id"])

    def test_an_omitted_stage_is_normalized_to_ready(self):
        ok = [{"status": 201, "body": {"id": "x", "stage": "ready"}}]
        with tempfile.TemporaryDirectory() as tmp:
            manifest = self._manifest(tmp, [{"title": "ready action"}])
            _, requests = self.run_script(
                TO_ACTIONS, ["mint", str(manifest)], responses=ok)
        body = json.loads([r for r in requests if r["method"] == "POST"][0]["data"])
        self.assertEqual("ready", body["stage"])

    def test_a_non_ready_stage_is_refused_before_any_write(self):
        with tempfile.TemporaryDirectory() as tmp:
            manifest = self._manifest(tmp, [{"title": "wrong stage", "stage": "triage"}])
            result, requests = self.run_script(
                TO_ACTIONS, ["mint", str(manifest)], check=False)
        self.assertNotEqual(0, result.returncode)
        self.assertIn("stage must be ready", result.stderr)
        self.assertEqual([], [r for r in requests if r["method"] == "POST"])

    def test_agent_is_refused_before_any_write(self):
        with tempfile.TemporaryDirectory() as tmp:
            manifest = self._manifest(tmp, [{"title": "delegated action", "agent": True}])
            result, requests = self.run_script(
                TO_ACTIONS, ["mint", str(manifest)], check=False)
        self.assertNotEqual(0, result.returncode)
        self.assertIn("agent", result.stderr)
        self.assertEqual([], [r for r in requests if r["method"] == "POST"])

    def test_a_manifest_entry_with_no_title_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            manifest = self._manifest(tmp, [{"project_pos": 1}])
            result, requests = self.run_script(
                TO_ACTIONS, ["mint", str(manifest)], check=False)
        self.assertNotEqual(0, result.returncode)
        self.assertEqual([], [r for r in requests if r["method"] == "POST"])


class MintDefaultContextTest(HelperTestCase):
    """ADR-0030 decision 3, copy-at-mint: a context-less action minted into a
    project with a `default_context` is filled with it. The context is
    copied onto the item at mint, not resolved at read time."""

    PROJECT = "cccccccc-0000-4000-8000-000000000003"

    def _manifest(self, tmp, entries):
        path = Path(tmp) / "manifest.json"
        path.write_text(json.dumps(entries))
        return path

    def _sweep(self, **project_over):
        return sweep(projects=[project(self.PROJECT, "sell the M3", **project_over)])

    def test_a_context_less_action_is_filled_with_the_projects_default_context(self):
        ok = [{"status": 201, "body": {"id": "x"}}]
        with tempfile.TemporaryDirectory() as tmp:
            manifest = self._manifest(
                tmp, [{"title": "an action", "project_id": self.PROJECT}])
            _, requests = self.run_script(
                TO_ACTIONS, ["mint", str(manifest)],
                sweep_payload=self._sweep(default_context="@computer"), responses=ok)
        body = json.loads([r for r in requests if r["method"] == "POST"][0]["data"])
        self.assertEqual("@computer", body["context"])

    def test_an_action_carrying_its_own_context_is_untouched(self):
        ok = [{"status": 201, "body": {"id": "x"}}]
        with tempfile.TemporaryDirectory() as tmp:
            manifest = self._manifest(tmp, [
                {"title": "an action", "project_id": self.PROJECT, "context": "@calls"}])
            _, requests = self.run_script(
                TO_ACTIONS, ["mint", str(manifest)],
                sweep_payload=self._sweep(default_context="@computer"), responses=ok)
        body = json.loads([r for r in requests if r["method"] == "POST"][0]["data"])
        self.assertEqual("@calls", body["context"])

    def test_a_project_with_no_default_context_leaves_context_absent(self):
        ok = [{"status": 201, "body": {"id": "x"}}]
        with tempfile.TemporaryDirectory() as tmp:
            manifest = self._manifest(
                tmp, [{"title": "an action", "project_id": self.PROJECT}])
            _, requests = self.run_script(
                TO_ACTIONS, ["mint", str(manifest)],
                sweep_payload=self._sweep(default_context=None), responses=ok)
        body = json.loads([r for r in requests if r["method"] == "POST"][0]["data"])
        self.assertNotIn("context", body)

    def test_a_projectless_action_is_untouched_even_with_a_default_context_elsewhere(self):
        ok = [{"status": 201, "body": {"id": "x"}}]
        with tempfile.TemporaryDirectory() as tmp:
            manifest = self._manifest(tmp, [{"title": "a standalone action"}])
            _, requests = self.run_script(
                TO_ACTIONS, ["mint", str(manifest)],
                sweep_payload=self._sweep(default_context="@computer"), responses=ok)
        body = json.loads([r for r in requests if r["method"] == "POST"][0]["data"])
        self.assertNotIn("context", body)

    def test_ids_are_unaffected_by_the_context_fill(self):
        # Filling a field at mint changes what is written, never which row
        # is written, so a re-run against the same manifest addresses the
        # same rows.
        ok = [{"status": 201, "body": {"id": "x"}}]
        entries = [{"title": "an action", "project_id": self.PROJECT}]
        with tempfile.TemporaryDirectory() as tmp:
            manifest = self._manifest(tmp, entries)
            _, filled = self.run_script(
                TO_ACTIONS, ["mint", str(manifest)],
                sweep_payload=self._sweep(default_context="@computer"), responses=ok)
        with tempfile.TemporaryDirectory() as tmp:
            manifest = self._manifest(tmp, entries)
            _, unfilled = self.run_script(
                TO_ACTIONS, ["mint", str(manifest)],
                sweep_payload=self._sweep(default_context=None), responses=ok)
        id_of = lambda rs: json.loads([r for r in rs if r["method"] == "POST"][0]["data"])["id"]
        self.assertEqual(id_of(filled), id_of(unfilled))


class ArchiveTest(HelperTestCase):
    def test_archive_sets_archived_at_under_cas(self):
        archived = item(ITEM_A, 1, "the marked chore", archived_at=9000, version=8)
        _, requests = self.run_script(
            TO_ACTIONS, ["archive", "HB-1"], responses=[{"status": 200, "body": archived}])

        patches = [r for r in requests if r["method"] == "PATCH"]
        self.assertEqual(1, len(patches))
        body = json.loads(patches[0]["data"])
        self.assertEqual(7, body["expected_version"])
        self.assertIsInstance(body["archived_at"], int)

    def test_archiving_an_already_archived_item_makes_no_write(self):
        payload = sweep(items=[item(ITEM_A, 1, "the marked chore", archived_at=9000)])
        _, requests = self.run_script(
            TO_ACTIONS, ["archive", "HB-1"], sweep_payload=payload)
        self.assertEqual([], [r for r in requests if r["method"] == "PATCH"])


def code_of(script):
    """The script with comment-only lines stripped.

    The scope guards below must read the code, not the prose: every one of
    these files explains in a header comment which routes it does *not*
    reach, so a naive substring search over the whole text finds the very
    paths it is asserting the absence of.
    """
    return "\n".join(
        line for line in script.read_text().splitlines()
        if not line.lstrip().startswith("#")
    )


class ScopeGuardTest(unittest.TestCase):
    """The guards that are structural rather than prose."""

    def test_microtask_has_no_verb_that_writes_anything_but_steps(self):
        code = code_of(MICROTASK)
        for forbidden in ["/api/items", "/api/projects", "/api/fog",
                          "/api/routes", "/api/blocked_by"]:
            self.assertNotIn(forbidden, code,
                             f"microtask's hb.sh must not reach {forbidden}")

    def test_to_actions_never_writes_the_delegation_axis_or_a_step(self):
        code = code_of(TO_ACTIONS)
        self.assertNotIn("/api/steps", code)
        self.assertNotIn('"agent":', code)

    def test_only_the_selector_writes_the_delegation_axis(self):
        # The axis has exactly one writer, and it is the skill whose
        # protocol #10 fixed. A second one would be a second place the
        # clear-on-finish rule could be forgotten.
        self.assertIn('{"agent": false}', code_of(NEXT_UP))
        for other in (MICROTASK, TO_ACTIONS):
            self.assertNotIn('"agent":', code_of(other),
                             f"{other.parent.parent.name} must not touch the axis")

    def test_every_helper_resolves_the_token_from_the_same_canonical_path(self):
        # One device token, one resting place. A second location is a
        # second thing to rotate and a second thing to forget.
        for script in (MICROTASK, TO_ACTIONS, NEXT_UP):
            self.assertIn(
                'HB_API_TOKEN_PATH:-$HOME/.config/hummingbird/api-token',
                script.read_text(), f"{script.name} reads a different token path")


if __name__ == "__main__":
    unittest.main()
