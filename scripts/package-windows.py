#!/usr/bin/env python3
"""Build a portable native Windows x64 ZIP from a clean committed checkout."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tempfile
import tomllib
from zipfile import ZipFile, ZIP_DEFLATED

ROOT = Path(__file__).resolve().parent.parent
FILES = ("tayriva.exe", "tayriva-mcp.exe", "duckdb.dll")
TARGET = "x86_64-pc-windows-msvc"


def command(*args, **opts):
    return subprocess.check_output(args, cwd=ROOT, text=True, **opts).strip()


def identity():
    return command("git", "rev-parse", "HEAD"), command("git", "rev-parse", "HEAD^{tree}")


def ensure_x64(path):
    with path.open("rb") as pe:
        if pe.read(2) != b"MZ":
            raise ValueError(f"{path.name} is not a Windows PE binary")
        pe.seek(0x3C)
        pe.seek(struct.unpack("<I", pe.read(4))[0])
        if pe.read(6) != b"PE\0\0\x64\x86":
            raise ValueError(f"{path.name} is not a Windows x64 binary")


def digest(path):
    with path.open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def main():
    if sys.platform != "win32":
        raise SystemExit("Run the Windows packaging script on a Windows x64 host with MSVC.")
    commit, tree = identity()
    if command("git", "status", "--porcelain"):
        raise SystemExit("Packaging requires a clean Git checkout.")
    lock = tomllib.loads((ROOT / "Cargo.lock").read_text(encoding="utf-8"))["package"]
    locked = next(p["version"] for p in lock if p["name"] == "libduckdb-sys")
    encoded = int(locked.split(".")[1])
    duckdb_version = f"{encoded // 10000}.{encoded // 100 % 100}.{encoded % 100}"
    version = next(p["version"] for p in lock if p["name"] == "gridloom-desktop")
    native = Path(os.environ["DUCKDB_LIB_DIR"]).resolve(strict=True)
    for item in ("duckdb.dll", "duckdb.lib"):
        if not (native / item).is_file():
            raise SystemExit(f"Missing {item} in DUCKDB_LIB_DIR; use official DuckDB {duckdb_version} Windows x64 assets.")
    npm = shutil.which("npm.cmd")
    for args in ([npm, "ci"], [npm, "run", "build"]):
        subprocess.run(args, cwd=ROOT / "frontend", check=True)
    output = Path(os.environ.get("GRIDLOOM_PACKAGE_DIR", ROOT / "target/package"))
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".build-gridloom-", dir=output) as work:
        build = Path(work) / f"tayriva-{version}-windows-x86_64-desktop"
        build.mkdir()
        # Cargo JSON locates the executables even with a custom target directory.
        for crate, filename in zip(("gridloom-desktop", "gridloom-mcp"), FILES):
            result = command("cargo", "build", "--locked", "--release", "--target", TARGET,
                             "-p", crate, "--message-format=json-render-diagnostics")
            binaries = [item["executable"] for item in map(json.loads, result.splitlines())
                        if item.get("reason") == "compiler-artifact"
                        and item.get("executable") and item.get("target", {}).get("name") == crate]
            if len(binaries) != 1:
                raise SystemExit(f"Unexpected build outputs for {crate}: {binaries}")
            shutil.copyfile(binaries[0], build / filename)
        shutil.copyfile(native / "duckdb.dll", build / "duckdb.dll")
        for name in FILES:
            ensure_x64(build / name)
        shutil.copytree(ROOT / "frontend/dist", build / "frontend/dist")
        for name in ("README.md", "LICENSE", "CHANGELOG.md"):
            shutil.copyfile(ROOT / name, build / name)
        docs = build / "docs"
        docs.mkdir()
        for name in ("MCP.md", f"RELEASE-{version}.md"):
            shutil.copyfile(ROOT / "docs" / name, docs / name)
        shutil.copyfile(ROOT / f"docs/third-party/duckdb-{duckdb_version}.LICENSE", docs / "DuckDB-LICENSE")
        if identity() != (commit, tree) or command("git", "status", "--porcelain"):
            raise SystemExit("Source changed during packaging.")
        (docs / "build-info.json").write_text(json.dumps({
            "format": "tayriva-windows-package", "format_version": 1,
            "version": version, "mode": "desktop", "architecture": "x86_64",
            "source_commit": commit, "source_tree": tree, "source_dirty": False,
            "duckdb_version": duckdb_version, "target": TARGET, "mcp_stdio": True,
            "guarded_save": True, "mcp_review": True,
        }, indent=2) + "\n", encoding="utf-8")
        archive = output / f"{build.name}.zip"
        if archive.exists():
            raise SystemExit(f"Will not overwrite {archive}")
        staged = Path(work) / archive.name
        with ZipFile(staged, "w", ZIP_DEFLATED, compresslevel=6) as z:
            for file in sorted(build.rglob("*")):
                if file.is_file():
                    z.write(file, file.relative_to(build.parent).as_posix())
        staged.replace(archive)
        archive.with_suffix(".zip.sha256").write_text(
            f"{digest(archive)}  {archive.name}\n", encoding="ascii")
        print(archive)


if __name__ == "__main__":
    main()
