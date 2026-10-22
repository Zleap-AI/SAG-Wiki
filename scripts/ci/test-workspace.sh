#!/usr/bin/env bash
set -euo pipefail
# Permission-denial tests must exercise normal Unix permissions, not root's bypass.
if [[ "$(id -u)" = 0 ]]; then
  if ! id sag-ci >/dev/null 2>&1; then
    useradd --create-home --user-group --shell /bin/bash sag-ci
  fi
  chown -R --no-dereference sag-ci:sag-ci "${CI_PROJECT_DIR:?}"
  runuser -u sag-ci -- env PATH="$PATH" cargo test --workspace --locked
else
  cargo test --workspace --locked
fi
