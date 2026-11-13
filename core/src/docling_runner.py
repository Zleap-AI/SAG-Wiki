"""Shipped document runner. Keep standard PDF quality with a bounded pipeline.

The Rust parent owns cancellation, whole-process time/memory limits and atomic
publication. This process never downloads models or publishes partial results.
"""
import argparse
import json
import os
import platform
import sys
from pathlib import Path

os.environ["HF_HUB_OFFLINE"] = "1"
os.environ["TRANSFORMERS_OFFLINE"] = "1"
os.environ["HF_DATASETS_OFFLINE"] = "1"
os.environ["TOKENIZERS_PARALLELISM"] = "false"

IMAGE_EXTENSIONS = {".bmp", ".gif", ".jpeg", ".jpg", ".png", ".tif", ".tiff", ".webp"}


def image_ocr(artifacts_path):
    """Load the bundled OCR models once for direct image recognition."""
    from rapidocr import RapidOCR

    models = artifacts_path / "RapidOcr"
    paths = {
        "Det.model_path": models / "PP-OCRv6_det_small.onnx",
        "Cls.model_path": models / "ch_ppocr_mobile_v2.0_cls_mobile.onnx",
        "Rec.model_path": models / "PP-OCRv6_rec_small.onnx",
    }
    if not all(path.is_file() for path in paths.values()):
        raise FileNotFoundError("RapidOCR models are incomplete")
    return RapidOCR(params={
        **{key: str(path) for key, path in paths.items()},
        "Global.text_score": 0.5,
        "Global.log_level": "warning",
        "EngineConfig.onnxruntime.intra_op_num_threads": 2,
        "EngineConfig.onnxruntime.inter_op_num_threads": 1,
    })


def convert_image(source, artifacts_path, ocr=None):
    """Run the bundled OCR engine directly for standalone images.

    The full Docling pipeline adds layout and table inference intended for
    documents. Images only need text recognition, and avoiding that pipeline
    also avoids importing torch and initializing its models.
    """
    from PIL import Image

    with Image.open(source) as image:
        if getattr(image, "n_frames", 1) != 1:
            return None
    if ocr is None:
        ocr = image_ocr(artifacts_path)
    result = ocr(str(source))
    if result is None or result.boxes is None or not result.txts:
        return ""
    return result.to_markdown().strip()


def write_output(output_file, text):
    """Publish one worker result only after its complete Markdown is written."""
    temporary = output_file.with_name(f".{output_file.name}.{os.getpid()}.tmp")
    temporary.write_text(f"{text}\n" if text else "", encoding="utf-8")
    os.replace(temporary, output_file)


def run_image_worker(artifacts_path):
    ocr = image_ocr(artifacts_path)
    print(json.dumps({"status": "ready"}), flush=True)
    for line in sys.stdin:
        request = None
        try:
            request = json.loads(line)
            request_id = request["id"]
            source = Path(request["source"])
            output_file = Path(request["output"])
            text = convert_image(source, artifacts_path, ocr)
            if text is None:
                print(json.dumps({"id": request_id, "status": "fallback"}), flush=True)
                continue
            write_output(output_file, text)
            print(json.dumps({"id": request_id, "status": "ok"}), flush=True)
        except Exception as error:
            print(json.dumps({
                "id": request.get("id") if isinstance(request, dict) else None,
                "status": "error",
                "message": str(error),
            }), flush=True)
    return 0


def run_pdf_worker(convert_one):
    """A scoped serial PDF session; any failure retires the process."""
    import contextlib
    import gc
    import tempfile

    print(json.dumps({"status": "ready"}), flush=True)
    for line in sys.stdin:
        request = None
        temporary = None
        code = 1
        try:
            request = json.loads(line)
            request_id = request["id"]
            source = Path(request["source"])
            output = Path(request["output"])
            if source.suffix.lower() != ".pdf":
                raise ValueError("PDF session only accepts PDF segments")
            with tempfile.NamedTemporaryFile(prefix=".sag-pdf-output-", dir=output.parent, delete=False) as file:
                temporary = Path(file.name)
            with contextlib.redirect_stdout(sys.stderr):
                code = convert_one(source, temporary)
            if code:
                raise RuntimeError(f"PDF conversion failed (code {code}); see parser diagnostics")
            code = 1
            if not temporary.read_text(encoding="utf-8").strip():
                raise ValueError("PDF conversion produced no document text")
            # Document results/native backends must not accumulate across segments.
            gc.collect()
            os.replace(temporary, output)
            print(json.dumps({"id": request_id, "status": "ok"}), flush=True)
        except Exception as error:
            if isinstance(error, MemoryError):
                code = 75
            print(json.dumps({"id": request.get("id") if isinstance(request, dict) else None,
                              "status": "error", "code": code, "message": str(error)}), flush=True)
            return 1
        finally:
            if temporary is not None:
                temporary.unlink(missing_ok=True)
    return 0


