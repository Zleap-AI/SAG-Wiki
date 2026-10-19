#!/bin/sh
set -eu
installer_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
work=''
cleanup() { if [ -n "$work" ]; then rm -rf "$work"; fi; }
trap cleanup EXIT HUP INT TERM
if [ -n "${SAG_BINARY:-}" ]; then
    binary=$SAG_BINARY
elif [ -n "${SAG_RELEASE_BASE_URL:-}" ]; then
    case "$(uname -s)/$(uname -m)" in
        Darwin/arm64) target=aarch64-apple-darwin ;;
        Darwin/x86_64) target=x86_64-apple-darwin ;;
        Linux/x86_64) target=x86_64-unknown-linux-gnu ;;
        Linux/aarch64) target=aarch64-unknown-linux-gnu ;;
        *) echo 'Unsupported platform. Use a matching sag binary.' >&2; exit 1 ;;
    esac
    archive="sag-${SAG_VERSION:-0.1.0}-$target.tar.gz"
    work=$(mktemp -d)
    curl -fsSL "$SAG_RELEASE_BASE_URL/$archive" -o "$work/$archive"
    curl -fsSL "$SAG_RELEASE_BASE_URL/$archive.sha256" -o "$work/$archive.sha256"
    (cd "$work"; if command -v sha256sum >/dev/null 2>&1; then sha256sum -c "$archive.sha256"; else shasum -a 256 -c "$archive.sha256"; fi)
    tar -xzf "$work/$archive" -C "$work"
    binary="$work/sag"
elif [ -f "$installer_dir/sag" ]; then
    binary="$installer_dir/sag"
elif [ -f "$installer_dir/../cli/Cargo.toml" ]; then
    command -v cargo >/dev/null 2>&1 || { echo 'Source installation requires Rust. Use the prebuilt release for installation without Rust.' >&2; exit 1; }
    cargo build --release --locked --manifest-path "$installer_dir/../cli/Cargo.toml"
    binary="$installer_dir/../cli/target/release/sag"
else
    echo 'Set SAG_RELEASE_BASE_URL to the published release, or run this installer beside the sag binary.' >&2
    exit 1
fi
"$binary" install cli
if [ "$#" -gt 0 ]; then
    "$binary" config --if-needed "$@"
elif [ -t 1 ] && [ -t 2 ] && [ -r /dev/tty ]; then
    "$binary" config --if-needed </dev/tty
else
    printf '%s\n' 'Open a terminal and run sag to complete first-use setup.'
fi
printf '%s\n' 'Installed sag. Open a new terminal to use it.'
