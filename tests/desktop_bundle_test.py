"""Acceptance checks for the actual lightweight macOS desktop artifact."""
import os
import json
import zipfile
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
BUNDLE = Path(os.environ.get('SAG_DESKTOP_BUNDLE', ROOT / 'src-tauri/target/release/bundle/macos/SAG Wiki.app'))


class DesktopBundleTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not BUNDLE.is_dir():
            raise AssertionError('Build the desktop bundle before artifact acceptance')

    def test_desktop_contains_native_app_and_cli_without_parser_runtimes_or_models(self):
        resources = BUNDLE / 'Contents/Resources'
        self.assertTrue((BUNDLE / 'Contents/MacOS/sag-desktop').is_file())
        self.assertTrue((resources / 'bin/sag').is_file())
        self.assertFalse((resources / 'docling-component').exists(), 'Document dependencies must be installed on demand')
        forbidden = {'site-packages', '__pycache__', 'models', 'python', 'docling-component',
                     'ffmpeg', 'ffmpeg.exe', 'ffprobe', 'ffprobe.exe'}
        model_extensions = {'.whl', '.pyc', '.onnx', '.safetensors', '.pt', '.npz'}
        unexpected = [str(path.relative_to(resources)) for path in resources.rglob('*')
                      if path.name in forbidden or path.suffix in model_extensions]
        self.assertEqual(unexpected, [], 'Parser runtimes/models leaked into desktop resources')

    def test_browser_extension_package_contains_manifest_scripts_and_license(self):
        package = BUNDLE / 'Contents/Resources/clipper/sag-web-clipper-chrome.zip'
        with zipfile.ZipFile(package) as archive:
            manifest = json.loads(archive.read('manifest.json'))
            self.assertEqual(manifest['manifest_version'], 3)
            self.assertIn('contextMenus', manifest['permissions'])
            self.assertNotIn('nativeMessaging', manifest['permissions'])
            for filename in ('background.js', 'extract.js', 'popup.html', 'options.html', 'DEFUDDLE-LICENSE.txt'):
                self.assertGreater(len(archive.read(filename)), 0)

    def test_desktop_payload_stays_below_100_mib(self):
        size = sum(path.stat().st_size for path in BUNDLE.rglob('*') if path.is_file())
        self.assertLess(size, 100 * 1024**2, f'Lightweight app unexpectedly grew to {size / 1024**2:.1f} MiB')


if __name__ == '__main__':
    unittest.main()
