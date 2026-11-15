"""Fast protocol/lifecycle regressions; no OCR dependencies are required."""
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("document_runner", Path(__file__).parents[1] / "core/src/docling_runner.py")
runner = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runner)


class WindowsResourcePathTests(unittest.TestCase):
    def test_windows_converter_receives_compatible_model_input_and_output_paths(self):
        argv = ["runner", r"\\?\C:\input\probe.pdf", "--output-file", r"\\?\C:\output\probe.md",
                "--artifacts-path", r"\\?\C:\components\models", "--probe"]
        with patch("sys.platform", "win32"), patch("sys.argv", argv), \
                patch.object(runner, "build_document_converter", return_value=object()) as build, \
                patch.object(runner, "convert_document", return_value=0) as convert, \
                contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(runner.main(), 0)
        args = build.call_args.args[0]
        self.assertEqual(str(args.artifacts_path), r"C:\components\models")
        self.assertEqual(str(convert.call_args.args[1]), r"C:\input\probe.pdf")
        self.assertEqual(str(convert.call_args.args[2]), r"C:\output\probe.md")

    def test_windows_worker_requests_write_to_the_normalized_output(self):
        for kind in ("pdf", "image"):
            with self.subTest(worker=kind), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                source = root / ("input.pdf" if kind == "pdf" else "input.png")
                destination = root / "output.md"
                request = {"id": 1, "source": "\\\\?\\" + str(source), "output": "\\\\?\\" + str(destination)}
                def convert(actual_source, output):
                    self.assertEqual(actual_source, source)
                    output.write_text("document text", encoding="utf-8")
                    return 0
                with patch("sys.platform", "win32"), patch("sys.stdin", io.StringIO(json.dumps(request) + "\n")), \
                        patch.object(runner, "image_ocr", return_value=object()), \
                        patch.object(runner, "convert_image", return_value="document text"), \
                        contextlib.redirect_stdout(io.StringIO()) as response:
                    code = runner.run_pdf_worker(convert) if kind == "pdf" else runner.run_image_worker(root)
                self.assertEqual(code, 0, response.getvalue())
                self.assertEqual(json.loads(response.getvalue().splitlines()[-1])["status"], "ok")
                self.assertEqual(destination.read_text(encoding="utf-8").strip(), "document text")

    def test_windows_import_paths_allow_native_resource_suffixes(self):
        paths = [
            r"\\?\C:\Users\Jomy\components\site-packages",
            r"\\?\UNC\server\share\site-packages",
            r"C:\Users\用户\Agent History\site-packages",
            "",
        ]
        with patch("sys.platform", "win32"), patch("sys.path", paths.copy()):
            SPEC.loader.exec_module(importlib.util.module_from_spec(SPEC))
            self.assertEqual(sys.path, [
                r"C:\Users\Jomy\components\site-packages",
                r"\\server\share\site-packages",
                paths[2],
                "",
            ])

    def test_other_platforms_keep_import_paths(self):
        paths = [r"\\?\C:\site-packages", "/tmp/site-packages", ""]
        for platform in ("darwin", "linux"):
            with self.subTest(platform=platform), patch("sys.platform", platform), patch("sys.path", paths.copy()):
                SPEC.loader.exec_module(importlib.util.module_from_spec(SPEC))
                self.assertEqual(sys.path, paths)

    @unittest.skipUnless(sys.platform == "win32", "Requires Windows file-opening semantics")
    def test_windows_table_model_config_load_from_verbatim_artifacts_path(self):
        with tempfile.TemporaryDirectory() as directory:
            models = Path(directory) / "模型 models"
            config = models / "docling-project--docling-models/model_artifacts/tableformer/accurate/tm_config.json"
            config.parent.mkdir(parents=True)
            config.write_text('{"test": true}', encoding="utf-8")
            # Exercise argparse and the same model path interpolation/open as
            # tableformer.common.read_config, without loading inference models.
            script = (
                "import importlib.util, json, sys\n"
                "spec = importlib.util.spec_from_file_location('model_regression', sys.argv[1])\n"
                "runner = importlib.util.module_from_spec(spec)\n"
                "spec.loader.exec_module(runner)\n"
                "def build(args, runtime):\n"
                "    filename = f'{args.artifacts_path}/docling-project--docling-models/model_artifacts/tableformer/accurate/tm_config.json'\n"
                "    with open(filename, 'r', encoding='utf-8') as config:\n"
                "        assert json.load(config) == {'test': True}\n"
                "    return object()\n"
                "runner.build_document_converter = build\n"
                "runner.convert_document = lambda *args: 0\n"
                "sys.argv = ['runner', 'probe.pdf', '--output-file', 'probe.md', '--artifacts-path', sys.argv[2], '--probe']\n"
                "raise SystemExit(runner.main())\n"
            )
            result = subprocess.run(
                [sys.executable, "-S", "-c", script, str(SPEC.origin), "\\\\?\\" + str(models)],
                capture_output=True, text=True, timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stderr)

    @unittest.skipUnless(sys.platform == "win32", "Requires Windows file-opening semantics")
    def test_windows_native_resource_load_from_verbatim_pythonpath(self):
        with tempfile.TemporaryDirectory() as directory:
            packages = Path(directory) / "组件 component" / "site-packages"
            package = packages / "docling_parse"
            resource = package / "pdf_resources" / "glyphs" / "standard" / "additional.dat"
            resource.parent.mkdir(parents=True)
            resource.write_bytes(b"glyph resource")
            (package / "__init__.py").write_text("", encoding="utf-8")
            # Match docling-parse's resource construction and use a native CRT
            # file open, avoiding any normalization by Python's file APIs.
            script = (
                "import runpy, sys, os, ctypes\n"
                "runpy.run_path(sys.argv[1], run_name='resource_regression')\n"
                "import docling_parse\n"
                "filename = os.path.dirname(docling_parse.__file__) + '/pdf_resources/glyphs//standard/additional.dat'\n"
                "crt = ctypes.CDLL('msvcrt')\n"
                "crt._wfopen.argtypes = [ctypes.c_wchar_p, ctypes.c_wchar_p]\n"
                "crt._wfopen.restype = ctypes.c_void_p\n"
                "crt.fgetc.argtypes = [ctypes.c_void_p]\n"
                "crt.fclose.argtypes = [ctypes.c_void_p]\n"
                "handle = crt._wfopen(filename, 'rb')\n"
                "if not handle: raise RuntimeError('filename does not exists: ' + filename)\n"
                "first_byte = crt.fgetc(handle)\n"
                "crt.fclose(handle)\n"
                "assert first_byte == ord('g')\n"
            )
            result = subprocess.run(
                [sys.executable, "-S", "-c", script, str(SPEC.origin)],
                env={**os.environ, "PYTHONPATH": "\\\\?\\" + str(packages)},
                capture_output=True, text=True, timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stderr)


