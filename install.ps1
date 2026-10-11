# Install or update qlang on Windows (https://github.com/quenti77/qlang).
#
#   irm https://raw.githubusercontent.com/quenti77/qlang/main/install.ps1 | iex
#
# To pass options, download the script and run it:
#   .\install.ps1 -Version 0.1.0 -VsCode
#
# Running it again updates qlang to the latest release.
#
#   -Version X     install this version instead of the latest one
#   -Dir DIR       install into DIR (default: %LOCALAPPDATA%\Programs\qlang)
#   -VsCode        also install the VS Code extension (needs the `code` command)
#   -Profile NAME  VS Code profile for the extension
#
# $env:QLANG_BASE_URL overrides https://github.com/quenti77/qlang/releases (mirror, tests).
[CmdletBinding()]
param(
  [string]$Version = "latest",
  [string]$Dir = (Join-Path $env:LOCALAPPDATA "Programs\qlang"),
  [switch]$VsCode,
  [string]$Profile = ""
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

function Fail($message) { throw "qlang install: $message" }

$repo = "quenti77/qlang"
$baseUrl = if ($env:QLANG_BASE_URL) { $env:QLANG_BASE_URL } else { "https://github.com/$repo/releases" }
if ($Version -eq "latest") { $url = "$baseUrl/latest/download" }
else { $url = "$baseUrl/download/v$($Version.TrimStart('v'))" }

if (-not [Environment]::Is64BitOperatingSystem) { Fail "only 64-bit Windows is supported" }
if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") {
  Write-Host "ARM64 Windows: using the x64 build (it runs under emulation)."
}
$target = "x86_64-pc-windows-msvc"

$tmp = Join-Path ([IO.Path]::GetTempPath()) ("qlang-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
  function Get-Checked($name) {
    $file = Join-Path $tmp $name
    try { Invoke-WebRequest -Uri "$url/$name" -OutFile $file -UseBasicParsing }
    catch { Fail "cannot download $url/$name" }
    $line = Get-Content (Join-Path $tmp "SHA256SUMS") | Where-Object { $_ -match "\s$([regex]::Escape($name))$" } | Select-Object -First 1
    if (-not $line) { Fail "$name is not listed in SHA256SUMS" }
    $expected = ($line -split "\s+")[0].ToLower()
    $actual = (Get-FileHash -Algorithm SHA256 $file).Hash.ToLower()
    if ($expected -ne $actual) { Fail "checksum mismatch for $name (expected $expected, got $actual)" }
    return $file
  }

  $exe = Join-Path $Dir "qlang.exe"
  $previous = ""
  if (Test-Path $exe) { try { $previous = (& $exe --version) } catch { } }

  Write-Host "Downloading qlang ($Version) for $target..."
  try { Invoke-WebRequest -Uri "$url/SHA256SUMS" -OutFile (Join-Path $tmp "SHA256SUMS") -UseBasicParsing }
  catch { Fail "cannot download $url/SHA256SUMS (is there a release $Version?)" }
  $archive = Get-Checked "qlang-$target.zip"

  Expand-Archive -Path $archive -DestinationPath $tmp -Force
  $new = Join-Path $tmp "qlang\qlang.exe"
  if (-not (Test-Path $new)) { Fail "the archive does not contain qlang.exe" }

  New-Item -ItemType Directory -Path $Dir -Force | Out-Null
  # Windows cannot overwrite a running program but can rename it away
  if (Test-Path $exe) { Move-Item -Force $exe "$exe.old" }
  Copy-Item -Force $new $exe
  Remove-Item -Force "$exe.old" -ErrorAction SilentlyContinue

  $installed = (& $exe --version)
  if (-not $installed) { Fail "the installed program does not run" }
  if ($previous -and $previous -ne $installed) { Write-Host "Updated: $previous -> $installed ($exe)" }
  elseif ($previous) { Write-Host "Already up to date: $installed ($exe)" }
  else { Write-Host "Installed: $installed ($exe)" }

  $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
  if (($userPath -split ";") -notcontains $Dir) {
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$Dir", "User")
    Write-Host "Added $Dir to your PATH. Open a new terminal to use 'qlang'."
  }

  if ($VsCode) {
    $code = Get-Command code, code-insiders, codium -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $code) { Fail "the 'code' command was not found. In VS Code: Command Palette > 'Shell Command: Install code command in PATH'" }
    $vsix = Get-Checked "qlang.vsix"
    $codeArgs = @("--install-extension", $vsix, "--force")
    if ($Profile) { $codeArgs = @("--profile", $Profile) + $codeArgs }
    & $code.Source @codeArgs
    Write-Host "VS Code extension installed. Reload the window to use it."
  }
}
finally {
  Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
