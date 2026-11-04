#!/usr/bin/env bash
# scripts/update-package-repos.sh against local repositories: an update, no change, a skipped repository, and
# a failure that must not be reported as an update.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

for name in tap bucket; do
  git init --quiet --bare --initial-branch=main "$work/$name.git"
  git clone --quiet "$work/$name.git" "$work/$name" 2>/dev/null
  git -C "$work/$name" -c user.name=t -c user.email=t@t commit --quiet --allow-empty -m init
  git -C "$work/$name" push --quiet origin HEAD:main
done
mkdir "$work/dist"
echo 'class Qlang; end' > "$work/dist/qlang.rb"
echo '{"version": "1.2.3"}' > "$work/dist/qlang.json"

fail() { echo "FAIL: $1" >&2; exit 1; }

out="$(TAP_REPO="$work/tap.git" BUCKET_REPO="$work/bucket.git" "$here/update-package-repos.sh" v1.2.3 "$work/dist")"
echo "$out" | grep -q "tap: updated to 1.2.3" || fail "tap not updated: $out"
echo "$out" | grep -q "bucket: updated to 1.2.3" || fail "bucket not updated: $out"
[ "$(git -C "$work/tap.git" show main:Formula/qlang.rb)" = "class Qlang; end" ] || fail "formula not pushed"
git -C "$work/bucket.git" show main:bucket/qlang.json | grep -q 1.2.3 || fail "manifest not pushed"

out="$(TAP_REPO="$work/tap.git" BUCKET_REPO="$work/bucket.git" "$here/update-package-repos.sh" v1.2.3 "$work/dist")"
echo "$out" | grep -q "tap: already at 1.2.3" || fail "a second run should change nothing: $out"

out="$(TAP_REPO="$work/tap.git" "$here/update-package-repos.sh" v1.2.3 "$work/dist")"
echo "$out" | grep -q "bucket: BUCKET_REPO not set, skipped" || fail "an unset repository is skipped: $out"

if TAP_REPO="$work/missing.git" "$here/update-package-repos.sh" v1.2.3 "$work/dist" > "$work/failed.txt" 2>&1; then
  fail "an unreachable repository must make the script fail"
fi
grep -q "updated" "$work/failed.txt" && fail "a failure must not say updated"
echo "update-package-repos: ok"
