"""Release component checks run without downloading models or executing inference."""
import hashlib
import json
import marshal
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "scripts/package-docling-component.py"


class ComponentPackagingTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="sag component test ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        for folder in ("python/bin", "packages/example", "models/layout"):
            (self.root / folder).mkdir(parents=True)
        (self.root / "python/bin/python3.12").write_bytes(b"fixture interpreter")
        (self.root / "python/bin/python3").symlink_to("python3.12")
        (self.root / "packages/example/__init__.py").write_text("VALUE = 1\n")
        (self.root / "models/layout/model.bin").write_bytes(b"fixture model")
        (self.root / "requirements.txt").write_text("example==1.0\n")

    def package(self, output="component", *extra):
        return subprocess.run([
            sys.executable, str(SCRIPT), "--python-runtime", str(self.root / "python"),
            "--site-packages", str(self.root / "packages"), "--models", str(self.root / "models"),
            "--requirements-lock", str(self.root / "requirements.txt"),
            "--platform", "macos-aarch64", "--output", str(self.root / output), *extra,
        ], capture_output=True, text=True)

    def test_component_is_relocatable_and_every_payload_file_has_a_digest(self):
        result = self.package()
        self.assertEqual(result.returncode, 0, result.stderr)
        output = self.root / "component"
        manifest = json.loads((output / "manifest.json").read_text())
        self.assertEqual(manifest["schema"], 1)
        self.assertEqual(manifest["profile"], "documents")
        self.assertEqual(manifest["python"], "python/bin/python3.12")
        self.assertNotIn(str(self.root), json.dumps(manifest))
        self.assertFalse((output / "python/bin/python3").is_symlink())
        for entry in manifest["files"]:
            payload = output / entry["path"]
            self.assertEqual(payload.stat().st_size, entry["size"])
            self.assertEqual(hashlib.sha256(payload.read_bytes()).hexdigest(), entry["sha256"])
        moved = self.root / "relocated component"
        output.rename(moved)
        verify = subprocess.run([sys.executable, str(SCRIPT), "--verify", str(moved)], capture_output=True, text=True)
        self.assertEqual(verify.returncode, 0, verify.stderr)

    def test_modified_or_missing_payload_is_rejected(self):
        self.assertEqual(self.package().returncode, 0)
        output = self.root / "component"
        (output / "models/layout/model.bin").write_bytes(b"modified")
        result = subprocess.run([sys.executable, str(SCRIPT), "--verify", str(output)], capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("model.bin", result.stderr)

    def test_external_symlink_is_rejected_and_existing_output_is_preserved(self):
        (self.root / "secret").write_text("outside payload")
        (self.root / "packages/escape").symlink_to(self.root / "secret")
        result = self.package()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("symlink", result.stderr.lower())
        self.assertFalse((self.root / "component").exists())
        (self.root / "packages/escape").unlink()
        self.assertEqual(self.package().returncode, 0)
        original = (self.root / "component/manifest.json").read_bytes()
        self.assertNotEqual(self.package().returncode, 0)
        self.assertEqual((self.root / "component/manifest.json").read_bytes(), original)

    def test_verification_rejects_unlisted_directory_and_dangling_symlinks(self):
        self.assertEqual(self.package().returncode, 0)
        output = self.root / "component"
        external = self.root / "external"
        external.mkdir()
        (external / "payload.py").write_text("VALUE = 2\n")
        for target in (external, self.root / "missing"):
            with self.subTest(target=target):
                link = output / "site-packages/unlisted"
                link.symlink_to(target, target_is_directory=True)
                result = subprocess.run([sys.executable, str(SCRIPT), "--verify", str(output)], capture_output=True, text=True)
                link.unlink()
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("symlink", result.stderr.lower())

    def test_virtual_environment_is_not_an_independent_runtime(self):
        (self.root / "python/pyvenv.cfg").write_text("home = /builder/python\n")
        result = self.package()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("virtual", result.stderr.lower())
        self.assertFalse((self.root / "component").exists())

    def test_release_bytecode_is_hash_checked_and_has_no_builder_paths(self):
        import shlex
        interpreter = self.root / "python/bin/python3.12"
        interpreter.write_text("#!/bin/sh\nexec " + shlex.quote(sys.executable) + ' "$@"\n')
        interpreter.chmod(0o755)
        (self.root / "packages/olefile").mkdir()
        (self.root / "packages/olefile/olefile2.py").write_text("VALUE = 0xFFFFFFFAL\n")
        result = self.package("component", "--compile-bytecode")
        self.assertEqual(result.returncode, 0, result.stderr)
        output = self.root / "component"
        cached = next((output / "site-packages/example/__pycache__").glob("*.pyc"))
        data = cached.read_bytes()
        self.assertEqual(int.from_bytes(data[4:8], "little"), 3)
        self.assertEqual(marshal.loads(data[16:]).co_filename, "/sag-component/site-packages/example/__init__.py")
        manifest = json.loads((output / "manifest.json").read_text())
        self.assertIn(cached.relative_to(output).as_posix(), [entry["path"] for entry in manifest["files"]])
        for cache in output.rglob("*.pyc"):
            bytecode = cache.read_bytes()
            self.assertEqual(int.from_bytes(bytecode[4:8], "little"), 3, str(cache))
            self.assertNotIn(str(self.root), marshal.loads(bytecode[16:]).co_filename)


if __name__ == "__main__":
    unittest.main()
