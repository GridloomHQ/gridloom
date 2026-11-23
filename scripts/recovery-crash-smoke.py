#!/usr/bin/env python3
"""Exercise recovery across a real process kill using isolated workbook and SQLite files."""
import argparse
from contextlib import contextmanager
import json
import os
from pathlib import Path
import re
import sqlite3
import stat
import subprocess
import tempfile
import time
from urllib.request import Request, urlopen
from urllib.error import HTTPError


@contextmanager
def server(binary, workspace, recovery):
    log_path = workspace / "server.log"
    with log_path.open("wb") as log, subprocess.Popen(
        [str(binary), "--port", "0"],
        cwd=workspace,
        env={**os.environ, "GRIDLOOM_RECOVERY_DIR": str(recovery)},
        stdout=log,
        stderr=subprocess.STDOUT,
    ) as process:
        try:
            deadline = time.monotonic() + 15
            while time.monotonic() < deadline and process.poll() is None:
                match = re.search(rb"Server running at (http://127\.0\.0\.1:\d+)", log_path.read_bytes())
                if match:
                    yield process, match[1].decode()
                    return
                time.sleep(0.05)
            raise RuntimeError(f"Server did not start: {log_path.read_text(errors='replace')}")
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)


def api(base, route, payload=None):
    data = None if payload is None else json.dumps(payload).encode()
    request = Request(base + "/api/" + route, data=data, headers={"Content-Type": "application/json"})
    with urlopen(request, timeout=5) as response:
        return json.load(response)


def read_cell(base):
    return api(base, "ranges/read", {
        "sheet_name": "Sheet1",
        "range": {"start_row": 0, "start_col": 0, "end_row": 0, "end_col": 0},
    })


def assert_blank(base):
    workbook = api(base, "workbook")
    assert workbook["file_path"] is None
    assert [sheet["name"] for sheet in workbook["sheets"]] == ["Sheet1"]
    assert workbook["sheets"][0]["tables"] == []
    assert workbook["connections"] == []
    assert workbook["sources"] == []
    assert workbook["staged_patches_count"] == 0
    assert read_cell(base) == [[""]]


def write_cell(base, text):
    api(base, "ranges/edit", {"sheet_name": "Sheet1", "row": 0, "col": 0, "values": [[text]]})


def wait_for_checkpoint(recovery, text, patch_id):
    deadline = time.monotonic() + 8
    while time.monotonic() < deadline:
        for file in recovery.glob("*.tay"):
            snapshot = file.read_bytes()
            if text.encode() in snapshot and patch_id.encode() in snapshot:
                return file
        time.sleep(0.05)
    raise TimeoutError("Background autosave did not checkpoint the workbook")


def smoke(binary, folder):
    recovery = folder / "recovery"
    named = folder / "named.tay"
    exported = folder / "recovered-copy.tay"
    database = folder / "ledger.sqlite"
    with sqlite3.connect(database) as db:
        db.executescript(
            "CREATE TABLE entries (id INTEGER PRIMARY KEY, amount INTEGER);"
            "INSERT INTO entries VALUES (1, 11);"
        )
        original_database = database.read_bytes()
        with server(binary, folder, recovery) as (process, base):
            assert_blank(base)
            assert not (folder / "fixtures").exists(), "Default launch created demo files"
            api(base, "connections", {"name": "Ledger", "kind": "sqlite", "path": str(database)})
            write_cell(base, "explicitly saved")
            api(base, "workbook/save", {"path": str(named)})
            assert api(base, "workbook")["file_path"] == str(named)
            original_workbook = named.read_bytes()

            recovery_text = "recovered after crash"
            write_cell(base, recovery_text)
            patch_id = api(base, "staged", {
                "connection_name": "Ledger", "table_name": "entries", "pk_columns": ["id"],
                "pk_values": [{"type": "Decimal", "value": "1"}], "column_name": "amount",
                "original_value": {"type": "Decimal", "value": "11"},
                "proposed_value": {"type": "Decimal", "value": "99"},
            })["id"]
            checkpoint = wait_for_checkpoint(recovery, recovery_text, patch_id)
            checkpoint_id = checkpoint.stem
            if os.name == "posix":
                assert stat.S_IMODE(recovery.stat().st_mode) & 0o077 == 0
                assert stat.S_IMODE(checkpoint.stat().st_mode) & 0o077 == 0
            assert named.read_bytes() == original_workbook
            assert database.read_bytes() == original_database
            process.kill()
            assert process.wait(timeout=5) != 0

        with server(binary, folder, recovery) as (_, base):
            assert_blank(base)
            assert any(item["id"] == checkpoint_id for item in api(base, "workbook/recovery"))
            api(base, "workbook/recovery/" + checkpoint_id, {})
            assert read_cell(base) == [["recovered after crash"]]
            assert api(base, "workbook")["file_path"] is None
            assert [patch["id"] for patch in api(base, "staged")] == [patch_id]
            try:
                api(base, "workbook/save", {"path": str(named)})
                raise AssertionError("Restored recovery unexpectedly replaced the named workbook")
            except HTTPError as error:
                assert error.code == 409, error
                assert "already exists" in error.read().decode(), error
            assert named.read_bytes() == original_workbook
            assert read_cell(base) == [["recovered after crash"]]
            api(base, "workbook/save", {"path": str(exported)})
            assert api(base, "workbook")["file_path"] == str(exported)
            api(base, "workbook/open", {"path": str(exported)})
            assert read_cell(base) == [["recovered after crash"]]
            assert [patch["id"] for patch in api(base, "staged")] == [patch_id]
            api(base, "workbook/open", {"path": str(named)})
            assert read_cell(base) == [["explicitly saved"]]
            assert not api(base, "staged")
            assert api(base, "workbook")["connections"] == ["Ledger"]
            api(base, "workbook/new", {})
            assert_blank(base)
            assert any(item["id"] == checkpoint_id for item in api(base, "workbook/recovery"))
            assert named.read_bytes() == original_workbook
            assert database.read_bytes() == original_database
            assert db.execute("SELECT amount FROM entries WHERE id=1").fetchone() == (11,)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server_binary", type=Path)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="gridloom-recovery-smoke-") as root:
        smoke(args.server_binary.resolve(strict=True), Path(root))
    print("PASS: blank launch and File/New, forced termination, private recovery, Save As, reopening, and unchanged SQLite source")
