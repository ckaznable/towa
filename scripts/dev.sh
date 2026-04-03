#!/usr/bin/env zsh
set -euo pipefail

cargo run &
api_pid=$!

cleanup() {
  kill "$api_pid" >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

cd web
npm run dev -- --host 127.0.0.1
