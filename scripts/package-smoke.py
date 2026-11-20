#!/usr/bin/env python3
"""Verify and smoke a relocated Linux package. Checksums are not signatures.

Only CI/user-approved packages should be executed. Native mode proves window-open,
not interactive native behavior. All validation finishes before any code is run.
"""
import argparse
import gzip
import hashlib
from html.parser import HTMLParser
import io
import json
import os
from pathlib import Path
import platform
import re
import signal
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request

MAX_ARCHIVE = 512 * 1024 * 1024
MAX_EXPANDED = 2 * 1024 * 1024 * 1024
MAX_FILE = 1024 * 1024 * 1024
MAX_MEMBERS = 20000
MAX_METADATA = 4 * 1024 * 1024
MAX_LOG = 4 * 1024 * 1024
HASH = re.compile(r"^[a-fA-F0-9]{64}$")
IDENTITY = re.compile(r"^[a-fA-F0-9]{40}(?:[a-fA-F0-9]{24})?$")
ENDPOINT = re.compile(r"(?:Gridloom Server running at|Gridloom Core Engine active at) http://127\.0\.0\.1:([0-9]{1,5})")
WINDOW_MARKER = "Gridloom desktop window opened."


class SmokeError(Exception):
    pass


def digest(path):
    result = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(chunk)
    return result.hexdigest()


def checksum_lines(text):
    entries = {}
    for line in text.splitlines():
        match = re.fullmatch(r"([a-fA-F0-9]{64}) [ *](.+)", line)
        if not match:
            raise SmokeError("Invalid SHA-256 manifest line")
        checksum, name = match.groups()
        if name in entries:
            raise SmokeError("Duplicate checksum entry")
        entries[name] = checksum.lower()
    if not entries:
        raise SmokeError("Empty checksum manifest")
    return entries


def safe_parts(name):
    if name.startswith("./"):
        name = name[2:]
    name = name.rstrip("/")
    if not name or len(name) > 4096 or "\\" in name or any(ord(c) < 32 or ord(c) == 127 for c in name):
        raise SmokeError("Unsafe archive/manifest path")
    parts = name.split("/")
    if any(p in ("", ".", "..") or len(p) > 255 for p in parts):
        raise SmokeError("Unsafe archive/manifest path")
    return parts


class LimitedReader(io.RawIOBase):
    """Bound gzip expansion including tar padding and extended/PAX headers."""
    def __init__(self, source, limit):
        self.source, self.remaining = source, limit

    def readable(self):
        return True

    def read(self, size=-1):
        if size < 0:
            size = self.remaining + 1
        data = self.source.read(min(size, self.remaining + 1))
        self.remaining -= len(data)
        if self.remaining < 0:
            raise SmokeError("Archive expanded-size limit exceeded")
        return data


def read_small(path, limit=MAX_METADATA):
    if path.stat().st_size > limit:
        raise SmokeError(f"Oversized metadata: {path.name}")
    return path.read_text(encoding="utf-8")


def validate_build_info(root, expected_commit=None, expected_tree=None):
    try:
        info = json.loads(read_small(root / "docs/build-info.json"))
    except (OSError, ValueError) as error:
        raise SmokeError(f"Invalid build-info.json: {error}") from error
    if not isinstance(info, dict) or info.get("format") != "gridloom-linux-package" or type(info.get("format_version")) is not int or info["format_version"] != 1:
        raise SmokeError("Unsupported package build-info format")
    if info.get("mode") not in ("server", "desktop") or info.get("build_profile") != "release" or info.get("system") != "Linux":
        raise SmokeError("Expected a Linux release server/desktop package")
    for key in ("version", "architecture"):
        if not isinstance(info.get(key), str) or not info[key].strip() or len(info[key]) > 100 or any(ord(c) < 32 for c in info[key]):
            raise SmokeError(f"Invalid build metadata field: {key}")
    for key in ("source_commit", "source_tree"):
        if not isinstance(info.get(key), str) or not IDENTITY.fullmatch(info[key]):
            raise SmokeError(f"Invalid build metadata field: {key}")
    if type(info.get("source_dirty")) is not bool:
        raise SmokeError("source_dirty must be a boolean")
    for expected, field in ((expected_commit, "source_commit"), (expected_tree, "source_tree")):
        if expected is not None:
            if not IDENTITY.fullmatch(expected):
                raise SmokeError(f"Expected {field} must be a full Git object ID")
            if info[field].lower() != expected.lower():
                raise SmokeError(f"Package {field} does not match expected identity")
    if (expected_commit is not None or expected_tree is not None) and info["source_dirty"]:
        raise SmokeError("Dirty package cannot satisfy exact source identity")
    return info


