#!/bin/sh
# Install or update qlang (https://github.com/quenti77/qlang).
#
#   curl -fsSL https://raw.githubusercontent.com/quenti77/qlang/main/install.sh | sh
#   curl -fsSL .../install.sh | sh -s -- --version 0.1.0 --vscode
#
# Running it again updates qlang to the latest release.
#
# Options:
#   --version X      install this version instead of the latest one
#   --dir DIR        install into DIR (default: $HOME/.local/bin, or $QLANG_INSTALL_DIR)
#   --vscode         also install the VS Code extension (needs the `code` command)
#   --profile NAME   VS Code profile for the extension
#   --help
#
# Environment: QLANG_BASE_URL overrides https://github.com/quenti77/qlang/releases
# (for a mirror, or for tests).
set -eu

repo="quenti77/qlang"
base_url="${QLANG_BASE_URL:-https://github.com/$repo/releases}"
dir="${QLANG_INSTALL_DIR:-$HOME/.local/bin}"
version="latest"
want_vscode=0
profile=""

say() { printf '%s\n' "$*"; }
usage() {
  cat <<'USAGE'
Install or update qlang from the latest GitHub release.

  curl -fsSL https://raw.githubusercontent.com/quenti77/qlang/main/install.sh | sh
  curl -fsSL https://raw.githubusercontent.com/quenti77/qlang/main/install.sh | sh -s -- --vscode

Running it again updates qlang.

Options:
  --version X      install this version instead of the latest one
  --dir DIR        install into DIR (default: $HOME/.local/bin, or $QLANG_INSTALL_DIR)
  --vscode         also install the VS Code extension (needs the `code` command)
  --profile NAME   VS Code profile for the extension
  --help
USAGE
}
fail() { printf 'qlang install: %s\n' "$*" >&2; exit 1; }

while [ $# -gt 0 ]; do
  case "$1" in
    --version) [ $# -ge 2 ] || fail "--version needs a value"; version="$2"; shift 2 ;;
    --dir) [ $# -ge 2 ] || fail "--dir needs a value"; dir="$2"; shift 2 ;;
    --vscode) want_vscode=1; shift ;;
    --profile) [ $# -ge 2 ] || fail "--profile needs a value"; profile="$2"; shift 2 ;;
    --help|-h) usage; exit 0 ;;
    *) fail "unknown option: $1 (try --help)" ;;
  esac
done

# --- which build?
os="$(uname -s)"
arch="$(uname -m)"
case "$os/$arch" in
  Linux/x86_64)            target="x86_64-unknown-linux-musl" ;;
  Linux/aarch64|Linux/arm64) target="aarch64-unknown-linux-musl" ;;
  Darwin/arm64)            target="aarch64-apple-darwin" ;;
  Darwin/x86_64)           target="x86_64-apple-darwin" ;;
  *) fail "no prebuilt qlang for $os/$arch. Build it from source: cargo install --git https://github.com/$repo qlang-cli" ;;
esac

if [ "$version" = "latest" ]; then
  url="$base_url/latest/download"
else
  url="$base_url/download/v${version#v}"
fi

# --- download tools
if command -v curl >/dev/null 2>&1; then
  download() { curl -fsSL "$1" -o "$2"; }
elif command -v wget >/dev/null 2>&1; then
  download() { wget -q "$1" -O "$2"; }
else
  fail "curl or wget is needed"
fi

if command -v sha256sum >/dev/null 2>&1; then
  sha256() { sha256sum "$1" | cut -d ' ' -f 1; }
elif command -v shasum >/dev/null 2>&1; then
  sha256() { shasum -a 256 "$1" | cut -d ' ' -f 1; }
else
  fail "sha256sum or shasum is needed to check the download"
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

fetch_checked() {
  # fetch_checked <file name>: download it next to the checksums and verify it
  name="$1"
  download "$url/$name" "$tmp/$name" || fail "cannot download $url/$name"
  expected="$(grep "  $name\$" "$tmp/SHA256SUMS" | cut -d ' ' -f 1 || true)"
  [ -n "$expected" ] || fail "$name is not listed in SHA256SUMS"
  actual="$(sha256 "$tmp/$name")"
  [ "$expected" = "$actual" ] || fail "checksum mismatch for $name (expected $expected, got $actual)"
}

previous=""
if [ -x "$dir/qlang" ]; then
  previous="$("$dir/qlang" --version 2>/dev/null || true)"
fi

say "Downloading qlang ($version) for $target..."
download "$url/SHA256SUMS" "$tmp/SHA256SUMS" || fail "cannot download $url/SHA256SUMS (is there a release $version?)"
archive="qlang-$target.tar.gz"
fetch_checked "$archive"

tar -xzf "$tmp/$archive" -C "$tmp"
[ -f "$tmp/qlang/qlang" ] || fail "the archive does not contain qlang"

mkdir -p "$dir"
# copy then rename, so a running qlang is replaced safely
cp "$tmp/qlang/qlang" "$dir/.qlang.new"
chmod 755 "$dir/.qlang.new"
mv -f "$dir/.qlang.new" "$dir/qlang"

installed="$("$dir/qlang" --version 2>/dev/null || true)"
[ -n "$installed" ] || fail "the installed program does not run"
if [ -n "$previous" ] && [ "$previous" != "$installed" ]; then
  say "Updated: $previous -> $installed ($dir/qlang)"
elif [ -n "$previous" ]; then
  say "Already up to date: $installed ($dir/qlang)"
else
  say "Installed: $installed ($dir/qlang)"
fi

case ":$PATH:" in
  *":$dir:"*) ;;
  *) say "Note: $dir is not in your PATH. Add this to your shell profile:"
     say "  export PATH=\"$dir:\$PATH\"" ;;
esac

# --- the VS Code extension
if [ "$want_vscode" = 1 ]; then
  code_cmd=""
  for c in code code-insiders codium; do
    if command -v "$c" >/dev/null 2>&1; then code_cmd="$c"; break; fi
  done
  [ -n "$code_cmd" ] || fail "the \`code\` command was not found. In VS Code: Command Palette > 'Shell Command: Install code command in PATH'"
  fetch_checked "qlang.vsix"
  if [ -n "$profile" ]; then
    "$code_cmd" --profile "$profile" --install-extension "$tmp/qlang.vsix" --force
  else
    "$code_cmd" --install-extension "$tmp/qlang.vsix" --force
  fi
  say "VS Code extension installed. Reload the window to use it."
fi
