#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Instrument frozen QML paint bodies without rewriting their math."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
here=Path(__file__).resolve().parent
env=dict(os.environ,QT_QPA_PLATFORM='offscreen',QT_QUICK_BACKEND='software',QML_XHR_ALLOW_FILE_READ='1',
         DBUS_SESSION_BUS_ADDRESS='unix:path=/nonexistent-primitives-bus',DBUS_SYSTEM_BUS_ADDRESS='unix:path=/nonexistent-primitives-system-bus')
with tempfile.TemporaryDirectory(prefix='primitives-record-') as tmp:
    log=Path(tmp)/'qtest.log'
    subprocess.run(['/usr/lib/qt6/bin/qmltestrunner','-maxwarnings','100000','-input',str(here/'tst_record_primitives.qml'),'-o',str(log)+',txt'],env=env,check=True,timeout=120,capture_output=True)
    rows=['# Frozen HEAD 87bb0c9: F visual density scale speed trails seed width height elapsed palette reduced phase count; P index xyxy color alpha radius kind.']
    frames=records=0
    for line in log.read_text().splitlines():
        for marker,tag in [('PRIMITIVE_FRAME ','F'),('PRIMITIVE_RECORD ','P')]:
            if marker in line:
                rows.append('\t'.join(map(str,[tag,*json.loads(line.split(marker,1)[1])])))
                if tag=='F':frames+=1
                else:records+=1
    assert frames==216 and records>10000, (frames,records)
    output='\n'.join(rows)+'\n';target=here/'fixtures.tsv'
    if '--check' in sys.argv:assert target.read_text()==output,'Primitive fixtures changed'
    else:target.write_text(output)
    print(f'Frozen primitives oracle: {frames} frames, {records} records '+('verified' if '--check' in sys.argv else 'recorded'))
