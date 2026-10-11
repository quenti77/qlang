#!/usr/bin/env bash
# Test install.sh against a fake GitHub release built from a local qlang binary.
#
#   cargo build --release && scripts/test-install.sh
#
# Needs target/release/qlang (or BIN=/path/to/qlang). Linux and macOS.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
bin="${BIN:-$root/target/release/qlang}"
[ -x "$bin" ] || { echo "build first: cargo build --release (missing $bin)" >&2; exit 1; }

case "$(uname -s)/$(uname -m)" in
  Linux/x86_64) target="x86_64-unknown-linux-musl" ;;
  Linux/aarch64|Linux/arm64) target="aarch64-unknown-linux-musl" ;;
  Darwin/arm64) target="aarch64-apple-darwin" ;;
  Darwin/x86_64) target="x86_64-apple-darwin" ;;
  *) echo "unsupported platform for this test" >&2; exit 1 ;;
esac

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
fails=0
pass() { printf '  ok   %s\n' "$1"; }
bad() { printf '  FAIL %s\n' "$1"; fails=$((fails + 1)); }
expect() { # expect <description> <command...>: the command must succeed
  desc="$1"; shift
  if "$@" >/dev/null 2>&1; then pass "$desc"; else bad "$desc"; fi
}
expect_out() { # expect_out <description> <pattern> <file>
  if grep -q -- "$2" "$3" 2>/dev/null; then pass "$1"; else bad "$1 (wanted: $2)"; sed 's/^/       | /' "$3" 2>/dev/null || true; fi
}

