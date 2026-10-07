# brow — portable Windows build packager (Phase 5)
#
# Stages brow.exe + engine resources + docs into a self-contained folder and
# zips it. The zip is the "portable .exe" artifact: unzip anywhere, run
# brow\ brow.exe. resources\ MUST stay next to brow.exe (the engine resolves
# resources relative to the executable).
#
# Usage:
#   powershell -File make-portable.ps1 -PayloadDir C:\staging\brow `
#     -Version 0.5.0 -OutDir C:\staging\dist
#
# Payload contract: <PayloadDir>\brow.exe (PE 64-bit), <PayloadDir>\resources\,
# and the runtime DLLs (ANGLE/GStreamer/MSVC) staged next to brow.exe.
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$PayloadDir,
  [Parameter(Mandatory = $true)][string]$Version,
  # Lazily resolved in the body: powershell.exe 5.1 (-File) evaluates param
  # defaults before $PSScriptRoot exists.
  [Parameter(Mandatory = $false)][string]$OutDir = ""
)

$ErrorActionPreference = "Stop"

if (-not $OutDir) { $OutDir = Join-Path $PSScriptRoot "dist" }

if (-not (Test-Path (Join-Path $PayloadDir "brow.exe"))) {
  throw "payload missing brow.exe: $PayloadDir"
}
if (-not (Test-Path (Join-Path $PayloadDir "resources"))) {
  throw "payload missing resources dir: $PayloadDir"
}

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$stage = Join-Path ([System.IO.Path]::GetTempPath()) ("brow-portable-" + [System.Guid]::NewGuid().ToString("N").Substring(0, 8))
New-Item -ItemType Directory -Path $stage | Out-Null

try {
  $inner = Join-Path $stage "brow"
  New-Item -ItemType Directory -Path $inner | Out-Null

  Copy-Item (Join-Path $PayloadDir "brow.exe") $inner
  Copy-Item (Join-Path $PayloadDir "resources") $inner -Recurse

  # brow (v0.6.1): the fallback servoshell binary ships as brow-classic.exe
  # (engineering fallback only), plus the bundled Noto UI fonts (Arabic/RTL
  # support). Both are optional at package time — but included when staged.
  if (Test-Path (Join-Path $PayloadDir "brow-classic.exe")) {
    Copy-Item (Join-Path $PayloadDir "brow-classic.exe") $inner
  }
  if (Test-Path (Join-Path $PayloadDir "fonts")) {
    Copy-Item (Join-Path $PayloadDir "fonts") $inner -Recurse
  }

  # Runtime DLLs staged next to brow.exe by the release workflow (ANGLE,
  # GStreamer libs + plugin subset, MSVC CRT). Without them brow.exe cannot
  # start on machines without GStreamer installed. DLLs are part of the
  # payload contract for Windows; fail loudly if staging forgot them.
  $dlls = Get-ChildItem (Join-Path $PayloadDir "*.dll") -ErrorAction SilentlyContinue
  if (-not $dlls -or $dlls.Count -eq 0) {
    throw "payload missing runtime DLLs (ANGLE/GStreamer/MSVC): $PayloadDir"
  }
  Copy-Item $dlls.FullName $inner

  $iconSrc = Join-Path $repoRoot "packaging\icons\brow.ico"
  if (Test-Path $iconSrc) { Copy-Item $iconSrc $inner }

  foreach ($doc in @("LICENSE", "README.md")) {
    $docPath = Join-Path $repoRoot $doc
    if (Test-Path $docPath) { Copy-Item $docPath $inner }
  }

  # Run notes so the artifact is self-explanatory without the repo.
  @"
brow $Version — portable build (x86_64-windows)
================================================

Run:      brow.exe (brow-classic.exe is the fallback stock-Servo shell)
Privacy:  the tracker filter (EasyList) is embedded in the engine; an
          updated list is picked up from .\resources\easylist.txt when
          present next to brow.exe. UI fonts (Noto Sans + Arabic) install
          from .\fonts on first run.
Verify:   check the SHA256SUMS file published next to this zip on the
          release page before extracting.
Source:   https://github.com/salim77007j/brow
License:  MPL-2.0 (see LICENSE). Third-party licenses are listed in the
          servo repository NOTICE files bundled in resources\.
"@ | Set-Content -Path (Join-Path $inner "README.txt") -Encoding UTF8

  $zip = Join-Path $OutDir ("brow-{0}-x86_64-windows-portable.zip" -f $Version)
  New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
  if (Test-Path $zip) { Remove-Item $zip -Force }
  Compress-Archive -Path (Join-Path $stage "brow") -DestinationPath $zip
  Write-Host "wrote $zip"
} finally {
  Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
}
