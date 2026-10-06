#!/usr/bin/env bash
# brow — SHA256SUMS generator (Phase 5)
#
# Writes a standard `sha256sum -c`-compatible SHA256SUMS file for every
# artifact in a directory (the manifest itself is excluded). The .ps1 twin
# produces the byte-identical format on Windows so release manifests are
# uniform across platforms.
#
# Usage:
#   ./generate-checksums.sh <artifact-dir>      # writes <artifact-dir>/SHA256SUMS
#   sha256sum -c <artifact-dir>/SHA256SUMS      # verify
set -euo pipefail

DIR="${1:-.}"
[[ -d "$DIR" ]] || { echo "error: not a directory: $DIR" >&2; exit 1; }
# Resolve before `cd` below so relative DIRs keep working afterwards.
DIR="$(cd "$DIR" && pwd)"

OUT="$DIR/SHA256SUMS"
TMP="$(mktemp)"
trap 'rm -f "$TMP"' EXIT

# Deterministic order; TEXT-mode manifest ("hash␣␣name", no asterisk prefix)
# so the .ps1 twin produces byte-identical output on Windows; the manifest
# itself is excluded so it can never checksum itself. Entries are bare
# filenames so `sha256sum -c SHA256SUMS` works from inside the artifact dir.
cd "$DIR"
found=0
while IFS= read -r -d '' f; do
  found=1
  sha256sum "${f#./}"
done < <(find . -maxdepth 1 -type f ! -name 'SHA256SUMS' -print0 | sort -z) > "$TMP"

if [[ "$found" -eq 0 ]]; then
  echo "error: no artifacts found in $DIR" >&2
  exit 1
fi

mv "$TMP" "$OUT"
echo "wrote $OUT ($(wc -l < "$OUT") entries)"
