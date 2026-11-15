"""Transfer only Cargo dependency outputs; never application/test executables."""
import os
import shutil
import subprocess
import sys
import tarfile
import tempfile
from contextlib import contextmanager
from pathlib import Path, PurePosixPath


@contextmanager
def compressed_stream(archive, packing):
    # Match Actions cache's fast native compressor, without buffering a whole
    # dependency archive in RAM or materializing an extra uncompressed tar.
    with open(archive, "wb" if packing else "rb") as file, tempfile.TemporaryFile() as errors:
        process = subprocess.Popen(["zstd", "-q", "-1", "-T0", "-c"] if packing else ["zstd", "-q", "-d", "-c"],
                                   stdin=subprocess.PIPE if packing else file,
                                   stdout=file if packing else subprocess.PIPE, stderr=errors)
        pipe = process.stdin if packing else process.stdout
        try:
            yield pipe
            if packing:
                pipe.close()
            else:
                while pipe.read(1024 * 1024):
                    pass
            process.wait(timeout=270)
            errors.seek(0)
            assert process.returncode == 0, errors.read(4096).decode(errors="replace")
        finally:
            try:
                pipe.close()
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait()


def allowed(name, kind, target):
    if "\\" in name or ":" in name or any(p in ("", ".", "..") for p in name.split("/")):
        return False
    parts = PurePosixPath(name).parts
    if not parts or PurePosixPath(name).is_absolute() or ".." in parts:
        return False
    prefixes = [("debug",)] if kind == "test" else [("release",), (target, "release")]
    for prefix in prefixes:
        if parts[:len(prefix)] != prefix:
            continue
        rest = parts[len(prefix):]
        if len(rest) >= 3 and rest[0] in (".fingerprint", "build"):
            return not rest[1].startswith("sag-")
        if len(rest) == 2 and rest[0] == "deps":
            return not rest[1].startswith(("sag_", "libsag_")) and Path(rest[1]).suffix in (
                ".rlib", ".rmeta", ".d", ".a", ".lib", ".so", ".dylib", ".dll")
    return False


def transfer(command, directory, archive, kind, target):
    directory = Path(directory).resolve()
    if command == "pack":
        count = 0
        with compressed_stream(archive, True) as stream, tarfile.open(fileobj=stream, mode="w|", dereference=True) as result:
            for parent, dirs, files in os.walk(directory, followlinks=False):
                dirs[:] = [d for d in dirs if not Path(parent, d).is_symlink()]
                for name in files:
                    path = Path(parent, name)
                    relative = path.relative_to(directory).as_posix()
                    if not allowed(relative, kind, target) or not path.is_file():
                        continue
                    actual = path.resolve()
                    if not actual.is_relative_to(directory) or not allowed(actual.relative_to(directory).as_posix(), kind, target):
                        continue
                    result.add(path, arcname=relative, recursive=False)
                    count += 1
        assert count, "No dependency outputs to transfer"
        return count
    assert command == "restore"
    # Stage and validate the complete archive before writing into Cargo's cache.
    with tempfile.TemporaryDirectory(prefix="sag-rust-deps-") as temporary:
        stage = Path(temporary)
        members = []
        seen = set()
        with compressed_stream(archive, False) as stream, tarfile.open(fileobj=stream, mode="r|") as source:
            for member in source:
                assert member.isfile() and allowed(member.name, kind, target), "Unsafe dependency archive entry"
                normalized = os.path.normcase(member.name)
                assert normalized not in seen, "Duplicate dependency archive entry"
                seen.add(normalized)
                source.extract(member, stage, filter="data")
                members.append(member)
        assert members, "Empty dependency archive"
        for member in members:
            destination = directory / member.name
            assert not any(p.is_symlink() for p in [destination, *destination.parents]), "Cache destination contains a symlink"
        for member in members:
            destination = directory / member.name
            destination.parent.mkdir(parents=True, exist_ok=True)
            # An optional restore may be interrupted. Never truncate an existing
            # dependency before its complete replacement is ready on this volume.
            with tempfile.NamedTemporaryFile(dir=destination.parent, prefix=".sag-cache-", delete=False) as temporary:
                replacement = Path(temporary.name)
            try:
                shutil.copy2(stage / member.name, replacement)
                os.replace(replacement, destination)
            finally:
                replacement.unlink(missing_ok=True)
        return len(members)


if __name__ == "__main__":
    print(f"Dependency files: {transfer(*sys.argv[1:])}")
