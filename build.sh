#!/usr/bin/env sh
set -eu
cd "$(dirname "$0")"
cargo build --release --locked
printf '\nBuilt Aura Launcher:\n  %s\n\nRun it with:\n  %s\n' \
  "$PWD/target/release/aura" "$PWD/target/release/aura"
