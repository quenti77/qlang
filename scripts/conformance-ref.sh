#!/usr/bin/env bash
# Which version of the conformance cases (quenti77/qlang-conformance) a CI run tests against.
#
#   scripts/conformance-ref.sh              prints the ref to clone
#   scripts/conformance-ref.sh --check-tag  fails unless conformance.ref is a tag of that repository
#
# A pull request whose branch has the same name in qlang-conformance (`feat/generic_statics` in both) is tested
# against that branch: the cases of a new feature are written beside it and need no tag before the feature is
# done. Anything else is tested against the tag in conformance.ref, which a push to main must name (a branch
# there would mean a feature went in with cases that are not released).
set -euo pipefail

repo="${CONFORMANCE_REPO:-https://github.com/quenti77/qlang-conformance.git}"
pinned="$(tr -d '[:space:]' < conformance.ref)"

if [ "${1:-}" = "--check-tag" ]; then
  if git ls-remote --exit-code --tags "$repo" "refs/tags/$pinned" > /dev/null 2>&1; then
    echo "conformance.ref ($pinned) is a tag"
  else
    echo "conformance.ref says '$pinned', which is not a tag of $repo: tag the cases, then put the tag here" >&2
    exit 1
  fi
  exit 0
fi

branch="${GITHUB_HEAD_REF:-}"
if [ -n "$branch" ] && git ls-remote --exit-code --heads "$repo" "$branch" > /dev/null 2>&1; then
  echo "conformance: the branch $branch of the cases" >&2
  echo "$branch"
else
  echo "conformance: the tag $pinned" >&2
  echo "$pinned"
fi
