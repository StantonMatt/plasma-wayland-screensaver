#!/usr/bin/python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Sample the actual GPU halo and a co-registered native face mask."""
import argparse
import json
import subprocess
from pathlib import Path
import numpy as np
from PIL import Image


def ray(luma, x, y, ux, uy, steps):
    # Interpolate linear combinations of the four measured RGB8 pixel samples.
    xx, yy = x + steps*ux - .5, y + steps*uy - .5
    i, j = np.floor(xx).astype(int), np.floor(yy).astype(int)
    u, v = xx-i, yy-j
    return ((luma[j,i]*(1-u) + luma[j,i+1]*u)*(1-v)
            + (luma[j+1,i]*(1-u) + luma[j+1,i+1]*u)*v)


def sample(path, meta):
    x,y=meta['x'],meta['y']
    r=meta['radius'];fx=meta['fx'];fy=meta['fy']
    margin=int(np.ceil(1.2*r))+6
    left,top=int(x)-margin,int(y)-margin
    with Image.open(path) as image:
        separation=meta.get('maskOffset',image.width//2)
        area=(left,top,left+2*margin+1,top+2*margin+1)
        # A translated exact copy with white face material and zero halo RGB
        # records all antialiased face support, at the same pixel phase.
        faceregion=(area[0]+separation,area[1],area[2]+separation,area[3])
        face=image.crop(faceregion).convert('L').point(lambda a: 255 if a else 0)
        native_face=np.asarray(face)>0
        mask=native_face.copy()
        for dy in range(-2,3):
            for dx in range(-2,3):
                if dx*dx+dy*dy<=4:
                    mask |= np.roll(native_face,(dy,dx),(0,1))
        rgb=np.asarray(image.crop(area).convert('RGB')).astype(float)
    lum=rgb @ np.array([.2126,.7152,.0722])
    x-=left;y-=top
    yy,xx=np.indices(lum.shape,dtype=float)
    dx,dy=xx+.5-x,yy+.5-y
    zone=(np.hypot(dx,dy)<=1.2*r)&~mask
    steps=[]
    for axis in (0,1):
        a,b=(np.s_[:-1,:],np.s_[1:,:]) if axis==0 else (np.s_[:,:-1],np.s_[:,1:])
        steps.append(np.max(abs(lum[a]-lum[b]),where=zone[a]&zone[b],initial=0))
    d=np.r_[np.arange(0,1.2*r,1.),1.2*r]
    rise=0.;radial=[];where=[];outside_rises=[];native_rises=[]
    for degrees in range(-90,91,15):
        a=np.deg2rad(degrees)
        ux,uy=fx*np.cos(a)-fy*np.sin(a),fy*np.cos(a)+fx*np.sin(a)
        profile=ray(lum,x,y,ux,uy,d)
        native=lum[np.floor(y+d*uy).astype(int), np.floor(x+d*ux).astype(int)]
        native_rises.append(float(np.max(np.diff(native),initial=0)))
        diffs=np.diff(profile)
        radial.append(float(np.max(diffs,initial=0)))
        where.append(float(d[np.argmax(diffs)]))
        maskline=ray(mask.astype(float),x,y,ux,uy,d)>0.
        outside_rises.append(float(np.max(diffs, where=~maskline[1:] & ~maskline[:-1],initial=0)))
        rise=max(rise,radial[-1])
    # Measure the half-peak only where the registered face mask is clear.
    profile=ray(lum,x,y,fx,fy,d)
    face=ray(mask.astype(float),x,y,fx,fy,d)
    bare=face<1e-6
    peak=np.max(profile[bare],initial=0)
    stop=np.flatnonzero(bare & (profile>=peak/2))
    extent=d[stop[-1]]/r if len(stop) and peak>0 else 0.
    return dict(**meta,rays=13,maxRise=rise,rises=radial,nativeRises=native_rises,at=where,outsideRises=outside_rises,maxAdjacent=max(steps),
                halfExtent=extent,photometricHalfExtent=4.7*extent,peak=peak,
                passed=bool(rise<1. and max(steps)<=3 and 4.7*extent>=.6))


def flank_samples(path, meta):
    # The isolated native proof renders onto black. The two references start
    # at the face boundary of the first full-width shoulder, in its own normal.
    # Use the vertex shader's interpolated direction at the shared knot.
    lo, hi = np.array(meta['sections'][9])
    f = meta['fraction']
    knot = (1-f)*lo + f*hi
    normal = ((1-f)*lo[2:4]/np.linalg.norm(lo[2:4])
              + f*hi[2:4]/np.linalg.norm(hi[2:4]))
    normal /= np.linalg.norm(normal)
    face = knot[4]*(0.12 + 0.88*abs(knot[5]))
    tip = np.array([meta['x'], meta['y']])
    shoulder = knot[:2] + tip - [meta['sourceX'], meta['sourceY']]
    ahead = np.array([meta['fx'], meta['fy']])
    distances = np.array([20., 50., 80., 120., 160.])
    points = np.concatenate([tip + distances[:,None]*ahead,
                             shoulder + (distances+face)[:,None]*normal,
                             shoulder - (distances+face)[:,None]*normal])
    origin = np.floor(points.min(axis=0)).astype(int)-2
    limit = np.ceil(points.max(axis=0)).astype(int)+2
    with Image.open(path) as frame:
        if (origin<0).any() or (limit>=frame.size).any():
            raise ValueError(f'Clipped native shoulder probe for {path}')
        if any(frame.getpixel((0,0))[:3]):
            raise ValueError(f'Non-black isolation background for {path}')
        lum=np.asarray(frame.crop((*origin,*limit)).convert('RGB'), dtype=float) @ [.2126,.7152,.0722]
    # Bilinear RGB8 luminance is measured in 0..255 units.
    forward,left,right=ray(lum,*(points-origin).T,0.,0.,np.zeros(len(points))).reshape(3,-1)
    excess=forward-np.minimum(left,right)
    return dict(forward=forward.tolist(),left=left.tolist(),right=right.tolist(),
                maxExcess=float(excess.max()), passed=bool(np.all(excess<=2.)))


def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('root',type=Path);p.add_argument('--report',type=Path,required=True)
    p.add_argument('--partial',action='store_true');args=p.parse_args()
    rows=[];expected={};seen={};flanks=[];flankExpected={};flankSeen={}
    for scene in sorted((args.root/'scenes').glob('head-*')):
        if not (scene/'manifest.json').is_file(): continue
        manifest=json.loads((scene/'manifest.json').read_text())
        captures={f['timelineNs']:scene/f['panorama'] for f in manifest['frames']}
        if manifest.get('videoFrames'):
            v=manifest['video']; frames=scene/'sampled-heads'; frames.mkdir(exist_ok=True)
            step=1
            subprocess.run(['ffmpeg','-loglevel','error','-threads','1','-i',str(scene/(v['name']+'.mp4')),
                '-vf',f'select=not(mod(n\\,{step}))','-fps_mode','passthrough','-threads','1','-start_number','0','-y',str(frames/'%03d.png')],check=True)
            captures={str(v['start']*10**9+i*10**9//v['fps']):frames/f'{i//step:03d}.png' for i in range(0,v['frames'],step)}
        for line in (scene/'gpu-tests.log').read_text().splitlines():
            if 'HeadSample ' not in line:continue
            meta=json.loads(line.split('HeadSample ',1)[1]);ns=meta['ns']
            if ns not in captures:continue
            isVideo=bool(manifest.get('videoFrames'))
            index=round((int(ns)/1e9-600)*v['fps']) if isVideo else 0
            sampleRays=not isVideo or index%(v['fps']//6)==0
            sampleFlanks=(isVideo and int(ns)<=602_000_000_000) or scene.name.startswith('head-E-max-')
            group=(scene.name.rsplit('-',1)[0],ns)
            if group in expected and expected[group]!=meta['visible']:
                raise ValueError(f'Native replay count changed at {group}')
            if sampleRays: expected[group]=meta['visible']
            if sampleFlanks: flankExpected[group]=meta['visible']
            if 'x' not in meta:continue
            ordinal=meta['ordinal']
            if sampleFlanks and ordinal not in flankSeen.setdefault(group,set()):
                flankSeen[group].add(ordinal)
                flanks.append(dict(scene=scene.name,ns=ns,ordinal=ordinal,**flank_samples(captures[ns],meta)))
            if not sampleRays: continue
            uids=seen.setdefault(group,set())
            if ordinal in uids:continue
            uids.add(ordinal)
            rows.append(dict(scene=scene.name,**sample(captures[ns],meta)))
    missing={str(k):sorted(set(range(v))-seen.get(k,set())) for k,v in expected.items() if len(seen.get(k,set()))!=v}
    missingFlanks={str(k):sorted(set(range(v))-flankSeen.get(k,set())) for k,v in flankExpected.items() if len(flankSeen.get(k,set()))!=v}
    if not args.partial and len(flankExpected)!=122:raise ValueError(f'Expected E-max and 121 G times, found {len(flankExpected)}')
    if not args.partial and len(expected)!=20:raise ValueError(f'Expected 20 complete times, found {len(expected)}')
    if not rows:raise ValueError('No measured heads')
    summary=dict(heads=len(rows),rays=len(rows)*13,missing=missing,
                 nativeRise=max(max(m['nativeRises']) for m in rows),biggestRise=max(m['maxRise'] for m in rows),biggestStep=max(m['maxAdjacent'] for m in rows),
                 monotonicToleranceExclusive=1., raysWithPositiveRise=sum(v>1e-6 for m in rows for v in m['rises']),
                 minimumExtent=min(m['halfExtent'] for m in rows),
                 minimumPhotometricExtent=min(m['photometricHalfExtent'] for m in rows),
                 flankHeads=len(flanks),flankDistances=[20,50,80,120,160],flankTimes=len(flankExpected),
                 missingFlanks=missingFlanks,flankFailures=sum(not m['passed'] for m in flanks),
                 maximumFlankExcess=max((m['maxExcess'] for m in flanks),default=0),failures=sum(not m['passed'] for m in rows))
    print(json.dumps(summary,indent=2));args.report.write_text(json.dumps(dict(summary=summary,measurements=rows,flanks=flanks),indent=2))
    return (bool(missing or missingFlanks) and not args.partial) or bool(summary['failures'] or summary['flankFailures'])

if __name__=='__main__':
    raise SystemExit(main())
