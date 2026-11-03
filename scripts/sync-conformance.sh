#!/bin/sh
# Copy the shared tekl conformance fixtures into the Termux app's JVM test resources.
#
#   scripts/sync-conformance.sh [path-to-termux-app]
#
# Default target is the sibling checkout ../termux-app. The app test reads
# termux-shared/src/test/resources/tekl-conformance/{manifest.tsv,cases/*.tekl}.
set -eu

repo="$(cd "$(dirname "$0")/.." && pwd)"
src="$repo/conformance"
app="${1:-$(cd "$repo/.." && pwd)/termux-app}"
dst="$app/termux-shared/src/test/resources/tekl-conformance"

if [ ! -d "$app/termux-shared" ]; then
    echo "error: $app does not look like the termux-app checkout" >&2
    echo "usage: $0 [path-to-termux-app]" >&2
    exit 1
fi

rm -f "$dst/manifest.tsv" "$dst"/cases/*.tekl
mkdir -p "$dst/cases"
cp "$src/manifest.tsv" "$dst/manifest.tsv"
cp "$src"/cases/*.tekl "$dst/cases/"

echo "synced $(find "$dst/cases" -name '*.tekl' | wc -l | tr -d ' ') fixtures to $dst"
