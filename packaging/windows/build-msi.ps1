# brow — MSI installer builder (Phase 5)
#
# Compiles packaging/windows/brow.wxs with the WiX 7 CLI. The WiX CLI is
# expected on PATH (GitHub runners: `winget install --silent --exact
# WiXToolset.WiXCLI --version 7.0.0.0` — see .github/workflows/release.yml).
#
# Usage:
#   powershell -File build-msi.ps1 -PayloadDir C:\staging\brow `
#     -Version 0.5.0 -IconFile C:\repo\packaging\icons\brow.ico `
#     -OutDir C:\staging\dist
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$PayloadDir,
  [Parameter(Mandatory = $true)][string]$Version,
  [Parameter(Mandatory = $false)][string]$IconFile = (Join-Path $PSScriptRoot "..\icons\brow.ico"),
  [Parameter(Mandatory = $false)][string]$Wxs = (Join-Path $PSScriptRoot "brow.wxs"),
  [Parameter(Mandatory = $false)][string]$OutDir = (Join-Path $PSScriptRoot "dist")
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path (Join-Path $PayloadDir "brow.exe"))) {
  throw "payload missing brow.exe: $PayloadDir"
}
if (-not (Test-Path (Join-Path $PayloadDir "resources"))) {
  throw "payload missing resources dir: $PayloadDir"
}
$wixCmd = Get-Command wix -ErrorAction SilentlyContinue
if (-not $wixCmd) {
  throw "wix CLI not found on PATH. Install with: winget install --silent --exact WiXToolset.WiXCLI --version 7.0.0.0"
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
