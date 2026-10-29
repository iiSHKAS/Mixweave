#!/bin/sh
# Build an app with embedded UI, independent of the Vite development server.
set -eu
project_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$project_dir"
npm run build
cargo build --manifest-path src-tauri/Cargo.toml --features tauri/custom-protocol
mkdir -p target/standalone
cp target/debug/mixweave target/standalone/mixweave
printf 'Standalone application: %s/target/standalone/mixweave\n' "$project_dir"
