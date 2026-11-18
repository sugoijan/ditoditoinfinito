#!/bin/bash
# Builds the MSD oracle: Etterna's MinaCalc at the pinned commit, by itself,
# as a small command-line program (oracle.cpp) that `ddi-msd`'s opt-in
# comparison feeds (crates/msd/README.md). Fetches only that public commit's
# calculator files (a shallow, sparse Git fetch by hash) into
# target/msd-oracle/, never into the repository.
#
#   tools/msd-oracle/build.sh            # arm64 (or the host's) build
#   tools/msd-oracle/build.sh --x86_64   # an x86_64 build (Rosetta on a Mac)
#
# Prints the oracle's path. The arithmetic is upstream's as written:
# -ffp-contract=off keeps clang from fusing multiplies and adds on arm64,
# as an x86_64 build without FMA never does, and as Rust never does.
set -euo pipefail

COMMIT=2d706147cc4bcf95a8ceb24dc4f4129be207f803
REPO=https://github.com/etternagame/etterna.git

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
out="$root/target/msd-oracle"
src="$out/etterna"
arch_flags=()
name=oracle
if [ "${1:-}" = "--x86_64" ]; then
  arch_flags=(-arch x86_64)
  name=oracle-x86_64
fi

mkdir -p "$out"
if [ ! -d "$src/.git" ] || [ "$(git -C "$src" rev-parse HEAD 2>/dev/null)" != "$COMMIT" ]; then
  rm -rf "$src"
  git init -q "$src"
  git -C "$src" remote add origin "$REPO"
  git -C "$src" config core.sparseCheckout true
  cat > "$src/.git/info/sparse-checkout" <<'EOF'
/LICENSE
/src/Etterna/MinaCalc/
/src/Etterna/Models/NoteData/NoteDataStructures.h
/extern/sse2neon/sse2neon/
EOF
  git -C "$src" fetch -q --depth 1 --filter=blob:none origin "$COMMIT"
  git -C "$src" checkout -q FETCH_HEAD
fi
test "$(git -C "$src" rev-parse HEAD)" = "$COMMIT"

calc="$src/src/Etterna/MinaCalc"
# Upstream's MinaCalc.cpp calls load_calc_params_from_disk, which only the
# full game defines, on its debug/parameter-reload path (never taken here:
# debugmode and loadparams stay false); a copy guards that one call.
sed 's|^\(\t*\)ulbu_in_charge->load_calc_params_from_disk(true);|#if !defined(STANDALONE_CALC)\n&\n#endif|' \
  "$calc/MinaCalc.cpp" > "$out/MinaCalc.oracle.cpp"
test "$(grep -c 'load_calc_params_from_disk' "$out/MinaCalc.oracle.cpp")" = 1
clang++ -std=c++20 -O2 -DNDEBUG -DSTANDALONE_CALC -ffp-contract=off \
  ${arch_flags[@]+"${arch_flags[@]}"} -w \
  -I"$calc" -I"$src/extern/sse2neon/sse2neon" \
  "$here/oracle.cpp" "$out/MinaCalc.oracle.cpp" -o "$out/$name"
echo "$out/$name"
