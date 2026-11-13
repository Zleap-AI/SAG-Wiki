"""Local-only split benchmark. Usage: python scripts/benchmark-qpdf.py INPUT.pdf QPDF

Requires pypdf for page-count verification. No upload; temporary PDFs are removed.
"""
import argparse
import hashlib
import json
import pathlib
import subprocess
import tempfile
import time
from pypdf import PdfReader

parser = argparse.ArgumentParser()
parser.add_argument("pdf", type=pathlib.Path)
parser.add_argument("qpdf", type=pathlib.Path)
args = parser.parse_args()
count = len(PdfReader(args.pdf).pages)
with tempfile.TemporaryDirectory(prefix="sag-qpdf-benchmark-") as directory:
    started = time.perf_counter()
    outputs = []
    for start in range(1, count + 1, 200):
        end = min(start + 199, count)
        output = pathlib.Path(directory) / f"{start}-{end}.pdf"
        subprocess.run([str(args.qpdf.resolve()), "--warning-exit-0", "--deterministic-id",
                        "--empty", "--pages", str(args.pdf.resolve()), f"{start}-{end}",
                        "--", str(output)], check=True, capture_output=True, timeout=120)
        outputs.append((output, end - start + 1))
    split_seconds = time.perf_counter() - started
    for output, expected in outputs:
        assert len(PdfReader(output).pages) == expected
    print(json.dumps({"input_bytes": args.pdf.stat().st_size,
                      "input_sha256": hashlib.sha256(args.pdf.read_bytes()).hexdigest(),
                      "pages": count, "part_pages": [n for _, n in outputs],
                      "part_bytes": [p.stat().st_size for p, _ in outputs],
                      "split_seconds": split_seconds}, indent=2))
