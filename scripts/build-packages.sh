#!/usr/bin/env bash
# Build the .deb and .rpm packages of a release from the two static Linux programs.
#
#   scripts/build-packages.sh <version> <amd64-qlang> <arm64-qlang> <LICENSE> <out-dir>
#
# Needs dpkg-deb and rpmbuild (apt install rpm). The programs are static (musl): the packages have
# no dependencies. They are attached to the release; they are not in an apt/yum repository.
set -euo pipefail

version="${1:?usage: build-packages.sh <version> <amd64-qlang> <arm64-qlang> <LICENSE> <out-dir>}"
amd64="${2:?missing the amd64 program}"
arm64="${3:?missing the arm64 program}"
license="${4:?missing the LICENSE file}"
out="${5:?missing the output directory}"
version="${version#v}"
mkdir -p "$out"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
desc="A small, readable, statically typed language with a built-in web server"

for pair in "amd64:$amd64:x86_64" "arm64:$arm64:aarch64"; do
  IFS=: read -r deb_arch program rpm_arch <<< "$pair"

  # --- .deb
  root="$work/deb-$deb_arch"
  install -Dm755 "$program" "$root/usr/bin/qlang"
  install -Dm644 "$license" "$root/usr/share/doc/qlang/copyright"
  size=$(( $(stat -c %s "$program") / 1024 + 1 ))
  mkdir -p "$root/DEBIAN"
  cat > "$root/DEBIAN/control" <<CONTROL
Package: qlang
Version: $version
Architecture: $deb_arch
Maintainer: quenti77 <quentin.ysambert@gmail.com>
Installed-Size: $size
Section: devel
Priority: optional
Homepage: https://github.com/quenti77/qlang
Description: $desc
 qlang is an interpreter with a formatter, a language server, a test runner,
 a package manager and an HTTP server. This package is the static program.
CONTROL
  dpkg-deb --root-owner-group --build "$root" "$out/qlang_${version}_${deb_arch}.deb" > /dev/null

  # --- .rpm
  top="$work/rpm-$rpm_arch"
  mkdir -p "$top"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}
  cp "$program" "$top/SOURCES/qlang"
  cp "$license" "$top/SOURCES/LICENSE"
  cat > "$top/SPECS/qlang.spec" <<SPEC
Name:       qlang
Version:    $version
Release:    1
Summary:    $desc
License:    MIT
URL:        https://github.com/quenti77/qlang
AutoReqProv: no

%global debug_package %{nil}
%global __os_install_post %{nil}
%global _build_id_links none

%description
qlang is an interpreter with a formatter, a language server, a test runner, a package manager
and an HTTP server. This package is the static program.

%install
install -Dm755 %{_sourcedir}/qlang %{buildroot}/usr/bin/qlang
install -Dm644 %{_sourcedir}/LICENSE %{buildroot}/usr/share/licenses/qlang/LICENSE

%files
/usr/bin/qlang
%license /usr/share/licenses/qlang/LICENSE
SPEC
  rpmbuild --quiet --define "_topdir $top" --target "$rpm_arch-linux" -bb "$top/SPECS/qlang.spec"
  cp "$top"/RPMS/"$rpm_arch"/qlang-"$version"-1."$rpm_arch".rpm "$out/"
done
ls -l "$out"
