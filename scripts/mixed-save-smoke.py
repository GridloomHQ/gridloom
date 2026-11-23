#!/usr/bin/env python3
"""Run the actual Tayriva HTTP save handler against an independent headless MCP editor."""
import argparse
from concurrent.futures import ThreadPoolExecutor
from contextlib import contextmanager
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import threading
import time
from urllib.error import HTTPError
from urllib.request import Request, urlopen

spec = importlib.util.spec_from_file_location("mcp_smoke", Path(__file__).with_name("mcp-smoke.py"))
mcp = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mcp)


@contextmanager
def backend(binary, folder, desktop):
    env = {**os.environ, "GRIDLOOM_RECOVERY_DIR": str(folder / "recovery")}
    env.pop("DISPLAY", None)
    env.pop("WAYLAND_DISPLAY", None)
    args = [binary, "--port", "0"]
    if desktop:
        args += ["--server"]
    log_path = folder / "backend.log"
    with log_path.open("wb") as log, subprocess.Popen(
        args, cwd=folder, env=env, stdout=log, stderr=subprocess.STDOUT
    ) as child:
        try:
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline and child.poll() is None:
                found = re.search(rb"(?:Core Engine active|Server running) at (http://127\.0\.0\.1:\d+)", log_path.read_bytes())
                if found:
                    base = found[1].decode()
                    break
                time.sleep(.025)
            else:
                raise AssertionError(f"Tayriva backend failed to start: {log_path.read_text(errors='replace')}")
            def post(route, **data):
                request = Request(base + "/api/" + route, json.dumps(data).encode(), {"Content-Type": "application/json"})
                try:
                    with urlopen(request, timeout=5) as http:
                        return http.status, json.load(http)
                except HTTPError as http:
                    return http.code, http.read().decode()
            yield post
        finally:
            if child.poll() is None:
                child.terminate()
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=5)


def read(client):
    rows = client.call("read_cells", sheet="Sheet1", range="A1:B1")["cells"]
    return [row["value"]["value"] for row in rows]


def inspect(executable, folder, name):
    with mcp.Client(executable, folder, read_only=True) as check:
        check.call("open_workbook", file=name)
        return read(check)


def mcp_save_loss(client, rev):
    result = client.rpc("tools/call", {"name": "save_workbook", "arguments": {"revision": rev}})["result"]
    if result.get("isError", False):
        assert "#CONFLICT!" in result["content"][0]["text"], result
        return False
    assert result["structuredContent"]["dirty"] is False, result
    return True


def scenario(raw_backend, executable, folder, order, desktop):
    folder.mkdir()
    name = "synthetic.tay"
    workbook = folder / name
    with mcp.Client(executable, folder) as seed:
        rev = seed.call("create_workbook", file=name)["revision"]
        rev = seed.call("edit_cells", sheet="Sheet1", revision=rev, edits=[
            mcp.cell("A1", "integer", "41"), mcp.cell("B1", "integer", "0"),
        ])["revision"]
        seed.call("save_workbook", revision=rev)

    with backend(raw_backend, folder, desktop) as post, mcp.Client(executable, folder) as agent:
        assert post("workbook/open", path=str(workbook))[0] == 200
        rev = agent.call("open_workbook", file=name)["revision"]
        rev = agent.call("edit_cells", sheet="Sheet1", revision=rev, edits=[
            mcp.cell("A1", "integer", "42"),
        ])["revision"]
        assert post("cells/set", sheet_name="Sheet1", row=0, col=1, raw_input="99")[0] == 200
        prior = workbook.read_bytes()
        if order == "mcp-first":
            assert mcp_save_loss(agent, rev)
            winner = workbook.read_bytes()
            code, body = post("workbook/save", path=str(workbook))
            assert code == 409 and "Workbook changed on disk" in body and "Save As" in body, body
            assert winner == workbook.read_bytes() != prior
            seq = (True, False)
        elif order == "desktop-first":
            assert post("workbook/save", path=str(workbook))[0] == 200
            winner = workbook.read_bytes()
            agent.call("save_workbook", revision=rev, error="changed on disk")
            assert winner == workbook.read_bytes() != prior
            seq = (False, True)
        else:
            barrier = threading.Barrier(2)
            def go(action):
                barrier.wait(timeout=3)
                return action()
            with ThreadPoolExecutor(2) as pool:
                m = pool.submit(go, lambda: mcp_save_loss(agent, rev))
                d = pool.submit(go, lambda: post("workbook/save", path=str(workbook)))
                status, text = d.result()
                assert status in (200, 409), text
                seq = (m.result(), status == 200)
            assert sum(seq) == 1, seq
            winner = workbook.read_bytes()
            assert winner != prior
        assert inspect(executable, folder, name) == (["42", "0"] if seq[0] else ["41", "99"])

        # The losing editor still has its own unsaved state. A copy must not touch the original.
        copy = folder / "separate.tay"
        if seq[0]:
            assert post("workbook/save", path=str(workbook), create_only=True)[0] == 409
            cells = post("cells/viewport", sheet_name="Sheet1", start_row=0, start_col=0, end_row=0, end_col=1, overscan=0)
            assert cells[1]["cells"]["B1"]["raw_input"] == "99"
            assert post("workbook/save", path=str(copy), create_only=True)[0] == 200
        else:
            assert agent.call("workbook_info")["dirty"]
            assert read(agent) == ["42", "0"]
            agent.call("save_workbook", revision=rev, save_as=copy.name)
        assert workbook.read_bytes() == winner
        assert inspect(executable, folder, copy.name) == (["41", "99"] if seq[0] else ["42", "0"])
        assert not list(folder.glob(".tayriva-save*")), "temporary save leaked"


def run(backend_path, mcp_path, desktop):
    with tempfile.TemporaryDirectory(prefix="gridloom-mixed-writers-") as root:
        for order in ("mcp-first", "desktop-first", "simultaneous"):
            scenario(backend_path, mcp_path, Path(root) / order, order, desktop)
    print("PASS: mixed Tayriva/MCP writer orders, concurrent lock, native HTTP 409, unchanged winner, and separate recovery copy.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("backend", type=Path)
    parser.add_argument("mcp", type=Path)
    parser.add_argument("--desktop", action="store_true")
    args = parser.parse_args()
    run(str(args.backend.resolve()), str(args.mcp.resolve()), args.desktop)
