#!/bin/sh
# Adds Dito Dito Infinito to this user's applications menu, run from where
# the folder is kept (moving the folder afterwards needs a second run);
# --remove takes it out again. Nothing outside $XDG_DATA_HOME is touched.
set -eu
id=dev.sugoijan.ditoditoinfinito
here=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
data=${XDG_DATA_HOME:-$HOME/.local/share}
entry=$data/applications/$id.desktop
icon=$data/icons/hicolor/scalable/apps/$id.svg
if [ "${1-}" = --remove ]; then
    rm -f "$entry" "$icon"
    echo "Removed $entry"
    exit 0
fi
mkdir -p "$(dirname -- "$entry")" "$(dirname -- "$icon")"
cp "$here/share/icons/hicolor/scalable/apps/$id.svg" "$icon"
# The Desktop Entry specification's Exec quoting (\ " ` $ escaped inside
# quotes), then its string escape (every \ doubled) and % as %%; awk takes
# the path from the environment, so no character is special to it.
EXE=$(printf '%s' "$here/ddi" | sed -e 's/[\\"`$]/\\&/g' -e 's/\\/\\\\/g' -e 's/%/%%/g') \
    awk '/^Exec=/ { print "Exec=\"" ENVIRON["EXE"] "\""; next } { print }' \
    "$here/share/applications/$id.desktop" > "$entry"
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$(dirname -- "$entry")" || true
fi
echo "Installed $entry"
