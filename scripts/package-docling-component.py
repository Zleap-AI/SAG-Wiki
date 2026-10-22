#!/usr/bin/env python3
"""Assemble a relocatable, hash-verified document runtime from CI-built inputs.

Use an independent CPython distribution and a wheel-only target installation,
not a virtualenv. Inference acceptance is performed by SAG before activation.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import stat
import subprocess
import sys
import tempfile


COMPONENT_VERSION = "2.140.0-documents.1"
PLATFORMS = {"macos-aarch64": "python/bin/python3.12", "windows-x86_64": "python/python.exe"}


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def validate_source(path):
    root = path.resolve(strict=True)
    if not root.is_dir():
        raise ValueError(f"Expected an input directory: {path}")
    for parent, directories, files in os.walk(root, followlinks=False):
        for name in directories + files:
            item = Path(parent) / name
            if item.is_symlink():
                target = item.resolve(strict=True)
                if not target.is_relative_to(root):
                    raise ValueError(f"External symlink is not a component input: {item}")
                if target.is_dir():
                    raise ValueError(f"Directory symlink must be flattened by the runtime builder: {item}")
    return root


def ignored(_directory, names):
    # Target-install entry points contain builder-local interpreter paths. SAG
    # invokes the shipped runner using the component interpreter instead.
    return [name for name in names if name in {"__pycache__", ".DS_Store"} or name.endswith((".pyc", ".pyo"))]


def payload_files(root):
    files = []
    for parent, directories, names in os.walk(root, followlinks=False):
        for name in directories + names:
            path = Path(parent) / name
            mode = path.lstat().st_mode
            if stat.S_ISLNK(mode):
                raise ValueError(f"Component payload contains a symlink: {path}")
            if not (stat.S_ISDIR(mode) or stat.S_ISREG(mode)):
                raise ValueError(f"Component payload contains a nonregular entry: {path}")
            if stat.S_ISREG(mode) and path != root / "manifest.json":
                files.append(path)
    return sorted(files)


def verify(root):
    root = root.resolve(strict=True)
    manifest = json.loads((root / "manifest.json").read_text())
    if manifest.get("schema") != 1 or manifest.get("profile") != "documents":
        raise ValueError("Unsupported document component schema/profile")
    if manifest.get("component_version") != COMPONENT_VERSION or manifest.get("platform") not in PLATFORMS:
        raise ValueError("Unsupported document component version/platform")
    if manifest.get("python") != PLATFORMS[manifest["platform"]]:
        raise ValueError("Unexpected component interpreter")
    seen = set()
    for entry in manifest["files"]:
        relative = PurePosixPath(entry["path"])
        if relative.is_absolute() or ".." in relative.parts or "\\" in entry["path"] or not relative.parts:
            raise ValueError(f"Unsafe component path: {entry['path']}")
        if entry["path"] in seen:
            raise ValueError(f"Duplicate component path: {entry['path']}")
        seen.add(entry["path"])
        path = root.joinpath(*relative.parts)
        if path.is_symlink() or not path.resolve(strict=True).is_relative_to(root):
            raise ValueError(f"Invalid component symlink/path: {entry['path']}")
        if path.stat().st_size != entry["size"] or digest(path) != entry["sha256"]:
            raise ValueError(f"Component checksum mismatch: {entry['path']}")
    actual = {path.relative_to(root).as_posix() for path in payload_files(root)}
    if actual != seen or manifest["python"] not in seen:
        raise ValueError("Component manifest does not cover the complete payload")
    return manifest


def assemble(args):
    if args.output.exists():
        raise ValueError(f"Refusing to replace an existing component: {args.output}")
    sources = [(validate_source(args.python_runtime), "python"),
               (validate_source(args.site_packages), "site-packages"),
               (validate_source(args.models), "models")]
    if (sources[0][0] / "pyvenv.cfg").exists():
        raise ValueError("A virtual environment is not an independent Python runtime")
    if not args.requirements_lock.is_file():
        raise ValueError("The exact requirements lock must exist")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".docling-component-", dir=args.output.parent) as staging:
        root = Path(staging) / "component"
        root.mkdir()
        for source, name in sources:
            shutil.copytree(source, root / name, symlinks=False, ignore=ignored)
        shutil.rmtree(root / "site-packages/bin", ignore_errors=True)
        shutil.copyfile(args.requirements_lock, root / "requirements.txt")
        interpreter = root / PLATFORMS[args.platform]
        if not interpreter.is_file():
            raise ValueError(f"Missing independent interpreter: {interpreter}")
        if args.compile_bytecode:
            # Compile on the target platform using the exact shipped interpreter.
            # Checked hashes tolerate relocation and do not rely on copy mtimes.
            compiled = subprocess.run([
                str(interpreter), "-I", "-B", "-m", "compileall", "-q", "-f", "-j", "2",
                "--invalidation-mode", "checked-hash", "-s", str(root),
                # olefile also distributes a Python 2 compatibility module;
                # Python 3 imports olefile.py, never this legacy source.
                "-x", r"(^|[/\\])olefile[/\\]olefile2[.]py$",
                "-p", "/sag-component", str(root / "python"), str(root / "site-packages"),
            ], capture_output=True, text=True)
            if compiled.returncode:
                raise ValueError(f"Component bytecode compilation failed: {compiled.stdout}\n{compiled.stderr}")
        manifest = {
            "schema": 1, "component_version": COMPONENT_VERSION,
            "profile": "documents", "platform": args.platform,
            "python": PLATFORMS[args.platform], "site_packages": "site-packages", "models": "models",
            "requirements_sha256": digest(root / "requirements.txt"),
            "files": [{"path": path.relative_to(root).as_posix(), "size": path.stat().st_size,
                       "sha256": digest(path)} for path in payload_files(root)],
        }
        (root / "manifest.json").write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n")
        verify(root)
        root.rename(args.output)
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verify", type=Path, help="Verify an assembled component without executing it")
    parser.add_argument("--python-runtime", type=Path)
    parser.add_argument("--site-packages", type=Path)
    parser.add_argument("--models", type=Path)
    parser.add_argument("--requirements-lock", type=Path)
    parser.add_argument("--platform", choices=PLATFORMS)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--compile-bytecode", action="store_true", help="Precompile hash-checked bytecode with the shipped target interpreter")
    args = parser.parse_args()
    if not args.verify and any(getattr(args, field) is None for field in
                               ("python_runtime", "site_packages", "models", "requirements_lock", "platform", "output")):
        parser.error("Provide all component inputs, or use --verify DIRECTORY")
    try:
        manifest = verify(args.verify) if args.verify else assemble(args)
    except (OSError, ValueError, KeyError) as error:
        print(f"Component packaging failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps({"component_version": manifest["component_version"], "platform": manifest["platform"],
                      "files": len(manifest["files"]), "bytes": sum(item["size"] for item in manifest["files"])}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
