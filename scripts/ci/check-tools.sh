#!/usr/bin/env bash
set -euo pipefail
# No installation/network access here: tools are supplied by the versioned CI image.
test "$(uname -s)" = Linux
test "$(uname -m)" = x86_64
test "$(node --version)" = v24.13.0
[[ "$(rustc --version)" == 'rustc 1.99.1 '* ]]
test "$(cargo zigbuild --version)" = 'cargo-zigbuild 0.24.0'
test "$(zig version)" = 0.17.0
printf '54b97fa7b70968c4d05c79840f5daeff508957d0bb2062fdb0376d00d9615c93  %s\n' "${FNPACK:?}" | sha256sum --check
for target in x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu; do
  test -d "$(rustc --print target-libdir --target "$target")"
done
node --version
rustc --version
python3 --version
