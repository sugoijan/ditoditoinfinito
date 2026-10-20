#!/usr/bin/env bash
# Launch the optional PhotoCraft development build, with its own profile.
set -euo pipefail
repo_dir="$(cd "$(dirname "$0")/../.." && pwd)"
photocraft_dir="${PHOTOCRAFT_SOURCE:-$(dirname "$repo_dir")/photocraft}"
studio_dir="${DDI_STUDIO_HOME:-$repo_dir/.local/skin-studio}"
if [[ ! -f "$photocraft_dir/apps/photocraft/Cargo.toml" ]]; then
    echo "PhotoCraft sibling checkout not found: $photocraft_dir" >&2
    exit 1
fi
mkdir -p "$studio_dir"
export DDI_STUDIO_HOME="$studio_dir"
export PHOTOCRAFT_CONFIG_DIR="$studio_dir/photocraft"
export DDI_HOME="$studio_dir/game"
export DDI_ASSETS="$repo_dir/assets"
export DDI_STUDIO_OUTPUT="${DDI_STUDIO_OUTPUT:-$studio_dir/export}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$repo_dir/target/photocraft}"
exec cargo run --manifest-path "$photocraft_dir/Cargo.toml" -p photocraft --features noteskin-preview -- --new-instance "$@"
