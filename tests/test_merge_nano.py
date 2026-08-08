"""Tests for the bake-off's Nano merge script.

`python3 -m unittest discover -s tests`

Everything runs against a COPY of scoring.md in a temp dir -- the real sheet is a
hand-edited artifact that the operator and other sessions may be filling in parallel,
and a test is not allowed to touch it.
"""

import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BAKEOFF = ROOT / "experiments/capture-parse-bakeoff"
sys.path.insert(0, str(BAKEOFF))

import merge_nano  # noqa: E402


def corpus_ids():
    with open(BAKEOFF / "corpus.jsonl", encoding="utf-8") as fh:
        return [json.loads(line)["id"] for line in fh if line.strip()]


class MergeNanoTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.tmp)
        self.scoring = os.path.join(self.tmp, "scoring.md")
        shutil.copy(BAKEOFF / "scoring.md", self.scoring)
        self.results = os.path.join(self.tmp, "nano_results.jsonl")
        self.ids = corpus_ids()

    def write_results(self, rows):
        with open(self.results, "w", encoding="utf-8") as fh:
            for row in rows:
                fh.write(json.dumps(row, ensure_ascii=False) + "\n")

    def run_script(self, *extra):
        """Run merge_nano.py as the operator does -> (returncode, stdout, stderr)."""
        proc = subprocess.run(
            [sys.executable, str(BAKEOFF / "merge_nano.py"),
             "--results", self.results, "--scoring", self.scoring, *extra],
            capture_output=True, text=True,
        )
        return proc.returncode, proc.stdout, proc.stderr

    def nano_cell(self, cid):
        with open(self.scoring, encoding="utf-8") as fh:
            for line in fh:
                if line.startswith(f"| {cid} |"):
                    return line.rstrip("\n").split(" | ")[3]
        self.fail(f"no scoring row for {cid}")

    # --- the mixed fixture: valid / invalid / error / missing / extra ---------

    def mixed_rows(self):
        a, b, c, d, e, f = self.ids[:6]
        return [
            {"id": a, "parse": {"title": "Email Dana the roadmap", "due": "2026-08-13"}},
            {"id": b, "parse": {"title": "A", "items": ["A", "B"],
                                "notes": "has a | pipe\nand a newline"}},
            {"id": c, "parse": {"title": "x", "label": {"context": "@spaceship"}}},
            {"id": d, "parse": {"notes": "no title at all"}},
            {"id": e, "error": "model output is not a strict JSON object",
             "raw_output": '```json\n{"title": "fenced"}\n```'},
            {"id": f, "error": "com.google.mlkit.genai.common.GenAiException code=9: busy",
             "raw_output": None},
            {"id": "not-a-capture", "parse": {"title": "stray"}},
        ]

    def test_mixed_fixture_fills_cells_and_reports_every_problem(self):
        self.write_results(self.mixed_rows())
        code, out, err = self.run_script()

        self.assertEqual(1, code, "anything unresolved must exit 1")
        a, b, c, d, e, f = self.ids[:6]

        self.assertIn("**t:** Email Dana the roadmap", self.nano_cell(a))
        self.assertIn("**INVALID:**", self.nano_cell(c))
        self.assertIn("**INVALID:**", self.nano_cell(d))
        # e is a fenced-but-valid output: unwrapped and scored, still marked.
        self.assertIn("**FENCED:**", self.nano_cell(e))
        self.assertIn("**ERR:**", self.nano_cell(f))

        # Errors are reported, not swallowed, and the extra/missing ids are named.
        self.assertIn("not-a-capture", err)
        self.assertIn(self.ids[6], err)          # first id with no result at all
        self.assertIn("@spaceship", err)         # the enum violation

        # The summary on stdout is what gets pasted into the issue comment.
        # 4, not 3: the stray "not-a-capture" row is itself a valid parse (the count
        # is of what the phone reported; the mismatch is reported separately), and the
        # fenced row is scored on its unwrapped parse.
        self.assertIn("schema-valid parses: 4", out)
        self.assertIn("arrived wrapped in a code fence: 1", out)
        self.assertIn("parses that missed the schema: 2", out)
        self.assertIn("rows the model/API failed outright: 1", out)

    def test_cells_are_sanitised_so_model_text_cannot_reshape_the_table(self):
        self.write_results(self.mixed_rows())
        self.run_script()
        cell = self.nano_cell(self.ids[1])
        self.assertNotIn(" | ", cell, "an unescaped pipe would split the row")
        self.assertIn(r"\|", cell)
        self.assertNotIn("\n", cell)
        self.assertIn("<br>", cell)

        fenced = self.nano_cell(self.ids[4])
        self.assertNotIn("\n", fenced)

    def test_every_reported_row_gets_a_cell(self):
        self.write_results(self.mixed_rows())
        self.run_script()
        for cid in self.ids[:6]:
            self.assertNotEqual("_TODO_", self.nano_cell(cid).strip(),
                                f"{cid} was reported on but left as _TODO_")

    def test_unreported_ids_stay_todo(self):
        self.write_results(self.mixed_rows())
        self.run_script()
        for cid in self.ids[6:]:
            self.assertEqual("_TODO_", self.nano_cell(cid).strip())

    def test_check_mode_writes_nothing(self):
        self.write_results(self.mixed_rows())
        before = Path(self.scoring).read_bytes()
        code, out, err = self.run_script("--check")
        self.assertEqual(1, code)
        self.assertEqual(before, Path(self.scoring).read_bytes())
        self.assertIn("nothing written", err)

    def test_clean_full_run_exits_zero_and_fills_every_row(self):
        self.write_results(
            [{"id": cid, "parse": {"title": f"parse for {cid}"}} for cid in self.ids]
        )
        code, out, err = self.run_script()
        self.assertEqual(0, code, err)
        self.assertIn(f"cells filled this run: {len(self.ids)}", out)
        for cid in self.ids:
            self.assertIn(f"parse for {cid}", self.nano_cell(cid))

    def test_rerun_is_idempotent_and_never_clobbers_a_filled_cell(self):
        self.write_results([{"id": self.ids[0], "parse": {"title": "first"}}])
        self.run_script()
        self.write_results([{"id": self.ids[0], "parse": {"title": "second"}}])
        code, out, err = self.run_script()
        self.assertIn("first", self.nano_cell(self.ids[0]))
        self.assertIn("cells filled this run: 0", out)

    def test_totals_footer_row_is_not_mistaken_for_a_capture(self):
        self.write_results([{"id": cid, "parse": {"title": "t"}} for cid in self.ids])
        self.run_script()
        with open(self.scoring, encoding="utf-8") as fh:
            footer = [l for l in fh if l.startswith("| nano |")]
        self.assertTrue(footer, "the totals footer should still be there")
        self.assertIn("_TODO_", footer[0])

    # --- code fences: the 2026-08-08 phone run's actual failure mode ----------

    def test_whole_output_fence_is_unwrapped_scored_and_still_marked(self):
        a, b = self.ids[:2]
        self.write_results([
            {"id": a, "error": merge_nano.NOT_JSON,
             "raw_output": '```json\n{"title": "Water the plants"}\n```'},
            {"id": b, "error": merge_nano.NOT_JSON,
             "raw_output": '```\n{"title": "Bare fence"}\n```'},
        ])
        code, out, err = self.run_script()

        # The parse is recovered...
        self.assertIn("Water the plants", self.nano_cell(a))
        self.assertIn("Bare fence", self.nano_cell(b))
        # ...but the envelope failure is never hidden.
        self.assertIn("**FENCED:**", self.nano_cell(a))
        self.assertIn("**FENCED:**", self.nano_cell(b))
        self.assertIn("arrived wrapped in a code fence: 2", out)
        self.assertIn("code fence", err)
        self.assertEqual(1, code, "fencing is still a reported problem")

    def test_a_fenced_parse_that_misses_the_schema_is_both_fenced_and_invalid(self):
        a = self.ids[0]
        self.write_results([
            {"id": a, "error": merge_nano.NOT_JSON,
             "raw_output": '```json\n{"title": "x", "label": {"context": "@spaceship"}}\n```'},
        ])
        code, out, err = self.run_script()
        cell = self.nano_cell(a)
        self.assertIn("**FENCED:**", cell)
        self.assertIn("**INVALID:**", cell)
        self.assertIn("@spaceship", err)

    def test_prose_around_the_json_is_never_unwrapped(self):
        a, b, c = self.ids[:3]
        self.write_results([
            # Prose outside the fence — unwrapping would be repairing content.
            {"id": a, "error": merge_nano.NOT_JSON,
             "raw_output": 'Sure! Here you go:\n```json\n{"title": "nope"}\n```'},
            {"id": b, "error": merge_nano.NOT_JSON,
             "raw_output": 'Here is the JSON: {"title": "nope"}'},
            # A fence around something that isn't a JSON object.
            {"id": c, "error": merge_nano.NOT_JSON, "raw_output": '```json\n[1, 2, 3]\n```'},
        ])
        code, out, err = self.run_script()
        for cid in (a, b, c):
            self.assertIn("**ERR:**", self.nano_cell(cid))
            self.assertNotIn("**FENCED:**", self.nano_cell(cid))
        self.assertIn("arrived wrapped in a code fence: 0", out)

    def test_api_errors_are_never_confused_with_fenced_output(self):
        a = self.ids[0]
        self.write_results([
            {"id": a, "error": "com.google.mlkit.genai.common.GenAiException code=9: busy",
             "raw_output": None},
        ])
        self.run_script()
        self.assertIn("**ERR:**", self.nano_cell(a))
        self.assertNotIn("**FENCED:**", self.nano_cell(a))

    def test_duplicate_ids_are_flagged(self):
        self.write_results([
            {"id": self.ids[0], "parse": {"title": "one"}},
            {"id": self.ids[0], "parse": {"title": "two"}},
        ])
        code, out, err = self.run_script()
        self.assertEqual(1, code)
        self.assertIn("reported twice", err)

    def test_missing_results_file_is_a_clear_error(self):
        code, out, err = self.run_script()
        self.assertEqual(1, code)
        self.assertIn("not found", err)


if __name__ == "__main__":
    unittest.main()
