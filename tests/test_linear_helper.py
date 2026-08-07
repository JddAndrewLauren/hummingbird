import datetime as dt
import json
import os
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
LINEAR = ROOT / ".claude/skills/next-up-personal/scripts/linear.sh"
TODAY = dt.date(2026, 8, 7)


def issue(identifier, state, state_type, *, due_date=None, blockers=None):
    return {
        "identifier": identifier,
        "title": identifier,
        "url": f"https://linear.app/issue/{identifier}",
        "priority": 0,
        "priorityLabel": "No priority",
        "dueDate": due_date,
        "createdAt": "2026-01-01T00:00:00.000Z",
        "state": {"name": state, "type": state_type},
        "project": None,
        "labels": {"nodes": []},
        "inverseRelations": {"nodes": blockers or []},
    }


class LinearSurveyFrontierTest(unittest.TestCase):
    def run_survey(self, issues):
        response = {
            "data": {
                "issues": {
                    "pageInfo": {"hasNextPage": False},
                    "nodes": issues,
                },
                "projects": {
                    "pageInfo": {"hasNextPage": False},
                    "nodes": [],
                },
            }
        }

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            key_path = tmp_path / "api-key"
            key_path.write_text("test-key")

            for name, body in {
                "curl": '#!/usr/bin/env bash\nprintf "%s" "$LINEAR_MOCK_RESPONSE"\n',
                "date": '#!/usr/bin/env bash\nprintf "2026-08-07\\n"\n',
            }.items():
                executable = tmp_path / name
                executable.write_text(body)
                executable.chmod(executable.stat().st_mode | stat.S_IXUSR)

            env = os.environ | {
                "PATH": f"{tmp_path}:{os.environ['PATH']}",
                "LINEAR_API_KEY_PATH": str(key_path),
                "LINEAR_MOCK_RESPONSE": json.dumps(response),
            }
            result = subprocess.run(
                [str(LINEAR), "survey"],
                cwd=ROOT,
                env=env,
                check=True,
                capture_output=True,
                text=True,
            )
            return json.loads(result.stdout)

    def test_includes_non_frontier_state_due_within_seven_days(self):
        due_soon = (TODAY + dt.timedelta(days=7)).isoformat()
        too_far_out = (TODAY + dt.timedelta(days=8)).isoformat()

        survey = self.run_survey(
            [
                issue("ION-1", "Triage", "triage", due_date=due_soon),
                issue("ION-5", "Triage", "triage", due_date=too_far_out),
            ]
        )

        self.assertEqual(["ION-1"], [item["identifier"] for item in survey["candidates"]])
        self.assertTrue(survey["candidates"][0]["dueSoon"])

    def test_excludes_externally_blocked_issue_even_when_overdue(self):
        overdue = (TODAY - dt.timedelta(days=1)).isoformat()

        survey = self.run_survey(
            [issue("ION-2", "Blocked", "started", due_date=overdue)]
        )

        self.assertEqual([], survey["candidates"])

    def test_still_excludes_ready_issue_with_open_relation_blocker(self):
        blocker = {
            "type": "blocks",
            "issue": {
                "identifier": "ION-4",
                "state": {"name": "Ready", "type": "unstarted"},
            },
        }

        survey = self.run_survey(
            [issue("ION-3", "Ready", "unstarted", blockers=[blocker])]
        )

        self.assertEqual([], survey["candidates"])
        self.assertEqual(["ION-3"], [item["identifier"] for item in survey["blocked"]])


if __name__ == "__main__":
    unittest.main()
