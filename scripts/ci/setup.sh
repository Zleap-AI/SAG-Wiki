#!/usr/bin/env bash
set -euo pipefail

# Bootstrap a Linux amd64 runner; an existing fixed Rust toolchain is reused.
test "$(uname -s)" = Linux
test "$(uname -m)" = x86_64
mkdir -p "${CARGO_HOME:?}"
python3 - <<'PY'
import json
import os
from pathlib import Path
mirror = os.environ.get('CARGO_REGISTRY_MIRROR', 'sparse+https://rsproxy.cn/index/')
config = Path(os.environ['CARGO_HOME']) / 'config.toml'
config.write_text('[source.crates-io]\nreplace-with = "ci-mirror"\n[source.ci-mirror]\nregistry = ' + json.dumps(mirror) + '\n' if mirror else '')
PY
if [[ "${SAG_FPK_TOOLCHAIN_IMAGE:-}" = 1 ]]; then
  bash scripts/ci/check-tools.sh
  return 0
fi
export DEBIAN_FRONTEND=noninteractive
python3 - <<'PY'
import os
from pathlib import Path
mirror = os.environ.get('DEBIAN_MIRROR', 'https://mirrors.aliyun.com/debian').rstrip('/')
for path in [Path('/etc/apt/sources.list'), *Path('/etc/apt/sources.list.d').glob('*')]:
    if path.is_file() and path.suffix in {'.list', '.sources'}:
        original = path.read_text()
        updated = original.replace('http://deb.debian.org/debian', mirror).replace('https://deb.debian.org/debian', mirror)
        if updated != original:
            path.write_text(updated)
PY
apt_options=(-o Acquire::Retries=3 -o Acquire::http::Timeout=30 -o Acquire::https::Timeout=30)
apt-get "${apt_options[@]}" update -qq
apt-get "${apt_options[@]}" install -y --no-install-recommends ca-certificates curl xz-utils python3 python3-venv build-essential pkg-config libssl-dev cmake passwd util-linux

export RUSTUP_HOME="${RUSTUP_HOME:-${CI_PROJECT_DIR:?}/.rustup-ci}"
tools_dir="${CI_PROJECT_DIR:?}/.ci-tools"
mkdir -p "$tools_dir" "$CARGO_HOME/bin"
if [[ ! -x "$tools_dir/node-v24.13.0-linux-x64/bin/node" ]]; then
  node_archive="$tools_dir/node.tar.xz"
  curl --fail --location --retry 3 --silent --show-error \
    https://nodejs.org/dist/v24.13.0/node-v24.13.0-linux-x64.tar.xz -o "$node_archive"
  echo "e798599612f4bb71333a3397ab0d095fd62214e115aea45aa858a145fc72d67e  $node_archive" | sha256sum --check
  tar -xJf "$node_archive" -C "$tools_dir"
fi
export PATH="$tools_dir/node-v24.13.0-linux-x64/bin:$CARGO_HOME/bin:$PATH"
node --version
if ! command -v rustup >/dev/null; then
  rustup_root="${RUSTUP_UPDATE_ROOT:-https://rsproxy.cn/rustup}"
  curl --fail --location --retry 3 --connect-timeout 20 --max-time 300 --silent --show-error \
    "$rustup_root/archive/1.29.2/x86_64-unknown-linux-gnu/rustup-init" -o "$tools_dir/rustup-init"
  echo "5be191e8c70a5007074254cb74d78dbfdd25cd5a61c27de7b8e5410122ec24f4  $tools_dir/rustup-init" | sha256sum --check
  chmod 755 "$tools_dir/rustup-init"
  "$tools_dir/rustup-init" -y --no-modify-path --profile minimal --default-toolchain 1.99.1
fi
rustc --version

if [[ "${1:-}" == build ]]; then
  python3 -m venv "$tools_dir/python"
  # Linux amd64 wheel hash independently checked against the PyPI release.
  printf '%s\n' 'ziglang==0.17.0 --hash=sha256:97079827064a7492cce5541ceb438ae8003ce1e54e399676da2d62fb7a5dc08b' > "$tools_dir/build-requirements.txt"
  "$tools_dir/python/bin/pip" install --disable-pip-version-check --index-url "${PIP_INDEX_URL:-https://mirrors.aliyun.com/pypi/simple/}" --require-hashes -r "$tools_dir/build-requirements.txt"
  zig_binary=$("$tools_dir/python/bin/python" -c 'from pathlib import Path; import ziglang; print(Path(ziglang.__file__).parent / "zig")')
  ln -sf "$zig_binary" "$tools_dir/python/bin/zig"
  export PATH="$tools_dir/python/bin:$PATH"
  cargo install cargo-zigbuild --version 0.24.0 --locked
  rustup target add x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu
  mkdir -p tools
  curl --fail --location --retry 3 --silent --show-error \
    https://static2.fnnas.com/fnpack/fnpack-1.2.3-linux-amd64 -o tools/fnpack
  echo '54b97fa7b70968c4d05c79840f5daeff508957d0bb2062fdb0376d00d9615c93  tools/fnpack' | sha256sum --check
  chmod 755 tools/fnpack
fi
