#!/usr/bin/env bash
set -euo pipefail

node --version
npm --version
npm config get registry
# Bound the complete install too: DNS, lifecycle scripts, and concurrent fetches
# can outlive npm's per-request timeout. Keep a small process heartbeat in CI.
timeout --signal=TERM --kill-after=10s 10m npm ci --prefer-offline --no-audit --foreground-scripts &
install_pid=$!
while kill -0 "$install_pid" 2>/dev/null; do
  ps -o pid,ppid,stat,etime,pcpu,comm -p "$install_pid" --ppid "$install_pid" || true
  sleep 30
done
wait "$install_pid"
