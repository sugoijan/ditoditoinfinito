#!/usr/bin/env bash
# Stdout belongs exclusively to MCP. Launch the GUI separately with run.sh.
set -euo pipefail
repo_dir="$(cd "$(dirname "$0")/../.." && pwd)"
export DDI_STUDIO_HOME="${DDI_STUDIO_HOME:-$repo_dir/.local/skincraft}"
export DDI_ASSETS="$repo_dir/assets"
target_dir="${CARGO_TARGET_DIR:-$repo_dir/target/photocraft}"
studio_binary="$target_dir/release/skincraft"
if [[ ! -x "$studio_binary" ]]; then
    studio_binary="$target_dir/debug/skincraft"
fi
if [[ ! -x "$studio_binary" ]]; then
    echo "Build SkinCraft first with tools/skin-studio/run.sh" >&2
    exit 1
fi
exec "$studio_binary" mcp
