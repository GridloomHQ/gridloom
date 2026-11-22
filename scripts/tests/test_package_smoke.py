import contextlib
import gzip
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import platform
import tarfile
import tempfile
import unittest
from unittest import mock

SPEC = importlib.util.spec_from_file_location("package_smoke", Path(__file__).parents[1] / "package-smoke.py")
smoke = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(smoke)
COMMIT, TREE = "a" * 40, "b" * 40


def build_info(**overrides):
    info = dict(format="gridloom-linux-package", format_version=1, version="0.3.0", mode="server", source_commit=COMMIT, source_tree=TREE, source_dirty=False, architecture=platform.machine(), system="Linux", build_profile="release")
    info.update(overrides)
    return info


class PackageTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.path = Path(self.temp.name)
        self.serial = 0

    def package(self, *, info=None, files=None, manifest_transform=None, extra_members=(), omit_manifest=False, trailer=b"", omit_files=()):
        self.serial += 1
        archive = self.path / f"package-{self.serial}.tar.gz"
        contents = {
            "gridloom": b"#!/bin/sh\nexit 0\n",
            "gridloom-server": b"fake binary",
            "lib/libduckdb.so": b"fake library",
            "frontend/dist/index.html": b'<html><script type="module" src="/assets/main.js"></script><link rel="stylesheet" href="/assets/main.css"></html>',
            "frontend/dist/assets/main.js": b'console.log("Gridloom")',
            "frontend/dist/assets/main.css": b'body { color: black }',
            "docs/build-info.json": json.dumps(info or build_info()).encode(),
        }
        contents.update(files or {})
        for name in omit_files:
            contents.pop(name, None)
        manifest = "".join(f"{hashlib.sha256(data).hexdigest()}  ./{name}\n" for name, data in contents.items())
        if manifest_transform:
            manifest = manifest_transform(manifest)
        if not omit_manifest:
            contents["SHA256SUMS"] = manifest.encode()
        tar_bytes = io.BytesIO()
        with tarfile.open(fileobj=tar_bytes, mode="w") as bundle:
            root = tarfile.TarInfo("package")
            root.type = tarfile.DIRTYPE
            root.mode = 0o755
            bundle.addfile(root)
            for name, data in contents.items():
                member = tarfile.TarInfo("package/" + name)
                member.size = len(data)
                member.mode = 0o755 if name in ("gridloom", "gridloom-server", "gridloom-desktop", "gridloom-mcp", "gridloom-mcp-bin") else 0o644
                bundle.addfile(member, io.BytesIO(data))
            for member, data in extra_members:
                bundle.addfile(member, io.BytesIO(data) if data is not None else None)
        archive.write_bytes(gzip.compress(tar_bytes.getvalue() + trailer))
        self.checksum(archive)
        return archive

    def checksum(self, archive):
        Path(str(archive) + ".sha256").write_text(f"{smoke.digest(archive)}  {archive.name}\n")

    def verify(self, archive, **kwargs):
        destination = self.path / f"extracted-{len(list(self.path.iterdir()))}"
        destination.mkdir()
        return smoke.verify_package(archive, destination, **kwargs)

    def test_valid_package_complete_identity(self):
        root, info = self.verify(self.package(), expected_commit=COMMIT, expected_tree=TREE)
        self.assertEqual(info["source_commit"], COMMIT)
        self.assertEqual((root / "lib/libduckdb.so").read_bytes(), b"fake library")
        self.assertTrue((root / "gridloom").stat().st_mode & 0o111)

    def test_headless_mcp_package_requires_both_executables(self):
        info = build_info(mcp_stdio=True)
        files = {"gridloom-mcp": b"launcher", "gridloom-mcp-bin": b"binary"}
        for missing in files:
            with self.subTest(missing=missing), self.assertRaisesRegex(smoke.SmokeError, "MCP launcher/binary"):
                self.verify(self.package(info=info, files=files, omit_files=[missing]))
        root, actual = self.verify(self.package(info=info, files=files))
        self.assertTrue(actual["mcp_stdio"])
        self.assertEqual((root / "gridloom-mcp-bin").read_bytes(), b"binary")

    def test_external_checksum_tampering_missing_wrong_filename(self):
        archive = self.package()
        archive.write_bytes(archive.read_bytes() + b"tampered")
        with self.assertRaisesRegex(smoke.SmokeError, "External archive"):
            self.verify(archive)
        self.checksum(archive)
        Path(str(archive) + ".sha256").write_text(f"{smoke.digest(archive)}  wrong.tar.gz\n")
        with self.assertRaises(smoke.SmokeError):
            self.verify(archive)
        Path(str(archive) + ".sha256").unlink()
        with self.assertRaises(OSError):
            self.verify(archive)

    def test_internal_manifest_hash_missing_extra_and_duplicate(self):
        transforms = [
            lambda text: "0" * 64 + text[64:],
            lambda text: "\n".join(text.splitlines()[1:]) + "\n",
            lambda text: text + "0" * 64 + "  ./missing.txt\n",
            lambda text: text + text.splitlines()[0] + "\n",
            lambda text: text + "0" * 64 + "  ./SHA256SUMS\n",
        ]
        for transform in transforms:
            with self.subTest(transform=transform):
                with self.assertRaises(smoke.SmokeError):
                    self.verify(self.package(manifest_transform=transform))
        with self.assertRaises(OSError):
            self.verify(self.package(omit_manifest=True))

    def test_archive_extra_file_not_in_manifest(self):
        member = tarfile.TarInfo("package/extra")
        member.size = 1
        with self.assertRaisesRegex(smoke.SmokeError, "Internal manifest"):
            self.verify(self.package(extra_members=[(member, b"x")]))

    def test_traversal_absolute_backslash_and_control_paths(self):
        for path in ("../escaped", "/tmp/escaped", "package/../../escaped", "package/a/../escaped", "package/../escaped", "package/back\\slash", "package/control\npath"):
            member = tarfile.TarInfo(path)
            member.size = 1
            with self.subTest(path=path), self.assertRaises(smoke.SmokeError):
                self.verify(self.package(extra_members=[(member, b"x")]))
        self.assertFalse((self.path / "escaped").exists())
        for path in ("../escaped", "/tmp/escaped", "a/../escaped"):
            with self.subTest(manifest=path), self.assertRaises(smoke.SmokeError):
                self.verify(self.package(manifest_transform=lambda text: text + "0" * 64 + "  " + path + "\n"))

    def test_links_special_files_duplicate_roots_and_permissions(self):
        for kind in (tarfile.SYMTYPE, tarfile.LNKTYPE, tarfile.FIFOTYPE, tarfile.CHRTYPE, tarfile.BLKTYPE):
            member = tarfile.TarInfo("package/link")
            member.type = kind
            member.linkname = "/tmp/escaped"
            with self.subTest(kind=kind), self.assertRaises(smoke.SmokeError):
                self.verify(self.package(extra_members=[(member, None)]))
        for name, mode in (("package/gridloom", 0o644), ("another/file", 0o644), ("package/suid", 0o4755)):
            member = tarfile.TarInfo(name)
            member.mode = mode
            with self.subTest(name=name), self.assertRaises(smoke.SmokeError):
                self.verify(self.package(extra_members=[(member, b"")]))

    def test_size_and_member_limits_and_nonzero_trailer(self):
        archive = self.package()
        for name, value in (("MAX_ARCHIVE", 1), ("MAX_FILE", 1), ("MAX_MEMBERS", 2), ("MAX_EXPANDED", 10)):
            with self.subTest(limit=name), mock.patch.object(smoke, name, value), self.assertRaises(smoke.SmokeError):
                self.verify(archive)
        with self.assertRaisesRegex(smoke.SmokeError, "Nonzero"):
            self.verify(self.package(trailer=b"hidden payload"))

    def test_build_identity_and_metadata_validation(self):
        for info in (build_info(source_commit="c" * 40), build_info(source_tree="c" * 40), build_info(source_dirty=True)):
            with self.subTest(info=info), self.assertRaises(smoke.SmokeError):
                self.verify(self.package(info=info), expected_commit=COMMIT, expected_tree=TREE)
        for key, value in (("format", "other"), ("format_version", True), ("mode", "other"), ("system", "Darwin"), ("source_dirty", "false"), ("source_tree", "short"), ("build_profile", "debug"), ("version", None), ("architecture", "")):
            with self.subTest(field=key), self.assertRaises(smoke.SmokeError):
                self.verify(self.package(info=build_info(**{key: value})))
        with self.assertRaises(smoke.SmokeError):
            self.verify(self.package(), expected_commit="short")
        self.verify(self.package(info=build_info(source_dirty=True)))

    def test_exact_duckdb_file_required(self):
        # Even a correctly checksummed lookalike library cannot replace the required soname.
        archive = self.package(files={"lib/libduckdb.so.backup": b"fake library"}, omit_files=("lib/libduckdb.so",))
        with self.assertRaisesRegex(smoke.SmokeError, "Bundled DuckDB library missing"):
            self.verify(archive)

    def test_dependency_resolution_rejects_external_missing_and_ambiguous_libraries(self):
        root, _ = self.verify(self.package())
        good = f"libduckdb.so => {root}/lib/libduckdb.so (0x1234)\nlibc.so.6 => /lib/libc.so.6 (0x2345)\n"
        smoke.check_dependency_output(root, good)
        outside = self.path / "libduckdb.so"
        outside.write_bytes(b"external library")
        for output in (good + "libwebkit.so => not found\n", good.replace(str(root / "lib/libduckdb.so"), str(outside)), good + good, "libduckdb.so => not found", "libc.so.6 => /lib/libc.so.6 (0x2345)", good.replace("libduckdb.so =>", "libduckdb.so.1 =>")):
            with self.subTest(output=output), self.assertRaises(smoke.SmokeError):
                smoke.check_dependency_output(root, output)

    def test_dependency_inspection_uses_isolated_environment_and_package_library(self):
        root, info = self.verify(self.package())
        fake = mock.Mock(pid=999999998, returncode=0)
        fake.poll.return_value = 0
        def start(command, **kwargs):
            self.assertEqual(command, ["ldd", str(root / "gridloom-server")])
            self.assertEqual(kwargs["env"]["LD_LIBRARY_PATH"], str(root / "lib"))
            self.assertEqual(kwargs["env"]["LC_ALL"], "C")
            self.assertNotEqual(kwargs["cwd"], str(root))
            kwargs["stdout"].write(f"libduckdb.so => {root}/lib/libduckdb.so (0x1234)\n".encode())
            kwargs["stdout"].flush()
            return fake
        with mock.patch.dict("os.environ", {"LD_LIBRARY_PATH":"/external", "LD_AUDIT":"/audit", "DUCKDB_LIB_DIR":"/external", "BASH_ENV":"/script"}):
            env = smoke.isolated_environment()
            for key in ("LD_LIBRARY_PATH", "LD_AUDIT", "DUCKDB_LIB_DIR", "BASH_ENV"):
                self.assertNotIn(key, env)
        with mock.patch.object(smoke.subprocess, "Popen", side_effect=start), mock.patch.object(smoke, "stop_group") as stop:
            smoke.verify_dependencies(root, info, env)
        stop.assert_called_once_with(fake)

    def test_dependency_inspection_failure_limits_always_cleanup(self):
        root, info = self.verify(self.package())
        for outcome in ("timeout", "oversized", "failed"):
            with self.subTest(outcome=outcome):
                fake = mock.Mock(pid=999999997, returncode=1 if outcome == "failed" else 0)
                fake.poll.return_value = None if outcome == "timeout" else fake.returncode
                def start(*args, **kwargs):
                    if outcome == "oversized":
                        kwargs["stdout"].write(b"x" * (512 * 1024 + 1))
                        kwargs["stdout"].flush()
                    return fake
                with mock.patch.object(smoke.subprocess, "Popen", side_effect=start), mock.patch.object(smoke.time, "monotonic", side_effect=[0, 11]), mock.patch.object(smoke, "stop_group") as stop, self.assertRaises(smoke.SmokeError):
                    smoke.verify_dependencies(root, info, smoke.isolated_environment())
                stop.assert_called_once_with(fake)

    def test_asset_parser_rejects_external_and_escaped_paths(self):
        for path in ("https://evil.invalid/code.js", "//evil.invalid/code.js", "/assets/../private.js", "/assets/%2e%2e/private.js", "/assets/main.js?remote=1"):
            with self.subTest(path=path), self.assertRaises(smoke.SmokeError):
                parser = smoke.Assets()
                parser.feed(f'<script src="{path}"></script>')

    @unittest.skipUnless(platform.system() == "Linux", "process groups are Linux smoke behavior")
    def test_timeout_and_failed_http_always_clean_process_group(self):
        import os
        import subprocess
        for failure in ("timeout", "bad-health"):
            with self.subTest(failure=failure):
                launcher = b"#!/usr/bin/env python3\nimport time\nprint('Gridloom Server running at http://127.0.0.1:12345',flush=True)\ntime.sleep(30)\n"
                root, info = self.verify(self.package(files={"gridloom": launcher}))
                created = []
                original = subprocess.Popen
                def track(*args, **kwargs):
                    process = original(*args, **kwargs)
                    created.append(process)
                    return process
                error = smoke.SmokeError("Health mismatch") if failure == "bad-health" else ConnectionError("not ready")
                with mock.patch.object(smoke, "verify_dependencies"), mock.patch.object(smoke.subprocess, "Popen", side_effect=track), mock.patch.object(smoke, "check_http", side_effect=error), contextlib.redirect_stdout(io.StringIO()), self.assertRaises(smoke.SmokeError):
                    smoke.launch_smoke(root, info, timeout=0.3)
                self.assertEqual(len(created), 1)
                self.assertIsNotNone(created[0].poll())
                with self.assertRaises(ProcessLookupError):
                    os.killpg(created[0].pid, 0)

    def test_native_requires_desktop_and_cannot_pass_without_window_marker(self):
        root, info = self.verify(self.package())
        with self.assertRaisesRegex(smoke.SmokeError, "requires a desktop"):
            smoke.launch_smoke(root, info, native_window=True)
        info = {**info, "mode": "desktop"}
        # Fake only the wrapper process, never claim native execution from this unit test.
        fake = mock.Mock(pid=999999999)
        fake.poll.return_value = None
        with mock.patch.object(smoke, "verify_dependencies"), mock.patch.object(smoke.subprocess, "Popen", return_value=fake) as popen, mock.patch.object(smoke, "stop_group") as stop, mock.patch.object(smoke, "check_http") as http, contextlib.redirect_stdout(io.StringIO()), self.assertRaisesRegex(smoke.SmokeError, "native window-open"):
            smoke.launch_smoke(root, info, native_window=True, timeout=0.05)
        command = popen.call_args.args[0]
        self.assertEqual(command[:5], ["dbus-run-session", "--", "xvfb-run", "-a", "env"])
        self.assertIn("GDK_BACKEND=x11", command)
        self.assertNotIn("--server", command)
        self.assertNotIn("WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS", popen.call_args.kwargs["env"])
        http.assert_not_called()
        stop.assert_called_once_with(fake)

    @unittest.skipUnless(platform.system() == "Linux", "process groups are Linux smoke behavior")
    def test_fake_relocated_process_health_assets_environment_and_cleanup(self):
        launcher = b'''#!/usr/bin/env python3
import http.server,json,os,sys
from pathlib import Path
assert "--port" in sys.argv
assert not any(k.startswith("DUCKDB") for k in os.environ)
assert not any(k in os.environ for k in ("LD_LIBRARY_PATH","GRIDLOOM_FRONTEND_DIR","LD_PRELOAD"))
root=Path(__file__).resolve().parent
assert Path.cwd() != root and root not in Path.cwd().parents
class Handler(http.server.SimpleHTTPRequestHandler):
 def __init__(self,*a,**kw): super().__init__(*a,directory=str(root/"frontend/dist"),**kw)
 def do_GET(self):
  if self.path == "/api/health":
   data=json.dumps({"status":"ok","app":"Gridloom","version":"0.3.0"}).encode(); self.send_response(200); self.end_headers(); self.wfile.write(data)
  else: super().do_GET()
server=http.server.HTTPServer(("127.0.0.1",0),Handler)
print("Gridloom Server running at http://127.0.0.1:%d"%server.server_port,flush=True)
server.serve_forever()
'''
        root, info = self.verify(self.package(files={"gridloom": launcher}))
        with mock.patch.object(smoke, "verify_dependencies"), mock.patch.dict("os.environ", {"DUCKDB_LIB_DIR": "/wrong", "LD_LIBRARY_PATH": "/wrong", "GRIDLOOM_FRONTEND_DIR": "/wrong"}), contextlib.redirect_stdout(io.StringIO()) as output:
            smoke.launch_smoke(root, info, timeout=5)
        self.assertIn("PASS: relocated", output.getvalue())
        port = int(smoke.ENDPOINT.search(output.getvalue()).group(1))
        import socket
        with socket.socket() as probe:
            probe.settimeout(1)
            self.assertNotEqual(probe.connect_ex(("127.0.0.1", port)), 0)


if __name__ == "__main__":
    unittest.main()