def verify_package(archive, destination, expected_commit=None, expected_tree=None):
    """Safely extract into an empty owned directory, then verify every regular file."""
    archive, destination = Path(archive), Path(destination)
    if archive.stat().st_size > MAX_ARCHIVE:
        raise SmokeError("Compressed archive size limit exceeded")
    checksum = checksum_lines(read_small(Path(str(archive) + ".sha256")))
    if set(checksum) != {archive.name} or checksum[archive.name] != digest(archive):
        raise SmokeError("External archive checksum mismatch or wrong filename")
    if not destination.is_dir() or any(destination.iterdir()):
        raise SmokeError("Extraction destination must be an empty directory")
    seen, files, root_name = set(), {}, None
    total, count = 0, 0
    try:
        with gzip.open(archive, "rb") as compressed:
            bounded = LimitedReader(compressed, MAX_EXPANDED + MAX_METADATA)
            with tarfile.open(fileobj=bounded, mode="r|") as bundle:
                for member in bundle:
                    count += 1
                    if count > MAX_MEMBERS:
                        raise SmokeError("Archive member-count limit exceeded")
                    parts = safe_parts(member.name)
                    if root_name is None:
                        root_name = parts[0]
                    if parts[0] != root_name:
                        raise SmokeError("Package must have one root directory")
                    key = "/".join(parts)
                    if key in seen:
                        raise SmokeError("Duplicate archive path")
                    seen.add(key)
                    if member.type not in (tarfile.REGTYPE, tarfile.AREGTYPE, tarfile.DIRTYPE):
                        raise SmokeError("Links and special archive files are forbidden")
                    if member.mode & 0o7000 or member.size < 0 or member.size > MAX_FILE:
                        raise SmokeError("Unsafe file mode or oversized archive member")
                    if len(parts) == 1 and not member.isdir():
                        raise SmokeError("Archive root must be a directory")
                    total += member.size
                    if total > MAX_EXPANDED:
                        raise SmokeError("Archive content-size limit exceeded")
                    target = destination.joinpath(*parts)
                    if member.isdir():
                        target.mkdir(parents=True, exist_ok=True)
                    else:
                        target.parent.mkdir(parents=True, exist_ok=True)
                        source = bundle.extractfile(member)
                        if source is None:
                            raise SmokeError("Missing archive file data")
                        hasher, written = hashlib.sha256(), 0
                        with source, target.open("xb") as output:
                            while chunk := source.read(1024 * 1024):
                                written += len(chunk)
                                if written > member.size:
                                    raise SmokeError("Archive member exceeded declared length")
                                output.write(chunk)
                                hasher.update(chunk)
                        if written != member.size:
                            raise SmokeError("Truncated archive member")
                        target.chmod(0o755 if member.mode & 0o111 else 0o644)
                        files["/".join(parts[1:])] = hasher.hexdigest()
                # tarfile stops at its first end marker. Only zero padding may follow;
                # never silently ignore a second archive or unmanifested hidden payload.
                while tail := bundle.fileobj.read(1024 * 1024):
                    if any(tail):
                        raise SmokeError("Nonzero data follows the tar end marker")
            # Force gzip CRC validation, including concatenated gzip members.
            while tail := bounded.read(1024 * 1024):
                if any(tail):
                    raise SmokeError("Nonzero data follows the archive")
    except (tarfile.TarError, OSError, EOFError) as error:
        raise SmokeError(f"Invalid archive: {error}") from error
    if root_name is None:
        raise SmokeError("Empty archive")
    root = destination / root_name
    entries = checksum_lines(read_small(root / "SHA256SUMS"))
    manifest = {}
    for name, checksum in entries.items():
        path = "/".join(safe_parts(name))
        if path in manifest or path == "SHA256SUMS":
            raise SmokeError("Duplicate/self-referential internal manifest path")
        manifest[path] = checksum
    expected = {name: checksum for name, checksum in files.items() if name != "SHA256SUMS"}
    if manifest != expected:
        raise SmokeError("Internal manifest mismatch: missing, extra or changed file")
    info = validate_build_info(root, expected_commit, expected_tree)
    for required in ("gridloom", f"gridloom-{info['mode']}", "frontend/dist/index.html"):
        if required not in files:
            raise SmokeError(f"Required package file missing: {required}")
    if not os.access(root / "gridloom", os.X_OK) or not os.access(root / f"gridloom-{info['mode']}", os.X_OK):
        raise SmokeError("Package launcher/binary is not executable")
    if "lib/libduckdb.so" not in files:
        raise SmokeError("Bundled DuckDB library missing")
    return root, info


