#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Track the arrival star's >50 grey footprint at 30 fps in a native or half-size panorama."""
import argparse
from collections import deque
import json
import math
from pathlib import Path
import subprocess

from PIL import Image

# Coarse waypoints identify the nominated star even when a neighbour is brighter.
# They define a search window only. Contact, centroids and plateaux use real pixels.
GUIDES = {
    100: [(76,5627,941),(76.5,5912,981),(77,6319,1031),(77.5,6907,1089),
          (78,7707,1135),(78.2,8069,1142),(79,9431,1092)],
    300: [(76,5390,859),(76.5,5580,881),(77,5838,909),(77.5,6199,945),
          (78,6727,986),(78.2,6994,1002),(78.4,7291,1017),(78.6,7614,1028),
          (78.8,7957,1034),(79,8306,1034)],
}


def guide(speed, t):
    for (t0,x0,y0),(t1,x1,y1) in zip(GUIDES[speed], GUIDES[speed][1:]):
        if t0 <= t <= t1:
            u = (t - t0)/(t1 - t0)
            return (x0 + u*(x1-x0), y0 + u*(y1-y0))
    raise ValueError(f'No waypoint at {t}')


def footprint(image, head, scale):
    cx,cy = head
    left,top = max(0,round(cx-130*scale)),max(0,round(cy-45*scale))
    right,bottom = min(image.width,round(cx+55*scale)),min(image.height,round(cy+46*scale))
    if left >= right or top >= bottom:
        return None
    patch = image.crop((left,top,right,bottom))
    grey = patch.convert('L')
    pixels = list(grey.getdata())
    w,h = patch.size
    mask = {(i%w,i//w) for i,v in enumerate(pixels) if v > 50}
    best = None
    best_score = -math.inf
    while mask:
        seed = mask.pop()
        component = {seed}
        stack = deque([seed])
        while stack:
            x,y = stack.pop()
            for dx,dy in ((-1,-1),(0,-1),(1,-1),(-1,0),(1,0),(-1,1),(0,1),(1,1)):
                point = x+dx,y+dy
                if point in mask:
                    mask.remove(point)
                    stack.append(point)
                    component.add(point)
        if len(component) < max(3,15*scale*scale):
            continue
        peak = max(pixels[y*w+x] for x,y in component)
        tip = max(x for x,y in component) + left
        mid_y = sum(y for x,y in component)/len(component) + top
        score = min(len(component)/(scale*scale),550) - 1.3*abs(tip-min(cx+8*scale,image.width-1))/scale - 4*abs(mid_y-cy)/scale
        if peak > 65 and score > best_score:
            best_score = score
            best = component
    if best is None:
        return None
    points = {(x+left,y+top) for x,y in best}
    colours = [patch.getpixel(p) for p in best]
    def longest_run(clipped):
        flat = 0
        for dx,dy in ((1,0),(0,1),(1,1),(1,-1)):
            for x,y in clipped:
                if (x-dx,y-dy) in clipped:
                    continue
                n = 1
                while (x+n*dx,y+n*dy) in clipped:
                    n += 1
                flat = max(flat,n)
        return flat
    grey_flat=longest_run({(x,y) for x,y in best if pixels[y*w+x] == 255})
    channel_flat=max(longest_run({pt for pt in best if patch.getpixel(pt)[c] == 255}) for c in range(3))
    return points,max(pixels[y*w+x] for x,y in best),max(max(c) for c in colours),grey_flat,channel_flat



def track(path):
    scene = json.loads(path.read_text())
    movie = path.parent/(scene['video']['name']+'.mp4')
    speed = scene['settings']['speed']
    fps = scene['video']['fps']
    width = int(subprocess.check_output(['ffprobe','-v','error','-select_streams','v:0',
                '-show_entries','stream=width','-of','csv=p=0',str(movie)]))
    scale = width/7920
    assert scale in (0.5,1) and fps in (30,60)
    x0,y0,w,h = (round(n*scale) for n in (4880,700,3040,520))
    filters = f'crop={w}:{h}:{x0}:{y0}'
    if fps == 60:
        filters += ',select=not(mod(n\\,2))'
    cmd = ['ffmpeg','-v','error','-threads','3','-ss',str(76-scene['video']['start']),
           '-i',str(movie),'-vf',filters,'-fps_mode','passthrough','-frames:v','90','-threads','1',
           '-pix_fmt','rgb24','-f','rawvideo','-']
    pairs=[]
    previous=None
    last=None
    disappeared=False
    peak=colour_peak=flat=channel_flat=0
    frame_bytes=w*h*3
    with subprocess.Popen(cmd,stdout=subprocess.PIPE) as decoder:
        for n in range(90):
            t=76+n/30
            raw=decoder.stdout.read(frame_bytes)
            if len(raw)!=frame_bytes:
                raise ValueError(f'Short film at {t}: {movie}')
            x,y=guide(speed,t)
            if disappeared or x>7920+100:
                continue
            region=footprint(Image.frombytes('RGB',(w,h),raw),(x*scale-x0,y*scale-y0),scale)
            if region is None:
                assert x>=7910, f'Target lost inside viewport at {t}'
                disappeared=True
                continue
            points,grey,channel,plateau,saturated=region
            peak=max(peak,grey)
            colour_peak=max(colour_peak,channel)
            flat=max(flat,plateau)
            channel_flat=max(channel_flat,saturated)
            if previous:
                overlap=len(points & previous)
                touch=overlap or any((x+dx,y+dy) in previous for x,y in points
                                       for dx,dy in ((1,0),(-1,0),(0,1),(0,-1)))
                pairs.append(dict(time=round(t,4),overlapPixels=overlap,touch=bool(touch)))
            previous=points
            last=t
        decoder.stdout.close()
        if decoder.wait(timeout=60):
            raise RuntimeError(f'ffmpeg failed for {movie}')
    assert last is not None and last>=78.0 and pairs
    return dict(film=str(movie),speed=speed,trails=scene['settings']['trails'],scale=scale,
                interval=[76,78.2],lastVisibleTime=round(last,4),visiblePairs=len(pairs),
                missed=[pair['time'] for pair in pairs if not pair['touch']],
                minimumOverlap=min(pair['overlapPixels'] for pair in pairs),peakGrey=peak,
                peakChannel=colour_peak,clippedGreyRun=flat,clippedChannelRun=channel_flat,pairs=pairs)


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('manifests',type=Path,nargs='+')
    p.add_argument('--report',required=True,type=Path)
    args=p.parse_args()
    results=[track(path) for path in args.manifests]
    args.report.write_text(json.dumps(results,indent=2)+'\n')
    for row in results:
        print({k:v for k,v in row.items() if k!='pairs'})
    return int(any(row['missed'] or row['clippedChannelRun']>8*row['scale'] for row in results))


if __name__=='__main__':
    raise SystemExit(main())
