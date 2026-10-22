"""Shipped document runner. Keep standard PDF quality with a bounded pipeline.

The Rust parent owns cancellation, whole-process time/memory limits and atomic
publication. This process never downloads models or publishes partial results.
"""
import argparse
import os
import platform
import sys
from pathlib import Path

os.environ["HF_HUB_OFFLINE"] = "1"
os.environ["TRANSFORMERS_OFFLINE"] = "1"
os.environ["HF_DATASETS_OFFLINE"] = "1"
os.environ["TOKENIZERS_PARALLELISM"] = "false"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    parser.add_argument("--output-file", required=True, type=Path)
    parser.add_argument("--artifacts-path", required=True, type=Path)
    parser.add_argument("--pdf-backend", choices=["pypdfium2"])
    parser.add_argument("--probe", action="store_true")
    args = parser.parse_args()
    if args.probe:
        import logging
        logging.basicConfig(level=logging.INFO)
        print("Offline probe: loading document conversion libraries", flush=True)

    from docling.datamodel.accelerator_options import AcceleratorOptions
    from docling.datamodel.backend_options import ThreadedDoclingParseBackendOptions
    from docling.datamodel.base_models import ConversionStatus, InputFormat, FailureCategory
    from docling.datamodel.pipeline_options import PdfPipelineOptions, RapidOcrOptions
    from docling.document_converter import DocumentConverter, PdfFormatOption, ImageFormatOption
    from docling.backend.docling_parse_backend import ThreadedDoclingParseDocumentBackend
    from docling.backend.pypdfium2_backend import PyPdfiumDocumentBackend
    from docling_core.types.doc import ImageRefMode

    options = PdfPipelineOptions(
        artifacts_path=args.artifacts_path,
        accelerator_options=AcceleratorOptions(num_threads=2),
        do_ocr=True,
        ocr_options=RapidOcrOptions(backend="onnxruntime", mode="full_page" if args.probe else "default"),
        do_table_structure=True,
        document_timeout=270,
        ocr_batch_size=1,
        layout_batch_size=1,
        table_batch_size=1,
        queue_max_size=2,
        generate_page_images=False,
        generate_picture_images=False,
        generate_parsed_pages=False,
    )
    backend_options = ThreadedDoclingParseBackendOptions(
        parser_threads=1, max_concurrent_results=2,
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
    if args.probe:
        print("Offline probe: converting page with full-page OCR", flush=True)
    result = converter.convert(args.source, raises_on_error=False,
                               max_file_size=512 * 1024 * 1024, max_num_pages=10000)
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
    result.document.save_as_markdown(args.output_file, image_mode=ImageRefMode.PLACEHOLDER)
    if not args.output_file.is_file() or not args.output_file.read_text().strip():
        print("Docling did not produce document text", file=sys.stderr)
        return 1
    if args.probe:
        text = args.output_file.read_text().lower()
        if "sag" not in text or "probe" not in text:
            print("Offline OCR probe did not recognize the required text", file=sys.stderr)
            return 1
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except MemoryError:
        print("Docling memory allocation failed", file=sys.stderr)
        sys.exit(75)
