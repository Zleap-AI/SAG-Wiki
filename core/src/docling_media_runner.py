"""Offline audio/video conversion with local ASR and a private FFmpeg."""
import argparse
import math
import os
import platform
from pathlib import Path
import subprocess
import sys

for key in ("HF_HUB_OFFLINE", "TRANSFORMERS_OFFLINE", "HF_DATASETS_OFFLINE"):
    os.environ[key] = "1"
os.environ["TOKENIZERS_PARALLELISM"] = "false"


def validate_probe_document(document, text, video_probe=False):
    if "quick" not in text or "fox" not in text:
        raise RuntimeError("Offline ASR probe did not recognize the required speech")
    from docling_core.types.doc import TrackSource
    # Docling stores media provenance as a list of TrackSource on each text item;
    # page-oriented `prov` and an individual `source.start_time` are different APIs.
    tracks = [source for item in document.texts if item.text.strip()
              for source in item.source if isinstance(source, TrackSource)]
    if not tracks or not all(math.isfinite(source.start_time) and
                             math.isfinite(source.end_time) and
                             0 <= source.start_time < source.end_time for source in tracks):
        raise RuntimeError("ASR probe produced no valid timestamped transcript provenance")
    if video_probe and not document.pictures:
        raise RuntimeError("Video probe produced no sampled frame")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    parser.add_argument("--output-file", type=Path, required=True)
    parser.add_argument("--artifacts-path", type=Path, required=True)
    parser.add_argument("--probe", action="store_true")
    args = parser.parse_args()
    from docling.datamodel.accelerator_options import AcceleratorOptions
    from docling.datamodel.base_models import ConversionStatus, InputFormat
    from docling.datamodel.pipeline_options import AsrPipelineOptions, VideoPipelineOptions
    from docling.datamodel.pipeline_options_asr_model import InlineAsrMlxWhisperOptions, InlineAsrNativeWhisperOptions
    from docling.document_converter import DocumentConverter, AudioFormatOption, VideoFormatOption
    from docling_core.types.doc import ImageRefMode

    mlx = sys.platform == "darwin" and platform.machine() == "arm64"
    local = args.artifacts_path / ("whisper-tiny-mlx" if mlx else "tiny.pt")
    if mlx:
        if not (local / "weights.npz").is_file() or not (local / "config.json").is_file():
            raise FileNotFoundError("Local MLX Whisper model is missing; reinstall the media component")
        asr = InlineAsrMlxWhisperOptions(repo_id=str(local), language=None, word_timestamps=True)
    else:
        if not local.is_file():
            raise FileNotFoundError("Local Whisper checkpoint is missing; reinstall the media component")
        asr = InlineAsrNativeWhisperOptions(repo_id=str(local), language=None, word_timestamps=True)
    common = dict(artifacts_path=args.artifacts_path,
                  accelerator_options=AcceleratorOptions(num_threads=2),
                  asr_options=asr, document_timeout=270)
    converter = DocumentConverter(allowed_formats=[InputFormat.AUDIO, InputFormat.VIDEO], format_options={
        InputFormat.AUDIO: AudioFormatOption(pipeline_options=AsrPipelineOptions(**common)),
        InputFormat.VIDEO: VideoFormatOption(pipeline_options=VideoPipelineOptions(
            **common, enable_diarization=False, generate_frame_images=True,
            frame_interval_seconds=10.0, max_sampled_frames=200)),
    })

    def convert(source, output, video_probe=False):
        result = converter.convert(source, raises_on_error=False, max_file_size=512 * 1024 * 1024)
        if result.status != ConversionStatus.SUCCESS:
            raise RuntimeError(f"Media conversion failed: {result.status}: {result.errors}")
        result.document.save_as_markdown(output, image_mode=ImageRefMode.PLACEHOLDER)
        text = output.read_text().lower()
        if not text.strip():
            raise RuntimeError("Media conversion produced no transcript")
        if args.probe:
            validate_probe_document(result.document, text, video_probe)
        return result

    convert(args.source, args.output_file)
    if args.probe:
        video = args.output_file.with_suffix(".mp4")
        subprocess.run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-nostdin", "-y",
                        "-f", "lavfi", "-i", "color=c=blue:s=160x120:r=1",
                        "-i", str(args.source), "-shortest", "-c:v", "mpeg4", "-c:a", "aac", str(video)],
                       check=True, timeout=30, stdin=subprocess.DEVNULL)
        convert(video, args.output_file.with_name("video.md"), video_probe=True)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except MemoryError:
        sys.exit(75)
