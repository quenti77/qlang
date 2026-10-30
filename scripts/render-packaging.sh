#!/usr/bin/env bash
# Fill the package-manager templates of packaging/ for a release.
#
#   scripts/render-packaging.sh <version> <SHA256SUMS> <out-dir>
#
# Writes qlang.rb (Homebrew), qlang.json (Scoop) and PKGBUILD (AUR) in <out-dir>. They are
# attached to the release; copy them into the tap / bucket / AUR repository.
set -euo pipefail

version="${1:?usage: render-packaging.sh <version> <SHA256SUMS> <out-dir>}"
sums="${2:?usage: render-packaging.sh <version> <SHA256SUMS> <out-dir>}"
out="${3:?usage: render-packaging.sh <version> <SHA256SUMS> <out-dir>}"
root="$(cd "$(dirname "$0")/.." && pwd)"
mkdir -p "$out"

sed_args=(-e "s/@VERSION@/${version#v}/g")
for target in aarch64-apple-darwin x86_64-apple-darwin aarch64-unknown-linux-musl \
              x86_64-unknown-linux-musl x86_64-pc-windows-msvc; do
  ext="tar.gz"; case "$target" in *windows*) ext="zip" ;; esac
  sum="$(grep "  qlang-$target.$ext\$" "$sums" | cut -d ' ' -f 1 || true)"
  [ -n "$sum" ] || { echo "no checksum for qlang-$target.$ext in $sums" >&2; exit 1; }
  key="$(echo "$target" | tr 'a-z-' 'A-Z_')"
  sed_args+=(-e "s/@SHA_${key}@/$sum/g")
done

sed "${sed_args[@]}" "$root/packaging/qlang.rb.in" > "$out/qlang.rb"
sed "${sed_args[@]}" "$root/packaging/qlang.scoop.json.in" > "$out/qlang.json"
sed "${sed_args[@]}" "$root/packaging/PKGBUILD.in" > "$out/PKGBUILD"
echo "rendered $out/qlang.rb $out/qlang.json $out/PKGBUILD"
