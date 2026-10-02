#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
set -euo pipefail
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
runner=${QMLTESTRUNNER:-/usr/lib/qt6/bin/qmltestrunner}
if [[ ! -x "$runner" ]]; then
    echo "Qt6 qmltestrunner not executable: $runner (set QMLTESTRUNNER)" >&2
    exit 1
fi
output=${1:-"$here/fixtures"}
# Temporary artifacts are confined to the authorized parity directory.
tmp=$(mktemp -d "$here/.record.XXXXXX")
trap 'rm -rf -- "$tmp"' EXIT
if ! QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software QML_XHR_ALLOW_FILE_READ=1 \
    timeout 180 "$runner" -input "$here/tst_record_snakes.qml" -o "$tmp/qtest.log",txt \
    >"$tmp/process.log" 2>&1; then
    cat "$tmp/process.log" "$tmp/qtest.log" >&2
    exit 1
fi
python3 "$here/fixtures.py" --record "$tmp/qtest.log" --output "$tmp/fixtures"
mkdir -p -- "$output"
for fixture in "$tmp/fixtures/"*; do
    stem=$(basename -- "$fixture")
    stem=${stem%%.json*}
    rm -f -- "$output/$stem.json" "$output/$stem.json.gz" "$output/$stem.json.xz"
    cp -- "$fixture" "$output/"
done
cat "$tmp/process.log"
sed -n '/Totals:/p' "$tmp/qtest.log"
