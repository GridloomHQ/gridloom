import importlib.util
import io
import json
from pathlib import Path
import struct
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("package_artifact", Path(__file__).parents[1] / "package-artifact.py")
artifact = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(artifact)


def elf(machine=62, elf_type=3, elf_class=2, byte_order=1, osabi=0):
    header = bytearray(64)
    header[:9] = b"\x7fELF" + bytes((elf_class, byte_order, 1, osabi, 0))
    struct.pack_into("<HHI", header, 16, elf_type, machine, 1)
    struct.pack_into("<H", header, 52, 64)
    return header


def record(path, name="gridloom-server", kind=None):
    return {"reason": "compiler-artifact", "target": {"name": name, "kind": ["bin"] if kind is None else kind}, "executable": str(path) if path is not None else None}


class ArtifactTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def binary(self, relative="custom-target/release/gridloom-server", data=None):
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(elf() if data is None else data)
        return path

    def resolve(self, records, **kwargs):
        output = io.StringIO("\n".join(json.dumps(r) for r in records) + "\n")
        return artifact.resolve_artifact(output, "gridloom-server", host_system=kwargs.get("host_system", "Linux"), host_architecture=kwargs.get("host_architecture", "x86_64"))

    def test_custom_target_artifact_is_selected_not_stale_default(self):
        stale = self.binary("target/release/gridloom-server", b"stale invalid binary")
        current = self.binary("different-target/x86_64-unknown-linux-gnu/release/gridloom-server")
        result = self.resolve([{"reason": "compiler-message", "message": {}}, record(stale, name="other"), record(current), {"reason": "build-finished", "success": True}])
        self.assertEqual(result, {"executable": str(current.resolve()), "architecture": "x86_64"})
        self.assertNotEqual(result["executable"], str(stale))

    def test_no_fallback_to_stale_default_for_missing_reported_artifact(self):
        self.binary("target/release/gridloom-server")
        with self.assertRaisesRegex(artifact.ArtifactError, "missing"):
            self.resolve([record(self.root / "missing/gridloom-server")])

    def test_missing_null_wrong_kind_and_ambiguous_artifacts(self):
        path = self.binary()
        for records in ([], [record(path, kind=["lib"])], [record(path, name="other")], [record(None)], [record(path), record(path)]):
            with self.subTest(records=records), self.assertRaises(artifact.ArtifactError):
                self.resolve(records)
        directory = self.root / "directory"
        directory.mkdir()
        with self.assertRaisesRegex(artifact.ArtifactError, "regular file"):
            self.resolve([record(directory)])

    def test_native_architectures_and_elf_types(self):
        for machine, host in ((62, "x86_64"), (183, "aarch64")):
            for elf_type in (2, 3):
                for osabi in (0, 3):
                    with self.subTest(machine=machine, elf_type=elf_type, osabi=osabi):
                        path = self.binary(data=elf(machine=machine, elf_type=elf_type, osabi=osabi))
                        self.assertEqual(self.resolve([record(path)], host_architecture=host)["architecture"], host)

    def test_cross_architecture_and_non_linux_hosts_rejected(self):
        path = self.binary(data=elf(machine=183))
        with self.assertRaisesRegex(artifact.ArtifactError, "Cross-architecture"):
            self.resolve([record(path)])
        path = self.binary()
        with self.assertRaisesRegex(artifact.ArtifactError, "Linux host"):
            self.resolve([record(path)], host_system="Darwin")
        with self.assertRaisesRegex(artifact.ArtifactError, "Unsupported Linux host"):
            self.resolve([record(path)], host_architecture="riscv64")

    def test_malformed_unsupported_and_incomplete_elf(self):
        invalid = [b"not ELF", bytes(elf())[:40], elf(machine=243), elf(elf_class=1), elf(byte_order=2), elf(elf_type=1), elf(osabi=9)]
        bad_version = elf()
        struct.pack_into("<I", bad_version, 20, 2)
        invalid.append(bad_version)
        bad_size = elf()
        struct.pack_into("<H", bad_size, 52, 32)
        invalid.append(bad_size)
        for data in invalid:
            with self.subTest(data=data), self.assertRaises(artifact.ArtifactError):
                self.resolve([record(self.binary(data=data))])

    def test_bad_json_and_output_limits(self):
        for content in ("not json\n", "[]\n", "{}\n" * (artifact.MAX_RECORDS + 1), "x" * (artifact.MAX_LINE + 1)):
            with self.subTest(size=len(content)), self.assertRaises(artifact.ArtifactError):
                artifact.resolve_artifact(io.StringIO(content), "gridloom-server", "Linux", "x86_64")


if __name__ == "__main__":
    unittest.main()
