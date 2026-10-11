#!/usr/bin/env bash
# Package a built `qlang` binary for a release.
#
#   scripts/package-release.sh <target-triple>
#
# Reads target/<target>/release/qlang (qlang.exe on Windows) and writes
# dist/qlang-<target>.tar.gz (dist/qlang-<target>.zip on Windows). The name has no
# version so that `releases/latest/download/<name>` always points to the newest one.
set -euo pipefail

target="${1:?usage: package-release.sh <target-triple>}"
case "$target" in
  *windows*) exe="qlang.exe"; kind="zip" ;;
  *)         exe="qlang";     kind="tar.gz" ;;
esac

bin="${BIN:-target/$target/release/$exe}"
[ -f "$bin" ] || { echo "binary not found: $bin" >&2; exit 1; }

mkdir -p dist
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
mkdir "$stage/qlang"
cp "$bin" "$stage/qlang/$exe"
cp LICENSE README.md "$stage/qlang/"

archive="qlang-$target.$kind"
out="$PWD/dist/$archive"
rm -f "$out"
if [ "$kind" = "zip" ]; then
  # 7z is available on GitHub's Windows runners; `zip` is not
  (cd "$stage" && 7z a -tzip -bso0 "$out" qlang)
else
  tar -C "$stage" -czf "$out" qlang
fi
echo "packaged $out"
