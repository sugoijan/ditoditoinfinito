# Shared by video/build.sh (the web's wasm module) and video/build-native.sh
# (the desktop helper's static FFmpeg): reading video/FFMPEG.toml, checked
# downloads and unpacking. Sourced, not run.

say() { printf 'video: %s\n' "$*"; }
die() { printf 'video: %s\n' "$*" >&2; exit 1; }

# `key = "value"` from FFMPEG.toml ($toml).
value() {
  local v
  v=$(sed -n "s/^$1 = \"\(.*\)\"\$/\1/p" "$toml")
  [ -n "$v" ] || die "$1 missing from $toml"
  printf '%s\n' "$v"
}

# The strings of the array `key = [ ... ]` in FFMPEG.toml, one per line
# (several may share a line; none contains a quote).
array() {
  sed -n "/^$1 = \[/,/^\]/p" "$toml" | sed '1s/^[^[]*\[//' | grep -o '"[^"]*"' | tr -d '"'
}

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

# Downloads `url` to `dest` unless it is there with the right hash.
fetch() {
  local url=$1 dest=$2 want=$3 got
  if [ -f "$dest" ] && [ "$(sha256 "$dest")" = "$want" ]; then return; fi
  say "downloading $url"
  curl -fL --retry 3 --progress-bar -o "$dest.part" "$url"
  got=$(sha256 "$dest.part")
  if [ "$got" != "$want" ]; then
    rm -f "$dest.part"
    die "SHA-256 mismatch for $url: got $got, FFMPEG.toml pins $want"
  fi
  mv "$dest.part" "$dest"
}

# Unpacks `tarball` into `dir` (its single top-level directory) once per hash.
unpack() {
  local tarball=$1 dir=$2 want=$3
  if [ -f "$dir/.ddi-sha256" ] && [ "$(cat "$dir/.ddi-sha256")" = "$want" ]; then return; fi
  say "unpacking $(basename "$tarball")"
  rm -rf "$dir"
  tar -xf "$tarball" -C "$(dirname "$dir")"
  [ -d "$dir" ] || die "$(basename "$tarball") did not unpack to $dir"
  printf '%s\n' "$want" > "$dir/.ddi-sha256"
}

# A JSON string literal of $1.
json_string() { printf '"%s"' "$(printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g')"; }
