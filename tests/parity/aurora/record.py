#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Record unmodified paint math from the frozen QML oracle; --check reproduces TSV."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
here = Path(__file__).resolve().parent
env = dict(os.environ, QT_QPA_PLATFORM="offscreen", QT_QUICK_BACKEND="software", QML_XHR_ALLOW_FILE_READ="1",
           DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent-aurora-record-bus",
           DBUS_SYSTEM_BUS_ADDRESS="unix:path=/nonexistent-aurora-record-system-bus")
with tempfile.TemporaryDirectory(prefix="aurora-record-") as tmp:
    log = Path(tmp) / "qtest.log"
    subprocess.run(["/usr/lib/qt6/bin/qmltestrunner", "-input", str(here / "tst_record_aurora.qml"),
                    "-o", str(log) + ",txt"], env=env, check=True, timeout=120, capture_output=True)
    rows = ["# Frozen Aurora.qml b0f8dfe. B: settings phase count index xyra colors; S: shade stops+alpha; R: VisualUtils random."]
    for line in log.read_text().splitlines():
        if "AURORA_FIXTURE " in line:
            f = json.loads(line.split("AURORA_FIXTURE ", 1)[1])
            c = f["config"]
            for i, b in enumerate(f["bands"]):
                rows.append("\t".join(map(str, ["B", *c[:5], f["palette"], f["reduced"], *c[5:], f["phase"],
                    len(f["bands"]), i, *b["geometry"], b["alpha"], b["stops"][0][1], b["stops"][1][1]])))
                assert [s[0] for s in b["stops"]] == [0, .5, 1] and b["stops"][2][1] == "#00000000"
            rows.append("\t".join(map(str, ["S", *sum(f["shade"], []), f["shadeAlpha"]])))
        if "AURORA_RANDOM " in line:
            rows.append("\t".join(map(str, ["R", *json.loads(line.split("AURORA_RANDOM ", 1)[1])])))
    assert len(rows) > 400
    output = "\n".join(rows) + "\n"
    target = here / "fixtures.tsv"
    if "--check" in sys.argv:
        assert target.read_text() == output, "Aurora fixtures changed"
    else:
        target.write_text(output)
    print(f"Aurora oracle: {len(rows)-1} fixture rows {'verified' if '--check' in sys.argv else 'recorded'}")
