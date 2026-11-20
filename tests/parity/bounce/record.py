#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Record frozen 8992f46 QML physics, without translating its math."""
import json, os
from pathlib import Path
import subprocess, sys, tempfile
here=Path(__file__).resolve().parent
env=dict(os.environ,QT_QPA_PLATFORM='offscreen',QT_QUICK_BACKEND='software',QML_XHR_ALLOW_FILE_READ='1',
 DBUS_SESSION_BUS_ADDRESS='unix:path=/nonexistent-bounce-bus',DBUS_SYSTEM_BUS_ADDRESS='unix:path=/nonexistent-bounce-system')
with tempfile.TemporaryDirectory(prefix='bounce-record-') as tmp:
 log=Path(tmp)/'qtest.log'
 subprocess.run(['/usr/lib/qt6/bin/qmltestrunner','-maxwarnings','100000','-input',str(here/'tst_record_bounce.qml'),'-o',str(log)+',txt'],env=env,check=True,timeout=120,capture_output=True)
 rows=['# Frozen 8992f46 Bounce.qml. F scenario step width height seed count scale speed trails gravity elasticity collisions palette records; B index x y vx vy size trail_count trail_xy...; P index cx cy sx sy rgb alpha radius kind focalx focaly inner_radius outer_radius']
 frames=balls=primitives=0
 for line in log.read_text().splitlines():
  for marker,tag in [('BOUNCE_FRAME ','F'),('BOUNCE_BALL ','B'),('BOUNCE_PRIMITIVE ','P')]:
   if marker in line:
    rows.append('\t'.join(map(str,[tag,*json.loads(line.split(marker,1)[1])])))
    if tag=='F':frames+=1
    elif tag=='B':balls+=1
    else:primitives+=1
 assert frames==64 and balls==1128 and primitives==5306,(frames,balls,primitives)
 output='\n'.join(rows)+'\n';target=here/'fixtures.tsv'
 if '--check' in sys.argv:
  # JavaScript Math results may differ in the last bits between CPUs/libm builds,
  # so compare numbers with a tight tolerance rather than byte-for-byte.
  def same(a,b):
   if a==b:return True
   try:x,y=float(a),float(b)
   except ValueError:return False
   return abs(x-y)<=1e-9+1e-12*max(abs(x),abs(y))
  expected=target.read_text().splitlines();actual=output.splitlines()
  assert len(expected)==len(actual),'Bounce fixtures changed: row count'
  for number,(e,a) in enumerate(zip(expected,actual),1):
   ef,af=e.split('\t'),a.split('\t')
   assert len(ef)==len(af) and all(map(same,ef,af)),f'Bounce fixtures changed at row {number}:\n expected {e}\n actual   {a}'
 else:target.write_text(output)
 print(f'Frozen Bounce oracle: {frames} frames, {balls} balls, {primitives} ordered primitives '+('verified' if '--check' in sys.argv else 'recorded'))
