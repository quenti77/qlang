# Install or update qlang on Windows (https://github.com/quenti77/qlang).
#
#   irm https://raw.githubusercontent.com/quenti77/qlang/main/install.ps1 | iex
#   & ([scriptblock]::Create((irm https://raw.githubusercontent.com/quenti77/qlang/main/install.ps1))) -Version 0.64.0
#
# Parameters: -Version X (default: the latest release), -Dir DIR (default: %LOCALAPPDATA%\qlang\bin,
# or $env:QLANG_INSTALL_DIR). The folder is added to the user's PATH. The download is checked
# against the SHA256SUMS of the release. $env:QLANG_BASE_URL overrides the releases address.
param(
  [string]$Version = "latest",
  [string]$Dir = ""
)
$ErrorActionPreference = "Stop"

$repo = "quenti77/qlang"
$base = if ($env:QLANG_BASE_URL) { $env:QLANG_BASE_URL } else { "https://github.com/$repo/releases" }
if (-not $Dir) {
  $Dir = if ($env:QLANG_INSTALL_DIR) { $env:QLANG_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "qlang\bin" }
}
if ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne "X64") {
  throw "no prebuilt qlang for this processor. Build it from source: cargo install --git https://github.com/$repo qlang-cli"
}
$target = "x86_64-pc-windows-msvc"
$url = if ($Version -eq "latest") { "$base/latest/download" } else { "$base/download/v$($Version.TrimStart('v'))" }

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("qlang-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
  Write-Host "Downloading qlang ($Version) for $target..."
  $name = "qlang-$target.zip"
  Invoke-WebRequest "$url/SHA256SUMS" -OutFile (Join-Path $tmp "SHA256SUMS") -UseBasicParsing
  Invoke-WebRequest "$url/$name" -OutFile (Join-Path $tmp $name) -UseBasicParsing
  $line = Get-Content (Join-Path $tmp "SHA256SUMS") | Where-Object { $_ -match "  $([regex]::Escape($name))$" } | Select-Object -First 1
  if (-not $line) { throw "$name is not listed in SHA256SUMS" }
  $expected = ($line -split "\s+")[0]
  $actual = (Get-FileHash (Join-Path $tmp $name) -Algorithm SHA256).Hash.ToLower()
  if ($expected -ne $actual) { throw "checksum mismatch for $name (expected $expected, got $actual)" }

  Expand-Archive (Join-Path $tmp $name) -DestinationPath $tmp -Force
  New-Item -ItemType Directory -Path $Dir -Force | Out-Null
  $exe = Join-Path $Dir "qlang.exe"
  # a running qlang.exe cannot be overwritten, but it can be renamed
  if (Test-Path $exe) { Move-Item $exe "$exe.old" -Force }
  Copy-Item (Join-Path $tmp "qlang\qlang.exe") $exe -Force
  Remove-Item "$exe.old" -Force -ErrorAction SilentlyContinue
  Write-Host "Installed: $(& $exe --version) ($exe)"

  $user = [Environment]::GetEnvironmentVariable("Path", "User")
  if (($user -split ";") -notcontains $Dir) {
    [Environment]::SetEnvironmentVariable("Path", "$user;$Dir".TrimStart(";"), "User")
    Write-Host "Added $Dir to your PATH: open a new terminal."
  }
} finally {
  Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