class Assets(HTMLParser):
    def __init__(self):
        super().__init__()
        self.paths = []
        self.scripts = 0

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        value = attrs.get("src") if tag == "script" else attrs.get("href") if tag == "link" and attrs.get("rel") in ("stylesheet", "modulepreload") else None
        if value is not None:
            parts = urllib.parse.urlsplit(value)
            if parts.scheme or parts.netloc or parts.query or parts.fragment or not parts.path.startswith("/assets/") or urllib.parse.unquote(parts.path) != parts.path:
                raise SmokeError("Frontend code/style assets must be plain local /assets/ paths")
            safe_parts(parts.path[1:])
            self.paths.append(parts.path)
            if tag == "script":
                self.scripts += 1


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise SmokeError("Unexpected HTTP redirect during smoke")


def http_bytes(opener, url, limit, timeout):
    with opener.open(url, timeout=timeout) as response:
        if response.status != 200:
            raise SmokeError(f"Unexpected HTTP status {response.status}")
        data = response.read(limit + 1)
        if len(data) > limit:
            raise SmokeError("HTTP response exceeds smoke size limit")
        return data


def check_http(root, info, port, deadline):
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
    base = f"http://127.0.0.1:{port}"
    def read(path, limit):
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise SmokeError("Package smoke timeout")
        return http_bytes(opener, base + path, limit, min(2, remaining))
    health = json.loads(read("/api/health", 65536))
    if health != {"status": "ok", "app": "Gridloom", "version": info["version"]}:
        raise SmokeError("Health version/app/status mismatch")
    index_path = root / "frontend/dist/index.html"
    index = index_path.read_bytes()
    if len(index) > MAX_METADATA or read("/", MAX_METADATA) != index:
        raise SmokeError("Served frontend does not match packaged index.html")
    parser = Assets()
    parser.feed(index.decode("utf-8"))
    if not parser.scripts or not parser.paths or len(parser.paths) > 100:
        raise SmokeError("Frontend has no bounded set of bundled script assets")
    for path in set(parser.paths):
        expected = (root / "frontend/dist" / path[1:]).read_bytes()
        if not expected or len(expected) > 32 * 1024 * 1024 or read(path, 32 * 1024 * 1024) != expected:
            raise SmokeError(f"Served asset differs from package: {path}")


def stop_group(process):
    # Kill the group even if the wrapper exited before its children.
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        process.wait(timeout=3)
    except subprocess.TimeoutExpired:
        pass
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    process.wait(timeout=3)


def isolated_environment():
    environment = os.environ.copy()
    for key in list(environment):
        if key.startswith(("DUCKDB", "LD_")) or key in ("GRIDLOOM_FRONTEND_DIR", "DISPLAY", "WAYLAND_DISPLAY", "WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS", "BASH_ENV", "ENV"):
            environment.pop(key, None)
    return environment


def check_dependency_output(root, output):
    if re.search(r"\bnot found\b", output):
        raise SmokeError("Package has unresolved shared libraries")
    matches = []
    for line in output.splitlines():
        if "libduckdb" not in line.lower():
            continue
        match = re.fullmatch(r"\s*libduckdb\.so\s+=>\s+(.+?)\s+\(0x[0-9a-fA-F]+\)\s*", line)
        if not match:
            raise SmokeError("Unexpected DuckDB shared-library dependency")
        matches.append(match.group(1))
    if len(matches) != 1:
        raise SmokeError("Expected exactly one bundled libduckdb.so dependency")
    try:
        resolved = Path(matches[0]).resolve(strict=True)
        bundled = (root / "lib/libduckdb.so").resolve(strict=True)
    except OSError as error:
        raise SmokeError(f"DuckDB dependency path is unavailable: {error}") from error
    if not Path(matches[0]).is_absolute() or resolved != bundled:
        raise SmokeError("DuckDB resolves outside the relocated package")


