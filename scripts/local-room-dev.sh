#!/bin/sh
set -eu
repo_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo_dir"
export OPENCOVIBE_DATA_DIR="$repo_dir/.local-data"
exec npm run tauri -- dev --config src-tauri/tauri.local.conf.json
