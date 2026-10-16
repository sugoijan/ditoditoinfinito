#!/bin/sh
# Maintainer build only. The installed runtime never calls this script.
set -eu
if [ "$#" -ne 3 ]; then
    printf '%s\n' 'Usage: build-native-runtime.sh PINNED_RUFFLE_GIT NEW_BUILD_DIR NEW_INSTALL_PREFIX' >&2
    exit 2
fi
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
ruffle_git=$1
build_dir=$2
install_prefix=$3
case $(rustc --version) in
    'rustc 1.99.'*) ;;
    *) printf '%s\n' 'Select Rust 1.99 before building.' >&2; exit 2 ;;
esac
for tool in cargo git java cc cmake pkg-config python3; do
    command -v "$tool" >/dev/null 2>&1 || {
        printf 'Missing maintainer build tool: %s\n' "$tool" >&2
        exit 2
    }
done
# Deliberately no package-manager commands or root/user setup here.
mkdir -- "$build_dir"
build_dir=$(CDPATH= cd -- "$build_dir" && pwd)
manifest=$script_dir/rust/Cargo.toml
export CARGO_TARGET_DIR="$build_dir/sdk-target"
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}
# Fetch all locked targets for the deliberately overinclusive notice inventory.
# CARGO_NET_OFFLINE=true remains supported for a fully seeded public cache.
cargo fetch --locked --manifest-path "$manifest"
cargo build --locked --release --manifest-path "$manifest"
bin=$CARGO_TARGET_DIR/release/ddi-danoni-export
"$bin" prepare-worker "$ruffle_git" "$build_dir/sources"
cargo fetch --locked --manifest-path "$build_dir/sources/worker/Cargo.toml"
CARGO_TARGET_DIR="$build_dir/worker-target" cargo build --locked --release \
    --manifest-path "$build_dir/sources/worker/Cargo.toml"
python3 - "$build_dir" <<'PY'
from pathlib import Path
import hashlib, json, sys
root = Path(sys.argv[1])
exe = root / 'worker-target/release/ddi-ruffle-probe'
job = {'schema': 'ddi-danoni-worker-package-job/1', 'sources': str(root / 'sources'),
       'executable': str(exe), 'executableSha256': hashlib.sha256(exe.read_bytes()).hexdigest()}
with (root / 'package-worker.json').open('x') as stream:
    json.dump(job, stream, indent=2)
PY
"$bin" package-worker "$build_dir/package-worker.json" "$build_dir/worker-bundle"
target=$(rustc -vV | sed -n 's/^host: //p')
python3 "$script_dir/collect-runtime-notices.py" "$manifest" \
    "$build_dir/sources/worker/Cargo.toml" "$build_dir/notices" --target "$target"
"$bin" install-runtime "$build_dir/worker-bundle" "$build_dir/notices" "$install_prefix"
"$install_prefix/bin/ddi-danoni-export" verify-runtime "$install_prefix"
"$install_prefix/bin/ddi-danoni-export" diagnose-runtime "$install_prefix" "$build_dir/capabilities"
