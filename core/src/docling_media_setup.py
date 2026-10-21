"""Explicit media installation only; conversion never calls this downloader."""
import argparse
import hashlib
import os
from pathlib import Path
import shutil
import urllib.request

MLX_REPOSITORY = "mlx-community/whisper-tiny-mlx"
MLX_REVISION = "6caf9c55601caafbe6508a8b0d216bdf4783c4e8"
TINY_SHA256 = "65147644a518d12f04e32d6f3b26facc3f8dd46e5390956a9424a650c0ce22b9"
TINY_URL = f"https://openaipublic.azureedge.net/main/whisper/models/{TINY_SHA256}/tiny.pt"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--download-media-models", choices=["mlx", "native"], required=True)
    parser.add_argument("root", type=Path)
    args = parser.parse_args()
    models = args.root / "models"
    models.mkdir(parents=True, exist_ok=True)
    import imageio_ffmpeg
    from imageio_ffmpeg._definitions import FNAME_PER_PLATFORM, get_platform
    binary = args.root / "bin" / ("ffmpeg.exe" if os.name == "nt" else "ffmpeg")
    binary.parent.mkdir(exist_ok=True)
    # The public selector honors environment/Conda/system overrides. Installation
    # must take the audited wheel asset, independently of the caller's environment.
    bundled = Path(imageio_ffmpeg.__file__).parent / "binaries" / FNAME_PER_PLATFORM[get_platform()]
    shutil.copyfile(bundled, binary)
    binary.chmod(0o755)
    if args.download_media_models == "mlx":
        from huggingface_hub import snapshot_download
        snapshot_download(MLX_REPOSITORY, revision=MLX_REVISION,
                          allow_patterns=["config.json", "weights.npz"],
                          local_dir=models / "whisper-tiny-mlx")
    else:
        destination = models / "tiny.pt"
        if not destination.is_file() or hashlib.file_digest(destination.open("rb"), "sha256").hexdigest() != TINY_SHA256:
            temporary = destination.with_suffix(".download")
            try:
                with urllib.request.urlopen(TINY_URL, timeout=30) as response, temporary.open("wb") as output:
                    digest = hashlib.sha256()
                    total = 0
                    while block := response.read(1024 * 1024):
                        total += len(block)
                        if total > 100 * 1024 * 1024:
                            raise ValueError("Whisper checkpoint exceeds its download limit")
                        digest.update(block)
                        output.write(block)
                    if digest.hexdigest() != TINY_SHA256:
                        raise ValueError("Whisper checkpoint checksum mismatch")
                temporary.replace(destination)
            finally:
                temporary.unlink(missing_ok=True)
    print("Media ASR model and private FFmpeg prepared", flush=True)


if __name__ == "__main__":
    main()
