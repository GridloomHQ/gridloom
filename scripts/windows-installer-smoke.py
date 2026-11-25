#!/usr/bin/env python3
"""Verify a real per-user setup, reinstall, GUI launch and safe uninstall."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import winreg

from importlib.util import module_from_spec, spec_from_file_location


def helper(name):
    spec = spec_from_file_location(name, Path(__file__).with_name(name + ".py"))
    mod = module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def digest(f):
    with f.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("setup", type=Path)
    p.add_argument("payload", type=Path)
    p.add_argument("logs", type=Path)
    a = p.parse_args()
    assert os.environ.get("GITHUB_ACTIONS") == "true", "Run in a disposable Windows CI account"
    a.logs.mkdir(parents=True, exist_ok=True)
    log = a.logs.resolve()
    common = ["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"]
    setup = a.setup.resolve()
    payload = a.payload.resolve()
    local = Path(os.environ["LOCALAPPDATA"])
    app = local / "Programs" / "Tayriva"
    recovery = local / "tayriva" / "recovery" / "installer-smoke.tay"
    shortcut = Path(os.environ["APPDATA"]) / "Microsoft/Windows/Start Menu/Programs/Tayriva.lnk"
    reg = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{663D2FE2-8DF9-4DDA-A7F5-607083D786B3}_is1"
    expected = json.loads((payload / "docs/build-info.json").read_text(encoding="utf-8"))
    mcp = helper("mcp-smoke")
    native = helper("windows-package-smoke")
    assert not app.exists() and not recovery.exists() and not shortcut.exists()

    def install(action):
        subprocess.run([setup, *common, "/SP-", "/LANG=english", f"/LOG={log / (action + '.log')}"],
                       check=True, timeout=100)
        assert shortcut.is_file()
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, reg) as key:
            assert winreg.QueryValueEx(key, "DisplayVersion")[0] == expected["version"]
            raw = winreg.QueryValueEx(key, "UninstallString")[0]
            assert Path(raw.strip('"')).parent == app, raw
        for file in payload.rglob("*"):
            if file.is_file():
                assert digest(app / file.relative_to(payload)) == digest(file), file

    with tempfile.TemporaryDirectory(prefix="Tayriva Installer ç ") as temp:
        cwd = Path(temp)
        install("install")
        native.native_smoke(app / "tayriva.exe", cwd)
        with mcp.Client(str(app / "tayriva-mcp.exe"), cwd) as c:
            b = c.call("create_workbook", file="installed-smoke.tay")
            b = c.call("edit_cells", sheet="Sheet1", revision=b["revision"],
                       edits=[mcp.cell("A1", "integer", "42")])
            c.call("save_workbook", revision=b["revision"])
        workbook = cwd / "installed-smoke.tay"
        recovery.parent.mkdir(parents=True, exist_ok=True)
        inside = app / "user-created.tay"
        for destination in (inside, recovery):
            shutil.copy2(workbook, destination)
        retained = {f: digest(f) for f in (workbook, inside, recovery)}
        install("reinstall")
        assert {f: digest(f) for f in retained} == retained
        uninstallers = list(app.glob("unins*.exe"))
        assert len(uninstallers) == 1, uninstallers
        subprocess.run([uninstallers[0], *common, f"/LOG={log / 'uninstall.log'}"],
                       check=True, timeout=100)
        assert not shortcut.exists()
        try:
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, reg):
                raise AssertionError("Uninstall registration remains")
        except FileNotFoundError:
            pass
        for file in payload.rglob("*"):
            if file.is_file():
                assert not (app / file.relative_to(payload)).exists(), file
        assert {f: digest(f) for f in retained} == retained
    print("PASS: per-user setup, native WebView2 launch, MCP workbook save, reinstall, and uninstall preserving workbook and recovery bytes.")


if __name__ == "__main__":
    main()
