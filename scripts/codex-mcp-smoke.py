#!/usr/bin/env python3
"""Verify a packaged Tayriva MCP launcher using local Codex, without a model turn.

Usage: python3 scripts/codex-mcp-smoke.py /path/to/tayriva-mcp
Requires an installed Codex CLI with the app-server MCP methods. Writes all
temporary Codex config, state and workbooks under an isolated directory.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading


def quote(value):
    # JSON strings are TOML basic strings for these paths and arguments.
    return json.dumps(value, ensure_ascii=False)


def verify(mode, area, executable):
    home = area / "isolated-codex-home"
    root = area / "trusted project"
    outside = area / "unconfigured project"
    workbooks = root / "test workbooks"
    for directory in (home, workbooks, outside):
        directory.mkdir(parents=True)
    env = {k: v for k, v in os.environ.items() if k not in ("OPENAI_API_KEY", "CODEX_API_KEY")}
    env["CODEX_HOME"] = str(home)
    # No login or model token is necessary. The trust applies only to the
    # temporary fixture; no user's Codex files are edited.
    (home / "config.toml").write_text(
        f"[projects.{quote(str(root))}]\ntrust_level = \"trusted\"\n", encoding="utf-8"
    )
    config_before = (home / "config.toml").read_bytes()
    options = {"command": quote(str(executable)), "args": quote(["--workbook-root", str(workbooks)])}
    overrides = []
    if mode == "project":
        (root / ".codex").mkdir()
        (root / ".codex" / "config.toml").write_text(
            "[mcp_servers.tayriva]\n" + "".join(f"{key} = {value}\n" for key, value in options.items()),
            encoding="utf-8",
        )
    else:
        for key, value in options.items():
            overrides += ["-c", f"mcp_servers.tayriva.{key}={value}"]

    def cli_list(cwd, cli_options):
        return json.loads(subprocess.check_output(
            ["codex", *cli_options, "mcp", "list", "--json"], cwd=cwd, env=env,
            text=True, timeout=15,
        ))

    assert not cli_list(outside, []), "The project setup must not affect another working directory"
    assert {entry["name"] for entry in cli_list(root, overrides)} == {"tayriva"}
    if mode == "invocation":
        assert not cli_list(root, []), "A CLI override must not persist"

    with tempfile.TemporaryFile(mode="w+t") as error, subprocess.Popen(
        ["codex", *overrides, "app-server", "--stdio"], cwd=root, env=env,
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=error, text=True, bufsize=1,
    ) as process:
        sequence = 0
        deadline = threading.Timer(35, process.kill)
        deadline.daemon = True
        deadline.start()

        def rpc(method, params=None, notify=False):
            nonlocal sequence
            sequence += 1
            request = {"method": method, "params": params or {}}
            if not notify:
                request["id"] = sequence
            process.stdin.write(json.dumps(request) + "\n")
            process.stdin.flush()
            if notify:
                return None
            for line in process.stdout:
                response = json.loads(line)
                if response.get("id") == sequence and ("result" in response or "error" in response):
                    assert "error" not in response, response
                    return response["result"]
            raise AssertionError(f"Codex app server stopped responding at {method}")

        try:
            rpc("initialize", {"clientInfo": {"name": "tayriva-smoke", "version": "1"},
                               "capabilities": {"experimentalApi": True}})
            rpc("initialized", notify=True)
            session = rpc("thread/start", {"cwd": str(root), "ephemeral": True,
                                           "sandbox": "read-only", "approvalPolicy": "never"})
            tid = session["thread"]["id"]
            inventory = rpc("mcpServerStatus/list", {
                "threadId": tid, "serverName": "tayriva", "detail": "toolsAndAuthOnly",
            })
            assert len(inventory["data"]) == 1, inventory
            assert {"read_cells", "compare_review", "save_workbook"} <= inventory["data"][0]["tools"].keys(), inventory

            def call(tool, arguments):
                response = rpc("mcpServer/tool/call", {"threadId": tid, "server": "tayriva",
                                                       "tool": tool, "arguments": arguments})
                assert not response.get("isError", False), response
                return response["structuredContent"]

            opened = call("create_workbook", {"file": "codex-test"})
            edited = call("edit_cells", {
                "sheet": "Sheet1", "revision": opened["revision"], "edits": [
                    {"cell": "A1", "input": {"type": "formula", "value": "=22.34+0.10"}},
                ],
            })
            cells = call("read_cells", {"sheet": "Sheet1", "range": "A1:B1"})["cells"]
            assert cells[0]["value"] == {"type": "Decimal", "value": "22.44"}, cells
            assert cells[1]["value"] == {"type": "Blank"} and cells[1]["formula"] is None, cells
            assert call("save_workbook", {"revision": edited["revision"]})["file"] == "codex-test.tay"
            assert (workbooks / "codex-test.tay").read_bytes()[68:72] == b"TAYW"
        except BaseException:
            error.seek(0)
            print(error.read(8192), file=sys.stderr)
            raise
        finally:
            deadline.cancel()
            process.terminate()
    assert config_before == (home / "config.toml").read_bytes()
    print(f"PASS: local Codex discovered and called packaged Tayriva MCP ({mode} scope).")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    launcher = Path(sys.argv[1]).resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="tayriva-codex-mcp-") as temporary:
        for mode in ("project", "invocation"):
            verify(mode, Path(temporary) / mode, launcher)
