#!/usr/bin/env python3
"""Verify the Windows x64 package after extraction under a different path."""
import argparse
import ctypes
from ctypes import wintypes
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import sys
import tempfile
import time
from urllib.request import urlopen
from zipfile import ZipFile


def helper(file):
    spec = importlib.util.spec_from_file_location(file, Path(__file__).with_name(file + ".py"))
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def visible_window(pid):
    user32 = ctypes.WinDLL("user32", use_last_error=True)
    callback_type = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    user32.EnumWindows.argtypes = [callback_type, wintypes.LPARAM]
    user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
    user32.IsWindowVisible.argtypes = [wintypes.HWND]
    user32.GetWindowTextW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
    visible = []

    @callback_type
    def visit(window, _):
        owner = wintypes.DWORD()
        user32.GetWindowThreadProcessId(window, ctypes.byref(owner))
        if owner.value == pid and user32.IsWindowVisible(window):
            title = ctypes.create_unicode_buffer(120)
            user32.GetWindowTextW(window, title, len(title))
            if title.value.startswith("Tayriva"):
                visible.append(window)
        return True

    assert user32.EnumWindows(visit, 0), ctypes.get_last_error()
    return next(iter(visible), None)


def native_smoke(executable, cwd):
    # Test the normal launch, not the --server escape hatch or a Linux DISPLAY.
    env = os.environ.copy()
    for name in ("DISPLAY", "WAYLAND_DISPLAY", "GRIDLOOM_FRONTEND_DIR", "DUCKDB_LIB_DIR", "DUCKDB_INCLUDE_DIR"):
        env.pop(name, None)
    env["GRIDLOOM_RECOVERY_DIR"] = str(cwd / "native recovery")
    log = cwd / "native stdout.log"
    with log.open("wb") as output, subprocess.Popen(
        [str(executable)], cwd=cwd, env=env, stdout=output, stderr=subprocess.STDOUT,
    ) as process:
        window = None
        try:
            deadline = time.monotonic() + 45
            while time.monotonic() < deadline and process.poll() is None:
                data = log.read_bytes()
                found = re.search(rb"Core Engine active at (http://127\.0\.0\.1:\d+)", data)
                if (found and b"Tayriva native webview initialized" in data
                        and b"Tayriva desktop window opened." in data
                        and (window := visible_window(process.pid))):
                    base = found[1].decode()
                    with urlopen(base, timeout=8) as request:
                        index = request.read().decode()
                    asset = re.search(r'src="(/*assets/[^"]+\.js)"', index)
                    assert asset, "Packaged HTML did not refer to the frontend bundle"
                    with urlopen(base + "/" + asset[1].lstrip("/"), timeout=8) as request:
                        assert len(request.read()) > 2000, "Packaged frontend bundle was unavailable"
                    return
                time.sleep(.1)
            raise AssertionError(f"Native WebView2 launch did not produce a visible window ({process.poll()}).")
        finally:
            if window and process.poll() is None:
                close = ctypes.WinDLL("user32").PostMessageW
                close.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
                close(window, 0x0010, 0, 0)  # WM_CLOSE
            if process.poll() is None:
                try:
                    process.wait(timeout=5 if window else 0)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=10)
            print(log.read_text(encoding="utf-8", errors="replace"))


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("archive", type=Path)
    p.add_argument("--expected-commit", required=True)
    p.add_argument("--expected-tree", required=True)
    args = p.parse_args()
    if sys.platform != "win32":
        p.error("A real Windows host is required for package smoke.")
    package = helper("package-windows")
    assert args.archive.with_suffix(".zip.sha256").read_text(encoding="ascii").split() == [
        package.digest(args.archive), args.archive.name,
    ]
    with tempfile.TemporaryDirectory(prefix="Tayriva Windows smoke ") as directory:
        area = Path(directory)
        stage = area / "Relocated release"
        work = area / "Unrelated launch directory"
        work.mkdir()
        with ZipFile(args.archive) as zipped:
            names = zipped.namelist()
            root = args.archive.stem
            assert names and len({n.casefold() for n in names}) == len(names)
            for name in names:
                parts = PurePosixPath(name).parts
                assert (len(parts) >= 2 and parts[0] == root and ".." not in parts
                        and not any("\\" in x or ":" in x for x in parts)), name
            zipped.extractall(stage)
        unpacked = stage / root
        build = json.loads((unpacked / "docs/build-info.json").read_text(encoding="utf-8"))
        assert build["source_commit"] == args.expected_commit
        assert build["source_tree"] == args.expected_tree and build["source_dirty"] is False
        assert build["target"] == package.TARGET
        assert all(build[key] is True for key in ("mcp_review", "mcp_stdio", "guarded_save"))
        for filename in package.FILES:
            package.ensure_x64(unpacked / filename)
        assert (unpacked / "docs/DuckDB-LICENSE").stat().st_size > 0
        desktop, mcp = (str(unpacked / name) for name in package.FILES[:2])
        # The build's DuckDB directory must not rescue a wrong or absent bundled DLL.
        native_dir = os.environ.pop("DUCKDB_LIB_DIR", None)
        os.environ.pop("DUCKDB_INCLUDE_DIR", None)
        if native_dir:
            os.environ["PATH"] = os.pathsep.join(
                part for part in os.environ["PATH"].split(os.pathsep)
                if os.path.normcase(part) != os.path.normcase(native_dir)
            )
        os.environ.pop("GRIDLOOM_FRONTEND_DIR", None)
        native_smoke(Path(desktop), work)
        helper("mcp-smoke").run(mcp)
        helper("mixed-save-smoke").run(desktop, mcp, True)
        helper("review-mcp-smoke").run(desktop, mcp, True)
    print("PASS: exact archived Windows x64 executables/DLL, relocated visible WebView2, frontend, MCP, saves, and review.")


if __name__ == "__main__":
    main()
