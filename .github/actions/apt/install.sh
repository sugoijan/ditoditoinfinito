#!/usr/bin/env bash
# The packages in $PACKAGES: from the .deb files in ~/.cache/ddi-apt
# when the Actions cache had them (action.yml), else downloaded with
# Canonical's archive put before the Azure mirror and kept there for the
# cache to save. Run by action.yml; by hand: PACKAGES="nasm" install.sh.
set -euo pipefail

dir=$HOME/.cache/ddi-apt
sudo=sudo
if [ "$(id -u)" = 0 ]; then sudo=; fi
# shellcheck disable=SC2086
set -- $PACKAGES
[ $# -gt 0 ] || { echo "apt: no packages" >&2; exit 1; }

if compgen -G "$dir/*.deb" >/dev/null; then
  echo "apt: $* from the cache ($(find "$dir" -name '*.deb' | wc -l) files)"
  # --auto-deconfigure: the files may upgrade packages the image has, in
  # an order dpkg takes them in by name. A newer libasound2-data that
  # Breaks the image's libasound2t64 was refused before the newer
  # libasound2t64 in the same call was unpacked (2026-11-27, every Linux
  # job); apt passes the same flag.
  if $sudo dpkg -i --auto-deconfigure "$dir"/*.deb; then exit; fi
  # A cache that does not fit the image never fails the job: the
  # packages by name from the archive, as on a miss, mending what dpkg
  # left half installed.
  echo "apt: the cached files did not install; downloading instead" >&2
  rm -f "$dir"/*.deb
  fix=--fix-broken
fi

# The runner image's mirror list: azure.archive.ubuntu.com first
# (actions/runner-images, configure-apt-sources.sh). Canonical's archive
# first instead; the Azure mirror stays as the fallback.
mirrors=/etc/apt/apt-mirrors.txt
if [ -f "$mirrors" ]; then
  printf 'https://archive.ubuntu.com/ubuntu/\tpriority:1\nhttp://azure.archive.ubuntu.com/ubuntu/\tpriority:2\nhttps://security.ubuntu.com/ubuntu/\tpriority:3\n' \
    | $sudo tee "$mirrors" >/dev/null
fi
$sudo apt-get update
$sudo apt-get clean
$sudo apt-get install -y --no-install-recommends ${fix:-} --download-only "$@"
mkdir -p "$dir"
# Nothing to download: every package was installed already.
if compgen -G "/var/cache/apt/archives/*.deb" >/dev/null; then
  cp /var/cache/apt/archives/*.deb "$dir"/
fi
$sudo apt-get install -y --no-install-recommends ${fix:-} "$@"
echo "apt: $* installed, $(find "$dir" -name '*.deb' | wc -l) files kept for the cache"