class PdfSessionTests(unittest.TestCase):
    def test_two_pdf_requests_build_one_converter_with_unchanged_options(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            requests = [{"id": i, "source": str(root / f"part-{i}.pdf"), "output": str(root / f"part-{i}.md")} for i in (1, 2)]
            converter = object()
            seen = []
            def convert(actual, source, output, runtime, probe=False):
                self.assertIs(actual, converter)
                self.assertEqual(runtime, {"docling_threads": 2, "do_ocr": True, "do_table_structure": True})
                seen.append(source.name)
                output.write_text(source.name)
                return 0
            with patch.object(runner, "build_document_converter", return_value=converter) as build, patch.object(runner, "convert_document", side_effect=convert), patch("sys.argv", ["runner", "--pdf-worker", "--artifacts-path", directory]), patch.dict("os.environ", {"SAG_CONVERSION_SETTINGS": json.dumps({"docling_threads": 2, "do_ocr": True, "do_table_structure": True})}), patch("sys.stdin", io.StringIO("".join(json.dumps(r) + "\n" for r in requests))), contextlib.redirect_stdout(io.StringIO()) as output:
                self.assertEqual(runner.main(), 0)
            self.assertEqual(build.call_count, 1)
            self.assertEqual(seen, ["part-1.pdf", "part-2.pdf"])
            self.assertEqual([json.loads(line)["status"] for line in output.getvalue().splitlines()], ["ready", "ok", "ok"])
            self.assertEqual((root / "part-2.md").read_text(), "part-2.pdf")

    def test_failed_request_preserves_output_and_retires_worker(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            destination = root / "old.md"
            destination.write_text("previous complete output")
            requests = [{"id": i, "source": str(root / "part.pdf"), "output": str(destination)} for i in (1, 2)]
            def convert(source, output):
                output.write_text("partial")
                return 76
            with patch("sys.stdin", io.StringIO("".join(json.dumps(r) + "\n" for r in requests))), contextlib.redirect_stdout(io.StringIO()) as response:
                self.assertEqual(runner.run_pdf_worker(convert), 1)
            lines = [json.loads(line) for line in response.getvalue().splitlines()]
            self.assertEqual(lines[-1]["code"], 76)
            self.assertEqual(len(lines), 2)
            self.assertEqual(destination.read_text(), "previous complete output")
            self.assertEqual(list(root.glob(".sag-pdf-output-*")), [])

    def test_empty_success_is_not_published(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            request = {"id": 1, "source": str(root / "part.pdf"), "output": str(root / "part.md")}
            with patch("sys.stdin", io.StringIO(json.dumps(request) + "\n")), contextlib.redirect_stdout(io.StringIO()) as output:
                self.assertEqual(runner.run_pdf_worker(lambda source, target: 0), 1)
            self.assertEqual(json.loads(output.getvalue().splitlines()[-1])["status"], "error")
            self.assertFalse((root / "part.md").exists())


if __name__ == "__main__":
    unittest.main()
