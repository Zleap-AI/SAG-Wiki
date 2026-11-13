"""Fast protocol/lifecycle regressions; no OCR dependencies are required."""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("document_runner", Path(__file__).parents[1] / "core/src/docling_runner.py")
runner = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runner)


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