# --- a fake release: the same files the release workflow publishes
make_release() { # make_release <dir>
  rel="$1"; mkdir -p "$rel"
  (cd "$work" && rm -rf stage dist && BIN="$bin" bash "$root/scripts/package-release.sh" "$target" >/dev/null 2>&1) \
    || (cd "$root" && BIN="$bin" bash scripts/package-release.sh "$target" >/dev/null)
  cp "$root/dist/qlang-$target.tar.gz" "$rel/"
  printf 'not really a vsix\n' > "$rel/qlang.vsix"
  (cd "$rel" && if command -v sha256sum >/dev/null; then sha256sum ./* ; else shasum -a 256 ./*; fi \
     | sed 's# \./# #' > SHA256SUMS)
}
cd "$root"
make_release "$work/fake/latest/download"
cp -r "$work/fake/latest/download" "$work/fake/v-copy"
mkdir -p "$work/fake/download" && mv "$work/fake/v-copy" "$work/fake/download/v0.1.0"
export QLANG_BASE_URL="file://$work/fake"
install="$root/install.sh"
dest="$work/bin"

echo "install"
sh "$install" --dir "$dest" >"$work/out1" 2>&1 || bad "first install failed"
expect "the program is installed and runs" "$dest/qlang" --version
expect_out "says it installed" "^Installed: qlang " "$work/out1"
expect "no leftover temporary file" test ! -e "$dest/.qlang.new"
printf 'print(6 * 7)\n' > "$work/hello.q"
if [ "$("$dest/qlang" run "$work/hello.q")" = "42" ]; then pass "the installed program runs a qlang file"; else bad "running a file"; fi

echo "update"
sh "$install" --dir "$dest" >"$work/out2" 2>&1
expect_out "running it again says it is up to date" "^Already up to date: " "$work/out2"
printf '#!/bin/sh\necho "qlang 0.0.1"\n' > "$dest/qlang"; chmod +x "$dest/qlang"
sh "$install" --dir "$dest" >"$work/out3" 2>&1
expect_out "an older version is updated" "^Updated: qlang 0.0.1 -> qlang " "$work/out3"
expect "the new program replaced the old one" test "$("$dest/qlang" --version)" != "qlang 0.0.1"

echo "specific version, and piping the script into sh"
dest2="$work/bin2"
sh "$install" --dir "$dest2" --version 0.1.0 >"$work/out4" 2>&1 || bad "install --version 0.1.0 failed"
expect "version 0.1.0 installed" test -x "$dest2/qlang"
sh "$install" --dir "$work/bin3" --version v0.1.0 >/dev/null 2>&1 && pass "a leading v is accepted" || bad "a leading v is accepted"
dest4="$work/bin4"
(cat "$install" | sh -s -- --dir "$dest4") >"$work/out5" 2>&1 || bad "install through a pipe failed"
expect "installed through a pipe" test -x "$dest4/qlang"
(cat "$install" | sh -s -- --help) >"$work/out6" 2>&1
expect_out "--help works through a pipe" "Install or update qlang" "$work/out6"

echo "safety"
cp -r "$work/fake" "$work/tampered"
printf 'garbage' >> "$work/tampered/latest/download/qlang-$target.tar.gz"
before="$("$dest/qlang" --version)"
if QLANG_BASE_URL="file://$work/tampered" sh "$install" --dir "$dest" >"$work/out7" 2>&1; then bad "a tampered archive was accepted"; else pass "a tampered archive is refused"; fi
expect_out "the error names the checksum" "checksum mismatch" "$work/out7"
expect "the installed program is untouched" test "$("$dest/qlang" --version)" = "$before"
if sh "$install" --dir "$work/bin5" --version 9.9.9 >"$work/out8" 2>&1; then bad "a missing release was accepted"; else pass "a missing release is an error"; fi
expect_out "the error mentions the release" "is there a release 9.9.9" "$work/out8"
expect "nothing was installed" test ! -e "$work/bin5/qlang"
if sh "$install" --bogus >"$work/out9" 2>&1; then bad "unknown option accepted"; else pass "unknown option is an error"; fi
mkdir -p "$work/fake-uname" "$work/fake-code"
printf '#!/bin/sh\necho Plan9\n' > "$work/fake-uname/uname"; chmod +x "$work/fake-uname/uname"
if PATH="$work/fake-uname:$PATH" sh "$install" --dir "$work/bin6" >"$work/out10" 2>&1; then bad "unknown platform accepted"; else pass "unknown platform is an error"; fi
expect_out "it suggests building from source" "cargo install" "$work/out10"

echo "PATH hint"
expect_out "warns when the directory is not in PATH" "is not in your PATH" "$work/out1"

echo "VS Code extension"
printf '#!/bin/sh\necho "code $*" >> "%s/code.log"\n' "$work" > "$work/fake-code/code"; chmod +x "$work/fake-code/code"
PATH="$work/fake-code:$PATH" sh "$install" --dir "$dest" --vscode >"$work/out11" 2>&1 || bad "install --vscode failed"
expect_out "runs code --install-extension" "install-extension .*qlang.vsix --force" "$work/code.log"
: > "$work/code.log"
PATH="$work/fake-code:$PATH" sh "$install" --dir "$dest" --vscode --profile Rust >"$work/out12" 2>&1 || bad "install --vscode --profile failed"
expect_out "passes the profile" "code --profile Rust --install-extension" "$work/code.log"
# Without any `code` command. The PATH holds only the tools install.sh needs, in a private
# directory, so that a VS Code installed on this machine can never be reached by a test.
tools="$work/tools"; mkdir -p "$tools"
for t in uname mktemp curl wget grep cut tar gzip gunzip sha256sum shasum cp chmod mv rm mkdir cat sed head tr dirname basename; do
  p="$(command -v "$t" 2>/dev/null || true)"
  case "$p" in /*) ln -sf "$p" "$tools/$t" ;; esac
done
if PATH="$tools" /bin/sh "$install" --dir "$dest" --vscode >"$work/out13" 2>&1; then
  bad "a missing code command was accepted"
else
  pass "a missing code command is an error"
  expect_out "and says how to get it" "Shell Command" "$work/out13"
fi

echo
if [ "$fails" -eq 0 ]; then echo "all install tests passed"; else echo "$fails install test(s) failed"; exit 1; fi
