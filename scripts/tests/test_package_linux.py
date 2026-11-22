"""Exercise the real packaging shell script with controlled build-tool stand-ins."""
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import struct
import subprocess
import sys
import tarfile
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]
COMMIT, TREE = "a" * 40, "b" * 40
ARCHITECTURES = {"x86_64": 62, "aarch64": 183}

MOCK_TOOL = r'''
import json, os, pathlib, sys
name=pathlib.Path(sys.argv[0]).name
args=sys.argv[1:]
root=pathlib.Path(os.environ["FIXTURE_ROOT"])
mode=os.environ.get("MOCK_FAILURE","")
if name=="cargo":
    if args==["--version"]:
        print("cargo 1.95.0 (test fixture)")
    else:
        assert len(args)==6 and args[:4]==["build","--locked","--release","-p"] and args[-1]=="--message-format=json-render-diagnostics", args
        package=args[4]
        assert package in ("gridloom-server","gridloom-mcp")
        fresh=root/"custom target"/"release"/package
        fresh.parent.mkdir(parents=True,exist_ok=True)
        fresh.write_bytes((root/("mcp-template" if package=="gridloom-mcp" else "fresh-template")).read_bytes())
        fresh.chmod(0o755)
        print(json.dumps({"reason":"compiler-artifact","target":{"name":package,"kind":["bin"]},"executable":str(fresh)}))
        print(json.dumps({"reason":"build-finished","success":True}))
elif name=="git":
    if args==["status","--porcelain","--untracked-files=normal"]:
        counter=root/"status-count"
        count=int(counter.read_text())+1 if counter.exists() else 1
        counter.write_text(str(count))
        if mode=="status-initial" or (mode=="status-final" and count==2):
            print("mock git status failed",file=sys.stderr)
            sys.exit(17)
        if mode=="initial-dirty" and count==1: print(" M source.rs")
    elif args==["rev-parse","--verify","HEAD"]:
        print("a"*40)
    elif args==["rev-parse","HEAD"]:
        print(("c" if mode=="head-change" else "a")*40)
    elif args==["rev-parse","HEAD^{tree}"]:
        counter=root/"tree-count"
        count=int(counter.read_text())+1 if counter.exists() else 1
        counter.write_text(str(count))
        print(("d" if mode=="tree-change" and count==2 else "b")*40)
    else:
        raise AssertionError(args)
elif name=="npm":
    assert args in (["ci"],["run","build"]), args
    if args==["run","build"]:
        target=root/"frontend"/"dist"
        target.mkdir(exist_ok=True)
        (target/"index.html").write_text("fresh frontend")
elif name=="ldd":
    assert 1<=len(args)<=2,args
    for arg in args:
        path=pathlib.Path(arg)
        assert path.read_bytes()==(root/("mcp-template" if path.name=="gridloom-mcp-bin" else "fresh-template")).read_bytes(), "ldd received stale artifact"
    library=root/"native"/"libduckdb.so"
    if os.environ.get("LD_LIBRARY_PATH"):
        candidate=pathlib.Path(os.environ["LD_LIBRARY_PATH"].split(":")[0])/"libduckdb.so"
        if candidate.is_file():library=candidate
    with (root/"ldd-paths").open("a") as log:
        for arg in args:
            print("\tlibduckdb.so => %s (0x1234)"%library)
            log.write(arg+"\n")
elif name in ("rustc","node"):
    assert args==["--version"],args
    print(name+" fixture-version")
else:
    raise AssertionError(name)
'''


