from pathlib import Path
import sqlite3
import subprocess
import sys
import tempfile
import unittest
import json

SCRIPT = Path(__file__).resolve().parents[1] / "make-ledger-demo.py"


class LedgerDemoTest(unittest.TestCase):
    def test_synthetic_ledger_has_stable_rows_and_reference_total(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "demo.sqlite"
            run = subprocess.run(
                [sys.executable, SCRIPT, path, "--rows", "31"],
                check=True, capture_output=True, text=True,
            )
            result = json.loads(run.stdout)
            self.assertEqual(result["rows"], 31)
            self.assertEqual(result["sample"]["rows"], 4)
            # Account 1100 is assigned to rows 1, 11, 21 and 31.
            self.assertEqual(result["sample"]["total_cents"], str(-10000 + 10010 + 10020 - 10030))
            with sqlite3.connect(path) as db:
                rows = db.execute("SELECT id, posted FROM entries ORDER BY posted, id").fetchall()
            self.assertEqual(rows, [(i, "2026-01-01") for i in range(1, 32)])
            before = path.read_bytes()
            rerun = subprocess.run(
                [sys.executable, SCRIPT, path, "--rows", "30"],
                capture_output=True,
            )
            self.assertNotEqual(rerun.returncode, 0)
            self.assertEqual(path.read_bytes(), before)
