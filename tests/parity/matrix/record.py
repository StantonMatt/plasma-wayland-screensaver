#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Record real old-QML stream parameters, glyphs, layout and column motion."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
here=Path(__file__).resolve().parent
env=dict(os.environ,QT_QPA_PLATFORM='offscreen',QT_QUICK_BACKEND='software',QML_XHR_ALLOW_FILE_READ='1',
    DBUS_SESSION_BUS_ADDRESS='unix:path=/nonexistent-matrix-bus',DBUS_SYSTEM_BUS_ADDRESS='unix:path=/nonexistent-matrix-system-bus')
with tempfile.TemporaryDirectory(prefix='matrix-record-') as tmp:
    log=Path(tmp)/'qtest.log'
    result=subprocess.run(['/usr/lib/qt6/bin/qmltestrunner','-maxwarnings','100000','-input',str(here/'tst_record_matrix.qml'),'-o',str(log)+',txt'],env=env,timeout=120,capture_output=True)
    if result.returncode:
        print(log.read_text());sys.exit(result.returncode)
    rows=['# Frozen 8992f46 Matrix.qml: F settings/phase/layout; C index x y length rate offset contentHeight headY glyph IDs.']
    frames=columns=0
    for line in log.read_text().splitlines():
        for marker,tag in [('MATRIX_FRAME ','F'),('MATRIX_COLUMN ','C')]:
            if marker in line:
                data=json.loads(line.split(marker,1)[1])
                if tag=='C':data=[*data[:-1],*data[-1]]
                rows.append('\t'.join(map(str,[tag,*data])))
                if tag=='F':frames+=1
                else:columns+=1
    assert frames==120 and columns>1000,(frames,columns)
    output='\n'.join(rows)+'\n';target=here/'fixtures.tsv'
    if '--check' in sys.argv:
        # JavaScript Math results may differ in the last bits between CPUs/libm builds.
        def same(a,b):
            if a==b:return True
            try:x,y=float(a),float(b)
            except ValueError:return False
            return abs(x-y)<=1e-9+1e-12*max(abs(x),abs(y))
        expected=target.read_text().splitlines();actual=output.splitlines()
        assert len(expected)==len(actual),'Matrix fixtures changed: row count (font environment may differ)'
        for number,(e,a) in enumerate(zip(expected,actual),1):
            ef,af=e.split('\t'),a.split('\t')
            assert len(ef)==len(af) and all(map(same,ef,af)),f'Matrix fixtures changed at row {number} (font environment may differ):\n expected {e}\n actual   {a}'
    else:target.write_text(output)
    print(f'Frozen Matrix oracle: {frames} frames, {columns} columns '+('verified' if '--check' in sys.argv else 'recorded'))
