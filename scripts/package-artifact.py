#!/usr/bin/env python3
"""Select the requested binary from Cargo JSON and verify its native Linux ELF header.

This validates the artifact Cargo reported, not a guessed target/release pathname.
ELF checks establish basic host compatibility, not executable authenticity or safety.
"""
import argparse
import json
import platform
from pathlib import Path
import struct
import sys

MAX_LINE = 4 * 1024 * 1024
MAX_RECORDS = 200000
ARCHITECTURES = {62: "x86_64", 183: "aarch64"}


class ArtifactError(ValueError):
    pass


def inspect_elf(executable, host_system=None, host_architecture=None):
    """Return actual ELF architecture; reject unsupported or cross-host artifacts."""
    system = platform.system() if host_system is None else host_system
    host = platform.machine() if host_architecture is None else host_architecture
    if system != "Linux":
        raise ArtifactError("Packaging requires a Linux host")
    if host not in ARCHITECTURES.values():
        raise ArtifactError(f"Unsupported Linux host architecture: {host}")
    path = Path(executable)
    if not path.is_file():
        raise ArtifactError("Cargo executable is not an existing regular file")
    try:
        with path.open("rb") as stream:
            header = stream.read(64)
    except OSError as error:
        raise ArtifactError(f"Cannot read Cargo executable: {error}") from error
    if len(header) != 64 or header[:4] != b"\x7fELF":
        raise ArtifactError("Cargo executable is not a complete ELF64 header")
    if header[4:7] != bytes((2, 1, 1)):
        raise ArtifactError("Expected ELF64 little-endian version 1")
    if header[7] not in (0, 3) or header[8] != 0:
        raise ArtifactError("Expected Linux/SysV ELF OS ABI")
    elf_type, machine, version = struct.unpack_from("<HHI", header, 16)
    header_size = struct.unpack_from("<H", header, 52)[0]
    if elf_type not in (2, 3) or version != 1 or header_size != 64:
        raise ArtifactError("Expected executable/PIE ELF64 header")
    architecture = ARCHITECTURES.get(machine)
    if architecture is None:
        raise ArtifactError(f"Unsupported ELF machine: {machine}")
    if architecture != host:
        raise ArtifactError(f"Cross-architecture artifact rejected: {architecture} on {host}")
    return architecture


def resolve_artifact(stream, expected_name, host_system=None, host_architecture=None):
    """Consume Cargo JSON lines, accepting exactly one matching binary artifact."""
    if not isinstance(expected_name, str) or not expected_name or any(c in expected_name for c in "/\\\x00"):
        raise ArtifactError("Expected binary name must be a nonempty basename")
    candidates = []
    records = 0
    while True:
        line = stream.readline(MAX_LINE + 1)
        if not line:
            break
        records += 1
        if records > MAX_RECORDS or len(line) > MAX_LINE:
            raise ArtifactError("Cargo JSON output limit exceeded")
        if not line.strip():
            continue
        try:
            record = json.loads(line)
        except (ValueError, TypeError) as error:
            raise ArtifactError("Malformed Cargo JSON output") from error
        if not isinstance(record, dict):
            raise ArtifactError("Cargo JSON record must be an object")
        if record.get("reason") != "compiler-artifact":
            continue
        target = record.get("target")
        if not isinstance(target, dict) or target.get("name") != expected_name:
            continue
        kinds = target.get("kind")
        if not isinstance(kinds, list) or "bin" not in kinds:
            continue
        executable = record.get("executable")
        if not isinstance(executable, str) or not executable or "\x00" in executable:
            raise ArtifactError("Matching Cargo binary has no executable pathname")
        candidates.append(executable)
    if len(candidates) != 1:
        raise ArtifactError(f"Expected exactly one matching Cargo binary artifact; found {len(candidates)}")
    try:
        path = Path(candidates[0]).resolve(strict=True)
    except (OSError, RuntimeError) as error:
        raise ArtifactError(f"Cargo executable is missing or invalid: {error}") from error
    architecture = inspect_elf(path, host_system, host_architecture)
    return {"executable": str(path), "architecture": architecture}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("expected_binary")
    args = parser.parse_args(argv)
    try:
        print(json.dumps(resolve_artifact(sys.stdin, args.expected_binary)))
    except (ArtifactError, OSError) as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
