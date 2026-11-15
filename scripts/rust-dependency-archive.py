"""Transfer only Cargo dependency outputs; never application/test executables."""
import os
import shutil
import sys
import tarfile
import tempfile
from pathlib import Path, PurePosixPath


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
        with tarfile.open(archive, "w:gz", compresslevel=1, dereference=True) as result:
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
        with tarfile.open(archive, "r:gz") as source:
            members = source.getmembers()
            assert members and all(m.isfile() and allowed(m.name, kind, target) for m in members), "Unsafe dependency archive entry"
            assert len({os.path.normcase(m.name) for m in members}) == len(members), "Duplicate dependency archive entry"
            source.extractall(stage, members=members, filter="data")
        for member in members:
            destination = directory / member.name
            assert not any(p.is_symlink() for p in [destination, *destination.parents]), "Cache destination contains a symlink"
        for member in members:
            destination = directory / member.name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(stage / member.name, destination)
        return len(members)


if __name__ == "__main__":
    print(f"Dependency files: {transfer(*sys.argv[1:])}")
