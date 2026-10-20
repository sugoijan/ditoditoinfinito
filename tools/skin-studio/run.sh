#!/usr/bin/env bash
# Launch SkinCraft's shared native host with an isolated project profile.
set -euo pipefail
repo_dir="$(cd "$(dirname "$0")/../.." && pwd)"
for editor in photocraft vectorcraft; do
    if [[ ! -f "$(dirname "$repo_dir")/$editor/Cargo.toml" ]]; then
        echo "Missing sibling checkout: $(dirname "$repo_dir")/$editor" >&2
        exit 1
    fi
done
studio_dir="${DDI_STUDIO_HOME:-$repo_dir/.local/skincraft}"
mkdir -p "$studio_dir"
export DDI_STUDIO_HOME="$studio_dir"
export PHOTOCRAFT_CONFIG_DIR="$studio_dir/photocraft"
export DDI_HOME="$studio_dir/game"
export DDI_ASSETS="$repo_dir/assets"
export DDI_STUDIO_OUTPUT="${DDI_STUDIO_OUTPUT:-$studio_dir/export}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$repo_dir/target/photocraft}"
exec cargo run --release --locked --manifest-path "$repo_dir/tools/skin-studio/Cargo.toml" \
    --features skincraft --bin skincraft -- "$@"
