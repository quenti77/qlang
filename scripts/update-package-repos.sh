#!/usr/bin/env bash
# Push the Homebrew formula and the Scoop manifest of a release to their repositories.
#
#   TAP_REPO=<git url> BUCKET_REPO=<git url> scripts/update-package-repos.sh <version> <dir>
#
# <dir> holds the files of the release (`qlang.rb`, `qlang.json`). A repository whose variable is not set is
# skipped. The URLs may carry a token (https://x-access-token:TOKEN@github.com/owner/repo) or be ssh/file URLs.
# Nothing is committed when the files did not change.
set -euo pipefail

version="${1:?usage: update-package-repos.sh <version> <dir>}"
dir="${2:?usage: update-package-repos.sh <version> <dir>}"
version="${version#v}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

push() {
  local url="$1" source="$2" target="$3" name="$4"
  git clone --quiet --depth 1 "$url" "$work/$name"
  mkdir -p "$(dirname "$work/$name/$target")"
  cp "$dir/$source" "$work/$name/$target"
  git -C "$work/$name" add "$target"
  if git -C "$work/$name" diff --cached --quiet; then
    echo "$name: already at $version"
    return
  fi
  git -C "$work/$name" -c user.name="qlang release" -c user.email="release@users.noreply.github.com" \
    commit --quiet -m "qlang $version"
  git -C "$work/$name" push --quiet origin HEAD
  echo "$name: updated to $version"
}

# a failure (clone, push) stops the script with a non-zero status: it must not look like an update
if [ -n "${TAP_REPO:-}" ]; then
  push "$TAP_REPO" qlang.rb Formula/qlang.rb tap
else
  echo "tap: TAP_REPO not set, skipped"
fi
if [ -n "${BUCKET_REPO:-}" ]; then
  push "$BUCKET_REPO" qlang.json bucket/qlang.json bucket
else
  echo "bucket: BUCKET_REPO not set, skipped"
fi