@unittest.skipUnless(platform.system() == "Linux" and platform.machine() in ARCHITECTURES, "Native Linux ELF packaging contract")
class PackageShellTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="gridloom-shell-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / "fixture with spaces"
        self.root.mkdir()
        self.output = Path(self.temp.name) / "package output"
        for folder in ("scripts", "frontend/dist", "docs", "docs/third-party", "crates/gridloom-server", "target/release", "native", "bin"):
            (self.root / folder).mkdir(parents=True, exist_ok=True)
        for script in ("package-linux.sh", "package-artifact.py"):
            shutil.copy2(SCRIPTS / script, self.root / "scripts" / script)
        for name in ("README.md", "LICENSE", "CHANGELOG.md", "docs/RELEASE-0.3.md", "docs/VERIFICATION-0.3.md", "docs/MCP.md"):
            (self.root / name).write_text(name + " fixture\n")
        (self.root / "Cargo.lock").write_text('[[package]]\nname = "libduckdb-sys"\nversion = "1.10506.1"\n')
        shutil.copy2(SCRIPTS.parent / "docs/third-party/duckdb-1.5.6.LICENSE", self.root / "docs/third-party/duckdb-1.5.6.LICENSE")
        (self.root / "crates/gridloom-server/Cargo.toml").write_text('[package]\nname = "gridloom-server"\nversion = "0.3.0"\n')
        (self.root / "frontend/dist/index.html").write_text("stale frontend")
        (self.root / "native/libduckdb.so").write_bytes(b"fixture shared library")
        header = bytearray(64)
        header[:9] = b"\x7fELF" + bytes((2, 1, 1, 0, 0))
        struct.pack_into("<HHI", header, 16, 3, ARCHITECTURES[platform.machine()], 1)
        struct.pack_into("<H", header, 52, 64)
        self.fresh = bytes(header) + b"fresh artifact from custom Cargo target"
        self.mcp = bytes(header) + b"fresh MCP artifact from custom Cargo target"
        (self.root / "mcp-template").write_bytes(self.mcp)
        (self.root / "fresh-template").write_bytes(self.fresh)
        (self.root / "target/release/gridloom-server").write_bytes(bytes(header) + b"stale default artifact")
        for tool in ("cargo", "git", "npm", "ldd", "rustc", "node"):
            path = self.root / "bin" / tool
            path.write_text("#!" + sys.executable + "\n" + MOCK_TOOL)
            path.chmod(0o755)

    def run_package(self, failure=""):
        environment = os.environ.copy()
        environment.update(PATH=str(self.root / "bin") + os.pathsep + environment["PATH"], FIXTURE_ROOT=str(self.root), GRIDLOOM_PACKAGE_DIR=str(self.output), MOCK_FAILURE=failure, CARGO_TARGET_DIR=str(self.root / "custom target"))
        environment.pop("LD_PRELOAD", None)
        environment.pop("LD_LIBRARY_PATH", None)
        return subprocess.run(["bash", str(self.root / "scripts/package-linux.sh"), "server"], cwd=self.temp.name, env=environment, capture_output=True, text=True, timeout=30)

    def read_archive(self):
        archives = list(self.output.glob("*.tar.gz"))
        self.assertEqual(len(archives), 1)
        archive = archives[0]
        with tarfile.open(archive, "r:gz") as package:
            contents = {member.name: package.extractfile(member).read() for member in package.getmembers() if member.isfile()}
        return archive, contents

    def test_shell_packages_reported_fresh_artifact_and_exact_identity(self):
        result = self.run_package()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        archive, contents = self.read_archive()
        stem = f"gridloom-0.3.0-linux-{platform.machine()}-server"
        self.assertEqual(archive.name, stem + ".tar.gz")
        self.assertEqual(contents[stem + "/gridloom-server"], self.fresh)
        self.assertEqual(contents[stem + "/gridloom-mcp-bin"], self.mcp)
        self.assertIn(b'exec "$base/gridloom-mcp-bin" "$@"', contents[stem + "/gridloom-mcp"])
        self.assertIn(stem + "/docs/MCP.md", contents)
        self.assertEqual(contents[stem + "/frontend/dist/index.html"], b"fresh frontend")
        info = json.loads(contents[stem + "/docs/build-info.json"])
        self.assertEqual((info["source_commit"], info["source_tree"], info["source_dirty"]), (COMMIT, TREE, False))
        self.assertEqual(info["architecture"], platform.machine())
        self.assertEqual(info["mode"], "server")
        self.assertIs(info["mcp_stdio"], True)
        self.assertEqual(info["duckdb_version"], "1.5.6")
        self.assertIn(b"Stichting DuckDB Foundation", contents[stem + "/docs/DuckDB-LICENSE"])
        self.assertEqual(info["build_profile"], "release")
        self.assertEqual(info["rustc"], "rustc fixture-version")
        checksum = Path(str(archive) + ".sha256").read_text()
        self.assertEqual(checksum, hashlib.sha256(archive.read_bytes()).hexdigest() + "  " + archive.name + "\n")
        for line in contents[stem + "/SHA256SUMS"].decode().splitlines():
            digest, name = line.split("  ", 1)
            self.assertEqual(hashlib.sha256(contents[stem + "/" + name.removeprefix("./")]).hexdigest(), digest)
        self.assertEqual((self.root / "ldd-paths").read_text().splitlines()[0], str(self.root / "custom target/release/gridloom-server"))

    def test_initial_and_final_git_status_failure_abort_without_archive(self):
        for failure in ("status-initial", "status-final"):
            with self.subTest(failure=failure):
                (self.root / "status-count").unlink(missing_ok=True)
                result = self.run_package(failure)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("mock git status failed", result.stderr)
                self.assertFalse(list(self.output.glob("*.tar.gz")))

    def test_head_or_tree_change_during_build_aborts_without_archive(self):
        for failure in ("head-change", "tree-change"):
            with self.subTest(failure=failure):
                (self.root / "tree-count").unlink(missing_ok=True)
                result = self.run_package(failure)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("Source commit/tree changed", result.stderr)
                self.assertFalse(list(self.output.glob("*.tar.gz")))

    def test_initial_dirty_state_remains_dirty_even_when_final_status_is_clean(self):
        result = self.run_package("initial-dirty")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        _, contents = self.read_archive()
        info = json.loads(next(value for name, value in contents.items() if name.endswith("/docs/build-info.json")))
        self.assertTrue(info["source_dirty"])


if __name__ == "__main__":
    unittest.main()
