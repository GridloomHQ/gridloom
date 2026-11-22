#!/usr/bin/env python3
"""Exercise real Gridloom MCP processes with no GUI, HTTP, or model API."""
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import threading


class Client:
    def __init__(self, executable, root, read_only=False, version="2025-11-25"):
        args = [executable, "--workbook-root", str(root)]
        if read_only:
            args += ["--read-only"]
        self.stderr = tempfile.TemporaryFile()
        env = os.environ.copy()
        env.pop("DISPLAY", None)
        env.pop("WAYLAND_DISPLAY", None)
        self.p = subprocess.Popen(args, cwd=root.parent, env=env, stdin=subprocess.PIPE,
                                  stdout=subprocess.PIPE, stderr=self.stderr)
        self.seq = 0
        self.deadline = threading.Timer(20, self.p.kill)
        self.deadline.daemon = True
        self.deadline.start()
        try:
            bad = self.rpc("tools/list")
            assert bad["error"]["code"] == -32002, bad
            init = self.rpc("initialize", {
                "protocolVersion": version, "capabilities": {},
                "clientInfo": {"name": "gridloom-mcp-smoke", "version": "1"},
            })
            assert init["result"]["protocolVersion"] == version, init
            assert init["result"]["capabilities"]["tools"] == {"listChanged": False}, init
            self.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
            assert self.rpc("ping")["result"] == {}
        except BaseException:
            self.__exit__(*sys.exc_info())
            raise

    def __enter__(self):
        return self

    def __exit__(self, type_, value, tb):
        try:
            self.p.stdin.close()
            self.p.wait(timeout=3)
            if not type_:
                assert self.p.returncode == 0, f"MCP process exited {self.p.returncode}"
                assert not self.p.stdout.read(1), "Unexpected messages after requests"
            else:
                self.stderr.seek(0)
                print(self.stderr.read(8192).decode(errors="replace"), file=sys.stderr)
        finally:
            self.deadline.cancel()
            if self.p.poll() is None:
                self.p.kill()
                self.p.wait(timeout=3)
            self.p.stdout.close()
            self.stderr.close()

    def send(self, value):
        self.p.stdin.write(json.dumps(value).encode() + b"\n")
        self.p.stdin.flush()

    def rpc(self, method, params=None):
        self.seq += 1
        message = {"jsonrpc": "2.0", "id": self.seq, "method": method}
        if params is not None:
            message["params"] = params
        self.send(message)
        line = self.p.stdout.readline(1024 * 1024)
        assert line.endswith(b"\n"), f"No complete MCP response to {method}"
        response = json.loads(line)
        assert response["jsonrpc"] == "2.0" and response["id"] == self.seq, response
        return response

    def call(self, tool, error=None, **args):
        response = self.rpc("tools/call", {"name": tool, "arguments": args})
        assert "result" in response, response
        payload = response["result"]
        fallback = payload["content"][0]
        assert fallback["type"] == "text"
        if error:
            assert payload["isError"] and error.lower() in fallback["text"].lower(), response
            return
        assert not payload.get("isError", False), response
        assert json.loads(fallback["text"]) == payload["structuredContent"], response
        return payload["structuredContent"]


def cell(name, kind, value=None):
    return {"cell": name, "input": {"type": kind, **({"value": value} if value is not None else {})}}


def database_metadata(path, change):
    with sqlite3.connect(path) as db:
        payload = json.loads(db.execute("SELECT json FROM workbook").fetchone()[0])
        change(payload)
        db.execute("UPDATE workbook SET json=?", (json.dumps(payload),))