def build_document_converter(args, runtime):
    from docling.datamodel.accelerator_options import AcceleratorOptions
    from docling.datamodel.backend_options import ThreadedDoclingParseBackendOptions
    from docling.datamodel.base_models import ConversionStatus, InputFormat, FailureCategory
    from docling.datamodel.pipeline_options import PdfPipelineOptions, RapidOcrOptions
    from docling.document_converter import DocumentConverter, PdfFormatOption, ImageFormatOption
    from docling.backend.docling_parse_backend import ThreadedDoclingParseDocumentBackend
    from docling.backend.pypdfium2_backend import PyPdfiumDocumentBackend

    options = PdfPipelineOptions(
        artifacts_path=args.artifacts_path,
        accelerator_options=AcceleratorOptions(num_threads=runtime.get("docling_threads", 2)),
        do_ocr=runtime.get("do_ocr", True),
        ocr_options=RapidOcrOptions(backend="onnxruntime", mode="full_page" if args.probe else "default"),
        do_table_structure=runtime.get("do_table_structure", True),
        document_timeout=runtime.get("docling_timeout_secs", 1800),
        ocr_batch_size=runtime.get("ocr_batch_size", 1),
        layout_batch_size=runtime.get("layout_batch_size", 1),
        table_batch_size=runtime.get("table_batch_size", 1),
        queue_max_size=runtime.get("queue_max_size", 2),
        generate_page_images=False,
        generate_picture_images=False,
        generate_parsed_pages=False,
    )
    backend_options = ThreadedDoclingParseBackendOptions(
        parser_threads=runtime.get("pdf_parser_threads", 1), max_concurrent_results=runtime.get("pdf_concurrent_results", 2),
        release_native_memory_every_n_pages=16,
    )
    # Recent docling-parse releases omit Intel Mac wheels; PDFium remains official.
    use_pdfium = bool(args.pdf_backend) or (sys.platform == "darwin" and platform.machine() == "x86_64")
    pdf = PdfFormatOption(
        pipeline_options=options,
        backend=PyPdfiumDocumentBackend if use_pdfium else ThreadedDoclingParseDocumentBackend,
        backend_options=backend_options,
    )
    converter = DocumentConverter(format_options={
        InputFormat.PDF: pdf,
        InputFormat.IMAGE: ImageFormatOption(pipeline_options=options),
    })
    return converter


def convert_document(converter, source, output_file, runtime, probe=False):
    from docling.datamodel.base_models import ConversionStatus, FailureCategory
    from docling_core.types.doc import ImageRefMode
    if probe:
        print("Offline probe: converting page with full-page OCR", flush=True)
    result = converter.convert(source, raises_on_error=False,
                               max_file_size=runtime.get("conversion_input_mib", 512) * 1024 * 1024, max_num_pages=runtime.get("max_document_pages", 10000))
    if result.status != ConversionStatus.SUCCESS:
        errors = "\n".join(str(error) for error in result.errors)
        print(f"Conversion did not complete: {result.status}\n{errors}", file=sys.stderr)
        categories = {error.category for error in result.errors}
        if categories & {FailureCategory.TIMEOUT, FailureCategory.CAPACITY}:
            return 75
        # Only a categorized parser rejection is a backend compatibility retry.
        # Dependency/model/filesystem errors keep the ordinary failure code.
        from docling.datamodel.document import get_input_rejection_cause
        from docling.exceptions import DocumentLoadError
        cause = get_input_rejection_cause(result.input)
        if isinstance(cause, DocumentLoadError) or (cause is None and categories == {FailureCategory.BACKEND_FAILURE}):
            return 76
        return 1
    result.document.save_as_markdown(output_file, image_mode=ImageRefMode.PLACEHOLDER)
    if not output_file.is_file() or not output_file.read_text(encoding="utf-8").strip():
        print("Docling did not produce document text", file=sys.stderr)
        return 1
    if probe:
        text = output_file.read_text(encoding="utf-8").lower()
        if "sag" not in text or "probe" not in text:
            print("Offline OCR probe did not recognize the required text", file=sys.stderr)
            return 1
    return 0


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("source", nargs="?", type=Path)
    parser.add_argument("--output-file", type=Path)
    parser.add_argument("--artifacts-path", required=True, type=Path)
    parser.add_argument("--pdf-backend", choices=["pypdfium2"])
    parser.add_argument("--probe", action="store_true")
    workers = parser.add_mutually_exclusive_group()
    workers.add_argument("--worker", action="store_true")
    workers.add_argument("--pdf-worker", action="store_true")
    args = parser.parse_args()
    if args.worker:
        if args.artifacts_path is None:
            parser.error("--artifacts-path is required with --worker")
        try:
            return run_image_worker(args.artifacts_path)
        except Exception as error:
            print(json.dumps({"status": "error", "message": str(error)}), flush=True)
            return 1
    if args.source is None and not args.pdf_worker:
        parser.error("source is required unless --worker is used")
    if args.output_file is None and not args.pdf_worker:
        parser.error("--output-file is required unless --worker is used")
    runtime = json.loads(os.environ.get("SAG_CONVERSION_SETTINGS", "{}"))
    if args.probe:
        import logging
        logging.basicConfig(level=logging.INFO)
        print("Offline probe: loading document conversion libraries", flush=True)

    if args.source is not None and args.source.suffix.lower() in IMAGE_EXTENSIONS:
        try:
            text = convert_image(args.source, args.artifacts_path)
            if text is not None:
                write_output(args.output_file, text)
                if args.probe:
                    lowered = text.lower()
                    if "sag" not in lowered or "image" not in lowered:
                        print("Offline image OCR probe did not recognize the probe text", file=sys.stderr)
                        return 1
                return 0
        except Exception as error:
            print(f"Direct image OCR failed; falling back to Docling: {error}", file=sys.stderr)

    converter = build_document_converter(args, runtime)
    if args.pdf_worker:
        return run_pdf_worker(lambda source, output: convert_document(converter, source, output, runtime))
    return convert_document(converter, args.source, args.output_file, runtime, args.probe)


if __name__ == "__main__":
    try:
        sys.exit(main())
    except MemoryError:
        print("Docling memory allocation failed", file=sys.stderr)
        sys.exit(75)
