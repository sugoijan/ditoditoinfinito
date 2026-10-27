#!/usr/bin/env bash
# Deletes the repository's stale Actions caches: of every key prefix (a
# key less its trailing hashes) only the newest entry on main stays, and
# nothing of other refs. The caches must fit the repository's 10 GB cap
# or GitHub evicts whatever was used least recently, which had whole jobs
# start cold; rust-cache saves on main only (`save-if`), and this keeps
# each lock file's or recipe's predecessor from lingering (AGENTS.md,
# "Deployment"). Run by deploy.yml's prune-caches job after the others
# saved. Needs gh with `actions: write` (GH_TOKEN in CI); DRY_RUN=1 only
# says what would go.
set -euo pipefail

gh cache list --limit 500 --json id,key,ref,createdAt,sizeInBytes > caches.json
python3 - <<'PY'
import json, os, re, subprocess
caches = json.load(open("caches.json"))
dry = os.environ.get("DRY_RUN") == "1"
# rust-cache keys end in the environment's hash and the lock file's;
# actions/cache keys here end in one hash of their inputs.
def prefix(key):
    if key.startswith("v0-rust-"):
        return re.sub(r"-[0-9a-f]{8}-[0-9a-f]{8}$", "", key)
    return re.sub(r"-[0-9a-f]{8,}$", "", key)
keep = {}
stale = []
for c in sorted(caches, key=lambda c: c["createdAt"], reverse=True):
    if c["ref"] != "refs/heads/main":
        stale.append(c)
        continue
    p = prefix(c["key"])
    if p in keep:
        stale.append(c)
    else:
        keep[p] = c
mb = lambda c: c["sizeInBytes"] / 1048576
for c in stale:
    if not dry:
        subprocess.run(["gh", "cache", "delete", str(c["id"])], check=True)
    print(f"{'would delete' if dry else 'deleted'} {mb(c):6.0f} MB  {c['ref']}  {c['key']}")
for c in keep.values():
    print(f"kept    {mb(c):6.0f} MB  {c['key']}")
print(f"{len(stale)} deleted, {sum(mb(c) for c in stale):.0f} MB; {len(keep)} kept, {sum(mb(c) for c in keep.values()):.0f} MB")
PY
rm -f caches.json
