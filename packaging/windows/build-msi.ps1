# brow — MSI installer builder (Phase 5)
#
# Compiles packaging/windows/brow.wxs with the WiX 6 CLI. The WiX CLI is
# expected on PATH (GitHub runners: `winget install --silent --exact
# WiXToolset.WiXCLI --version 6.0.2.0` — see .github/workflows/release.yml).
#
# Usage:
#   powershell -File build-msi.ps1 -PayloadDir C:\staging\brow `
#     -Version 0.5.0 -IconFile C:\repo\packaging\icons\brow.ico `
#     -OutDir C:\staging\dist
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$PayloadDir,
  [Parameter(Mandatory = $true)][string]$Version,
  # NOTE: defaults are resolved lazily in the body below. Windows PowerShell
  # 5.1 ("powershell -File", which is how the release workflow invokes this)
  # evaluates parameter defaults BEFORE $PSScriptRoot is populated, so any
  # default like (Join-Path $PSScriptRoot ...) fails with "Cannot bind
  # argument to parameter 'Path' because it is an empty string".
  [Parameter(Mandatory = $false)][string]$IconFile = "",
  [Parameter(Mandatory = $false)][string]$Wxs = "",
  [Parameter(Mandatory = $false)][string]$OutDir = ""
)

$ErrorActionPreference = "Stop"

if (-not $IconFile) { $IconFile = Join-Path $PSScriptRoot "..\icons\brow.ico" }
if (-not $Wxs)      { $Wxs      = Join-Path $PSScriptRoot "brow.wxs" }
if (-not $OutDir)   { $OutDir   = Join-Path $PSScriptRoot "dist" }

if (-not (Test-Path (Join-Path $PayloadDir "brow.exe"))) {
  throw "payload missing brow.exe: $PayloadDir"
}
if (-not (Test-Path (Join-Path $PayloadDir "resources"))) {
  throw "payload missing resources dir: $PayloadDir"
}
$wixCmd = Get-Command wix -ErrorAction SilentlyContinue
if (-not $wixCmd) {
  throw "wix CLI not found on PATH. Install with: winget install --silent --exact WiXToolset.WiXCLI --version 6.0.2.0"
}

# WiX resolves relative File sources against the current directory; pin it to
# the payload root so $(env.BROW_PAYLOAD) stays authoritative.
$env:BROW_PAYLOAD = (Resolve-Path $PayloadDir).Path
$env:BROW_ICON = (Resolve-Path $IconFile).Path

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$msi = Join-Path $OutDir ("brow-{0}-x64.msi" -f $Version)

& wix build -arch x64 `
  -d "BrowVersion=$Version" `
  -out $msi $Wxs
if ($LASTEXITCODE -ne 0) { throw "wix build failed with exit code $LASTEXITCODE" }

Write-Host "wrote $msi"
