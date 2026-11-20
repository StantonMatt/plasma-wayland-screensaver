#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Execute the unchanged QML paint handlers against a command recorder."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
here=Path(__file__).resolve().parent
env=dict(os.environ,QT_QPA_PLATFORM='offscreen',QT_QUICK_BACKEND='software',QML_XHR_ALLOW_FILE_READ='1',
    DBUS_SESSION_BUS_ADDRESS='unix:path=/nonexistent-strokes-record-bus',DBUS_SYSTEM_BUS_ADDRESS='unix:path=/nonexistent-strokes-record-system-bus')
with tempfile.TemporaryDirectory(prefix='strokes-record-') as tmp:
    log=Path(tmp)/'qtest.log'
    result=subprocess.run(['/usr/lib/qt6/bin/qmltestrunner','-input',str(here/'tst_record_strokes.qml'),'-o',str(log)+',txt'],env=env,timeout=120,capture_output=True)
    if result.returncode:
        print(log.read_text());print(result.stderr.decode());sys.exit(result.returncode)
    rows=['# Frozen 87bb0c9 QML strokes. F: visual,density,scale,speed,trails,seed,width,height,elapsed,palette,reduced,phase. R/C/Q: path,circles,quadratic,width/alpha/color.']
    frames=0
    for line in log.read_text().splitlines():
        if 'STROKES_FIXTURE ' not in line:continue
        f=json.loads(line.split('STROKES_FIXTURE ',1)[1]);frames+=1
        rows.append('\t'.join(map(str,['F',4 if f['name']=='Ribbons' else 5,*f['config'],f['palette'],f['reduced'],f['phase']])))
        rows.extend('\t'.join(map(str,row)) for row in f['commands'])
    assert frames==144,frames
    output='\n'.join(rows)+'\n';target=here/'fixtures.tsv'
    if '--check' in sys.argv:assert target.read_text()==output,'Strokes fixtures changed'
    else:target.write_text(output)
    print(f'Strokes oracle: {frames} frames, {len(rows)-1} fixture rows verified')
