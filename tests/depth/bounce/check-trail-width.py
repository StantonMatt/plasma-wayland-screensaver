#!/usr/bin/env python3
"""Measure the native Trails-100 study and crop an identical-pose turn.

Usage: check-trail-width.py BEFORE_SCENES AFTER_FIXED_POSE_SCENES OUTPUT
The scenes come from polish-029/bounce/capture.py, using the original Rust
archive for both sides. No display or GPU context is created by this script.
"""
import json
import sys
from pathlib import Path
from PIL import Image

before, after, output = map(Path, sys.argv[1:])
output.mkdir(parents=True, exist_ok=True)


def section(path):
    with Image.open(path) as image:
        # Silhouette-edge gradients isolate the unchanged sphere from the
        # trail's soft shoulders and halo. The two strongest opposing edges
        # locate its centre without a brightness-threshold bias.
        radius = 79.2
        side = 1280-round(.7*radius)
        column = [sum((a-b)*w for a,b,w in zip(image.getpixel((side,y))[:3],
                         image.getpixel((1000,y))[:3],(.2126,.7152,.0722)))
                  for y in range(400,1300)]
        slopes = [column[i+1]-column[i] for i in range(len(column)-1)]
        top = 400+max(range(len(slopes)),key=slopes.__getitem__)+.5
        bottom = 400+min(range(len(slopes)),key=slopes.__getitem__)+.5
        center_y = (top+bottom)/2
        sample_y = round(center_y-1.5*radius)
        row = [image.getpixel((x, sample_y))[:3] for x in range(1150, 1411)]
    luminance = [sum(c*w for c,w in zip(rgb, (0.2126,0.7152,0.0722))) for rgb in row]
    background = (luminance[0]+luminance[-1])/2
    gain = [max(0.,value-background) for value in luminance]
    center = max(range(len(gain)), key=gain.__getitem__)
    half = gain[center]/2
    left = next(i for i in range(center-1,-1,-1) if gain[i] <= half)
    right = next(i for i in range(center+1,len(gain)) if gain[i] <= half)
    lo = left+(half-gain[left])/(gain[left+1]-gain[left])
    hi = right-1+(gain[right-1]-half)/(gain[right-1]-gain[right])
    return dict(center_y=center_y, sample_y=sample_y, fwhm_px=hi-lo, peak_gain=gain[center], shoulder_gain=gain[center+round(.25*79.2)])


filename='700000000-HDMI-A-5.png'
measurements={v:section(base/'polish-width'/filename) for v,base in [('before',before),('after',after)]}
assert measurements['before']['center_y']==measurements['after']['center_y'], measurements
reduction=1-measurements['after']['fwhm_px']/measurements['before']['fwhm_px']
retained=measurements['after']['peak_gain']/measurements['before']['peak_gain']
measurements.update(body_width_reduction=reduction,core_peak_retained=retained)
(output/'trail-width.json').write_text(json.dumps(measurements,indent=2)+'\n')
for scene,filename,box,label in [('polish-width',filename,(1150,865,1410,1145),'straight'),('polish-turn','1600000000-HDMI-A-5.png',(920,840,1440,1210),'turn'),('polish-turn','1600000000-HDMI-A-5.png',(960,995,1130,1130),'turn-detail')]:
    panels=[]
    for version,base in [('before',before),('after',after)]:
        with Image.open(base/scene/filename) as image:
            crop=image.convert('RGB').crop(box)
        crop.save(output/f'{label}-{version}.png')
        panels.append(crop)
    joined=Image.new('RGB',(panels[0].width*2,panels[0].height))
    joined.paste(panels[0],(0,0));joined.paste(panels[1],(panels[0].width,0))
    joined.save(output/f'{label}-comparison.png')
print(json.dumps(measurements,indent=2))
assert .15 <= reduction <= .20, measurements
assert retained >= .95, measurements
assert measurements['after']['shoulder_gain'] >= 20, measurements
