"""Installation-boundary tests; no packages, network, models or compiler needed."""
from pathlib import Path
import runpy
import sys
import tempfile
import types
import unittest
from unittest.mock import patch


class PrivateFfmpegTests(unittest.TestCase):
    def test_installer_copies_the_wheel_binary_despite_external_override(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            package = root / "site-packages" / "imageio_ffmpeg"
            (package / "binaries").mkdir(parents=True)
            bundled = package / "binaries" / "ffmpeg-fixture"
            bundled.write_bytes(b"official-wheel-binary")
            external = root / "external-ffmpeg"
            external.write_bytes(b"unrelated-environment-override")
            module = types.ModuleType("imageio_ffmpeg")
            module.__file__ = str(package / "__init__.py")
            module.get_ffmpeg_exe = lambda: str(external)
            definitions = types.ModuleType("imageio_ffmpeg._definitions")
            definitions.FNAME_PER_PLATFORM = {"fixture": bundled.name}
            definitions.get_platform = lambda: "fixture"
            hub = types.ModuleType("huggingface_hub")

            def snapshot_download(repository, *, revision, allow_patterns, local_dir):
                self.assertEqual(repository, "mlx-community/whisper-tiny-mlx")
                self.assertEqual(revision, "6caf9c55601caafbe6508a8b0d216bdf4783c4e8")
                self.assertEqual(allow_patterns, ["config.json", "weights.npz"])
                local_dir.mkdir(parents=True)
                (local_dir / "config.json").write_text("{}")
                (local_dir / "weights.npz").write_bytes(b"fixture")

            hub.snapshot_download = snapshot_download
            modules = {"imageio_ffmpeg": module, "imageio_ffmpeg._definitions": definitions,
                       "huggingface_hub": hub}
            candidate = root / "component"
            with patch.dict(sys.modules, modules), patch.object(sys, "argv", [
                "media-setup.py", "--download-media-models", "mlx", str(candidate)
            ]), patch.dict("os.environ", {"IMAGEIO_FFMPEG_EXE": str(external)}):
                runpy.run_path(str(Path(__file__).with_name("docling_media_setup.py")))["main"]()
            binary = candidate / "bin" / ("ffmpeg.exe" if sys.platform == "win32" else "ffmpeg")
            self.assertEqual(binary.read_bytes(), b"official-wheel-binary")
            self.assertEqual(external.read_bytes(), b"unrelated-environment-override")


if __name__ == "__main__":
    unittest.main()