def run(executable):
    with tempfile.TemporaryDirectory(prefix="gridloom-mcp-exact-") as directory:
        area = Path(directory)
        root = area / "workbooks"
        root.mkdir()
        book = root / "forecast.gridloom"
        with Client(executable, root) as c:
            available = {t["name"]: t for t in c.rpc("tools/list")["result"]["tools"]}
            assert set(available) == {
                "list_workbooks", "list_functions", "open_workbook", "create_workbook", "workbook_info",
                "close_workbook", "add_sheet", "create_local_table", "read_cells", "validate_cells",
                "edit_cells", "recalculate", "explain_cell", "save_workbook",
                "review_cell", "list_reviews", "get_review", "compare_review",
                "record_review", "transition_question",
            }
            assert "SUM" in c.call("list_functions")["names"]
            assert "SQLVALUE" not in c.call("list_functions")["names"]
            assert not available["read_cells"]["annotations"]["openWorldHint"]
            assert available["edit_cells"]["inputSchema"]["additionalProperties"] is False
            c.call("open_workbook", error="plain", file="../outside.gridloom")
            c.call("open_workbook", error="plain", file=str(area / "outside.gridloom"))
            c.call("create_workbook", error="plain", file="bad.sqlite")
            assert c.call("list_workbooks") == {"files": [], "truncated": False}

            s = c.call("create_workbook", file=book.name)
            assert s["active_sheet"] == "Sheet1" and s["dirty"] and not book.exists()
            r = s["revision"]
            # Notification cannot run a tool even with a valid action and revision.
            c.send({"jsonrpc": "2.0", "method": "tools/call",
                    "params": {"name": "save_workbook", "arguments": {"revision": r}}})
            c.call("close_workbook", revision=r, error="unsaved")
            c.call("open_workbook", file=book.name, error="unsaved")
            assert not book.exists()
            c.call("edit_cells", sheet="Sheet1", revision=r,
                   edits=[cell("A1", "integer", "7"), cell("Z9999", "integer", "1")], error="ref")
            c.call("edit_cells", sheet="Sheet1", revision=r,
                   edits=[cell("A1", "integer", "7"), cell("A1", "integer", "9")], error="value")
            c.call("edit_cells", sheet="Sheet1", revision=r,
                   edits=[cell("A1", "integer", "7"), cell("B1", "formula", "=(")], error="formula")
            c.call("edit_cells", sheet="Sheet1", revision=r,
                   edits=[cell("A1", "formula", "=B1"), cell("B1", "formula", "=A1")], error="circular")
            assert c.call("read_cells", sheet="Sheet1", range="A1")["cells"][0]["value"]["type"] == "Blank"
            c.call("read_cells", sheet="Sheet1", range="A1:A257", error="256")
            s = c.call("edit_cells", sheet="Sheet1", revision=r, edits=[
                cell("A1", "decimal", "100.25"), cell("B1", "decimal", "40.10"),
                cell("C1", "formula", "=A1-B1"), cell("D1", "text", "=not a formula"),
                cell("E1", "integer", "9007199254740993"), cell("F1", "formula", "=1/0"),
                cell("G1", "text", ""), cell("H1", "null"),
            ])
            c.call("save_workbook", error="stale revision", revision=r)
            r = s["revision"]
            assert not book.exists()
            view = c.call("read_cells", sheet="Sheet1", range="A1:H1")
            row = {x["cell"]: x["value"] for x in view["cells"]}
            assert row["C1"] == {"type": "Decimal", "value": "60.15"}, row
            assert row["D1"] == {"type": "Text", "value": "=not a formula"}, row
            assert row["E1"] == {"type": "Integer", "value": "9007199254740993"}, row
            assert row["G1"]["type"] == "EmptyText" and row["H1"]["type"] == "Null", row
            invalid = c.call("validate_cells", sheet="Sheet1", range="A1:H1")
            assert not invalid["valid"] and [x["cell"] for x in invalid["errors"]] == ["F1"]
            trace = c.call("explain_cell", sheet="Sheet1", cell="C1")
            assert trace["formula"] == "=A1-B1" and {x["range"] for x in trace["upstream"]} == {"A1", "B1"}, trace
            assert [x["range"] for x in c.call("explain_cell", sheet="Sheet1", cell="A1")["downstream"]] == ["C1"]
            # Global validation after a deliberate workbook error, then corrected.
            r = c.call("edit_cells", sheet="Sheet1", revision=r, edits=[cell("F1", "blank")])["revision"]
            r = c.call("recalculate", revision=r)["revision"]
            assert c.call("validate_cells", sheet="Sheet1", range="A1:H1")["valid"]
            r = c.call("edit_cells", sheet="Sheet1", revision=r, edits=[
                cell("J1", "formula", "=SEQUENCE(2,2)"),
            ])["revision"]
            spilled = c.call("read_cells", sheet="Sheet1", range="J1:K2")["cells"]
            assert spilled[0]["value"]["rows"] == spilled[0]["value"]["columns"] == 2, spilled
            assert spilled[3]["spill"] and spilled[3]["value"]["value"] == "4", spilled
            assert c.call("explain_cell", sheet="Sheet1", cell="J1")["value"]["rows"] == 2
            r = c.call("add_sheet", sheet="Assumptions", revision=r)["revision"]
            r = c.call("create_local_table", sheet="Assumptions", name="Rates", revision=r,
                       range="A1:B3", columns=[{"name": "Name", "kind": "Text"},
                                               {"name":"Rate", "kind":"Decimal"}])["revision"]
            c.call("edit_cells", sheet="Assumptions", revision=r,
                   edits=[cell("A2", "text", "USD"), cell("B2", "text", "not numeric")],
                   error="Decimal literal")
            assert c.call("read_cells", sheet="Assumptions", range="A2")["cells"][0]["value"]["type"] == "Blank"
            r = c.call("edit_cells", sheet="Assumptions", revision=r,
                       edits=[cell("A2", "text", "USD"), cell("B2", "decimal", "0.125")])["revision"]
            r = c.call("save_workbook", revision=r)["revision"]
            assert book.read_bytes()[:16] == b"SQLite format 3\x00"
            c.call("close_workbook", revision=r)

        with Client(executable, root, read_only=True, version="2025-06-18") as c:
            available = {t["name"] for t in c.rpc("tools/list")["result"]["tools"]}
            assert "save_workbook" not in available and "edit_cells" not in available
            assert c.rpc("tools/call", {"name": "save_workbook"})["error"]["code"] == -32602
            s = c.call("open_workbook", file=book.name)
            assert s["read_only"] and s["sheet_count"] == 2
            local = c.call("read_cells", sheet="Assumptions", range="A1:B2")["cells"]
            assert [x["value"]["value"] for x in local] == ["Name", "Rate", "USD", "0.125"], local
            row = {x["cell"]: x["value"] for x in c.call("read_cells", sheet="Sheet1", range="C1:H1")["cells"]}
            assert row["C1"]["value"] == "60.15" and row["D1"]["value"] == "=not a formula"
            assert row["E1"]["value"] == "9007199254740993"
            c.call("close_workbook", revision=s["revision"])

        with Client(executable, root) as one, Client(executable, root) as two:
            r1 = one.call("open_workbook", file=book.name)["revision"]
            r2 = two.call("open_workbook", file=book.name)["revision"]
            r1 = one.call("edit_cells", sheet="Sheet1", revision=r1, edits=[cell("A1", "decimal", "110.25")])["revision"]
            r2 = two.call("edit_cells", sheet="Sheet1", revision=r2, edits=[cell("A1", "decimal", "120.25")])["revision"]
            one.call("save_workbook", revision=r1)
            before_conflict = book.read_bytes()
            two.call("save_workbook", revision=r2, error="changed on disk")
            assert book.read_bytes() == before_conflict
            save = two.call("save_workbook", revision=r2, save_as="copy.gridloom")
            assert save["file"] == "copy.gridloom" and not save["dirty"]
            assert book.read_bytes() == before_conflict
            r = one.call("create_workbook", file="appeared.gridloom")["revision"]
            appeared = root / "appeared.gridloom"
            appeared.write_text("existing file")
            one.call("save_workbook", revision=r, error="already exists")
            assert appeared.read_text() == "existing file"
            one.call("close_workbook", revision=r, discard_changes=True)

        outside_sql = area / "external.sqlite"
        with sqlite3.connect(outside_sql) as db:
            db.execute("CREATE TABLE fixture (hidden TEXT)")
            db.execute("INSERT INTO fixture VALUES ('private must not load')")
        linked = root / "linked.gridloom"
        shutil.copyfile(book, linked)
        database_metadata(linked, lambda doc: doc["connection_configs"].update({
            "external": {"kind": "Sqlite", "name": "external", "path": str(outside_sql)},
        }))
        outside_book = area / "outside.gridloom"
        shutil.copyfile(book, outside_book)
        (root / "symlink.gridloom").symlink_to(outside_book)
        os.link(outside_book, root / "hardlink.gridloom")
        with Client(executable, root) as c:
            assert c.call("list_workbooks")["files"] == [
                "appeared.gridloom", "copy.gridloom", "forecast.gridloom", "linked.gridloom",
            ]
            c.call("open_workbook", file="symlink.gridloom", error="not a link")
            c.call("open_workbook", file="hardlink.gridloom", error="hard-linked")
            c.call("open_workbook", file="linked.gridloom", error="linked database workbooks")
            c.call("open_workbook", file="appeared.gridloom", error="corrupt workbook")

    print("PASS: stdio MCP 2025-11-25 / 2025-06-18 handshake, exact workbook calculations, safeguards, and reopen.")


if __name__ == "__main__":
    run(str(Path(sys.argv[1]).resolve()))
