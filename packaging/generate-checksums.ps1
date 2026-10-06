# brow — SHA256SUMS generator, Windows twin of generate-checksums.sh (Phase 5)
#
# Emits the byte-identical manifest format: "<hex>  <name>\n" (two spaces,
# LF line endings, binary-mode semantics of `sha256sum -b`). Verification on
# either platform: sha256sum -c SHA256SUMS.
#
# Usage:
#   powershell -File generate-checksums.ps1 -ArtifactDir C:\staging\dist
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$ArtifactDir
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path $ArtifactDir -PathType Container)) {
  throw "not a directory: $ArtifactDir"
}

$lines = Get-ChildItem -LiteralPath $ArtifactDir -File |
  Where-Object { $_.Name -ne "SHA256SUMS" } |
  Sort-Object -Property Name |
  ForEach-Object {
    $hash = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    # sha256sum text mode on Linux strips CR differences; we normalize to the
    # binary-mode manifest ("*<name>" is NOT used) — same format as `sha256sum -b`.
    "{0}  {1}" -f $hash, $_.Name
  }

if ($lines.Count -eq 0) {
  throw "no artifacts found in $ArtifactDir"
}

$out = Join-Path $ArtifactDir "SHA256SUMS"
# LF line endings + UTF-8 (no BOM) so sha256sum -c parses it on any OS.
[System.IO.File]::WriteAllText($out, ($lines -join "`n") + "`n", [System.Text.UTF8Encoding]::new($false))
Write-Host "wrote $out ($($lines.Count) entries)"
