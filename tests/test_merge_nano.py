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
        self.assertIn("**ERR:**", self.nano_cell(e))
        self.assertIn("**ERR:**", self.nano_cell(f))

        # Errors are reported, not swallowed, and the extra/missing ids are named.
        self.assertIn("not-a-capture", err)
        self.assertIn(self.ids[6], err)          # first id with no result at all
        self.assertIn("@spaceship", err)         # the enum violation

        # The summary on stdout is what gets pasted into the issue comment.
        # 3, not 2: the stray "not-a-capture" row is itself a valid parse -- the
        # count is of what the phone reported, and the mismatch is reported separately.
        self.assertIn("schema-valid parses: 3", out)
        self.assertIn("parses that missed the schema: 2", out)
        self.assertIn("rows the model/API failed outright: 2", out)

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