def verify_dependencies(root, info, environment):
    """Run only after integrity and identity checks: ldd may execute ELF loaders."""
    env = environment.copy()
    env["LD_LIBRARY_PATH"] = str(root.resolve() / "lib")
    env["LC_ALL"] = "C"
    with tempfile.TemporaryDirectory(prefix="gridloom-dependencies-cwd-") as cwd, tempfile.TemporaryFile() as log:
        process = subprocess.Popen(["ldd", str(root.resolve() / f"gridloom-{info['mode']}")],
                                   cwd=cwd, env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        deadline = time.monotonic() + 10
        try:
            while process.poll() is None:
                if time.monotonic() >= deadline:
                    raise SmokeError("Dependency inspection timed out")
                if os.fstat(log.fileno()).st_size > 512 * 1024:
                    raise SmokeError("Dependency inspection output limit exceeded")
                time.sleep(0.05)
            if os.fstat(log.fileno()).st_size > 512 * 1024:
                raise SmokeError("Dependency inspection output limit exceeded")
            output = os.pread(log.fileno(), 512 * 1024, 0).decode("utf-8", "replace")
            if process.returncode != 0:
                raise SmokeError(f"Dependency inspection failed ({process.returncode}): {output[:2000]}")
            check_dependency_output(root, output)
        finally:
            stop_group(process)


def launch_smoke(root, info, native_window=False, timeout=45):
    if platform.system() != "Linux" or info["architecture"] != platform.machine():
        raise SmokeError("Execution requires the package's Linux architecture")
    if native_window and info["mode"] != "desktop":
        raise SmokeError("--native-window requires a desktop package")
    environment = isolated_environment()
    verify_dependencies(root, info, environment)
    command = [str(root.resolve() / "gridloom"), "--port", "0"]
    if native_window:
        command = ["dbus-run-session", "--", "xvfb-run", "-a", "env", "GDK_BACKEND=x11", "LIBGL_ALWAYS_SOFTWARE=1", "WEBKIT_DISABLE_COMPOSITING_MODE=1"] + command
    elif info["mode"] == "desktop":
        command.append("--server")
    # Deliberately outside both source checkout and extracted package.
    with tempfile.TemporaryDirectory(prefix="gridloom-relocated-cwd-") as cwd:
        with tempfile.TemporaryFile() as log:
            process = subprocess.Popen(command, cwd=cwd, env=environment, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
            deadline = time.monotonic() + min(timeout, 45 if native_window else timeout)
            try:
                while time.monotonic() < deadline:
                    if process.poll() is not None:
                        raise SmokeError(f"Package exited prematurely ({process.returncode})")
                    if os.fstat(log.fileno()).st_size > MAX_LOG:
                        raise SmokeError("Package smoke log size limit exceeded")
                    output = os.pread(log.fileno(), MAX_LOG, 0).decode("utf-8", "replace")
                    match = ENDPOINT.search(output)
                    if match and (not native_window or WINDOW_MARKER in output.splitlines()):
                        port = int(match.group(1))
                        if not 0 < port < 65536:
                            raise SmokeError("Invalid ephemeral port reported")
                        try:
                            check_http(root, info, port, deadline)
                        except (urllib.error.URLError, TimeoutError, ConnectionError):
                            time.sleep(0.1)
                            continue
                        if process.poll() is not None:
                            raise SmokeError("Package exited during HTTP checks")
                        print("PASS: relocated package health/version, frontend, assets and bundled DuckDB resolution verified." + (" Native window-open marker observed; native interactions NOT exercised." if native_window else " Native window NOT exercised."))
                        return
                    time.sleep(0.1)
                raise SmokeError("Timed out waiting for package readiness" + (" and native window-open marker" if native_window else ""))
            finally:
                stop_group(process)
                print(os.pread(log.fileno(), MAX_LOG, 0).decode("utf-8", "replace"), end="")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("--expected-commit")
    parser.add_argument("--expected-tree")
    parser.add_argument("--native-window", action="store_true")
    parser.add_argument("--timeout", type=float, default=45)
    args = parser.parse_args(argv)
    if not 0 < args.timeout <= 120:
        parser.error("--timeout must be greater than zero and at most 120 seconds")
    try:
        with tempfile.TemporaryDirectory(prefix="gridloom-verified-package-") as location:
            root, info = verify_package(args.archive, Path(location), args.expected_commit, args.expected_tree)
            print(f"Verified checksums and build identity: {info['mode']} {info['version']} {info['source_commit']} (checksums are not signatures).", flush=True)
            launch_smoke(root, info, args.native_window, args.timeout)
    except (SmokeError, OSError, ValueError) as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
