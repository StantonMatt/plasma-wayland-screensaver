#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Native Matrix regressions, using productScene PNGs (requires Pillow).

Halo mode requires a paired private capture with PVS_MATRIX_BOLT_REFERENCE=1;
only bolt emission is absent, leaving rain, flash, masks and simulation intact.
Use --capture ROOT --reference ROOT --output DIR. The 10..40 px profile is
aligned to the trunk around world y=1000. Reference pixels exclude rain glyphs
and separate the halo's edge from their edges. Many-seed mode instead takes
--many-expectations JSON and checks every return at both sampling phases.

Body width uses the median trunk-aligned bolt radiance above 40 Y', subtracting
paired bolt-off pixels and excluding independently identified rain glyphs.
This prevents branches in a sky probe or adjoining glyphs from widening the
wrong stroke. Core brightness is the mean of a fixed 13-pixel band; raw peak
pixels are separately checked. No shader/envelope model is used.
"""
from pathlib import Path
from PIL import Image,ImageStat
import argparse,json,statistics,colorsys,re
WEIGHTS=(.2126,.7152,.0722)
def y(rgb):return sum(a*b for a,b in zip(rgb,WEIGHTS))
def median_rgb(pixels):return tuple(statistics.median(p[c] for p in pixels) for c in range(3))
def runs(values,threshold):
    result=[];start=None
    for i,value in enumerate([*values,-1]):
        if value>threshold and start is None:start=i
        elif value<=threshold and start is not None:result.append((start,i));start=None
    return result
def trace(im):
    px=im.load();rows=[]
    for row in range(950,1051,4):
        values=[y(px[x,row]) for x in range(3800,4700)]
        floor=statistics.median(values)
        regions=runs(values,floor+40)
        a,b=max(regions,key=lambda r:r[1]-r[0])
        hot=max(values[a:b]);indices=[i for i in range(a,b) if values[i]>=hot-1]
        rows.append((row,3800+round(statistics.median(indices))))
    return rows
def body(im,centres,reference):
    px=im.load();rp=reference.load();peaks=[];bands=[]
    skies={row:median_rgb([rp[x+d,row] for d in range(-100,101)]) for row,x in centres}
    curve=[];counts=[]
    for d in range(-100,101):
        clean=[(row,x) for row,x in centres if max(abs(rp[x+d,row][i]-skies[row][i]) for i in range(3))<=4]
        assert len(clean)>=3, f'Insufficient plain-sky width samples at offset{d}: {len(clean)}'
        curve.append(statistics.median(y(px[x+d,row])-y(rp[x+d,row]) for row,x in clean))
        counts.append(len(clean))
    peak=max(range(96,105),key=lambda i:curve[i]);a=b=peak;threshold=40.
    if curve[peak]>threshold:
        while a>0 and curve[a-1]>threshold:a-=1
        while b<200 and curve[b+1]>threshold:b+=1
        left=a-(curve[a]-threshold)/max(.001,curve[a]-curve[a-1]) if a else a
        right=b+(curve[b]-threshold)/max(.001,curve[b]-curve[b+1]) if b<200 else b
        width=right-left
    else:width=0.
    for row,x in centres:
        values=[y(px[x+d,row]) for d in range(-100,101)]
        peak=max(range(96,105),key=lambda i:values[i])
        peaks.append(values[peak])
        bands.append(statistics.mean(values[94:107]))
    return dict(widthNative=width,widthFilm=width/2,minimumPlainSkyWidthSamples=min(counts),
        coreY=statistics.mean(bands),peakPixelY=max(peaks))
def halo(im,centres,reference):
    px=im.load();rp=reference.load();c={};off={};counts={}
    skies={row:median_rgb([rp[x+d,row] for d in range(-100,101)]) for row,x in centres}
    for d in range(-100,101):
        clean=[(row,x) for row,x in centres if max(abs(rp[x+d,row][i]-skies[row][i]) for i in range(3))<=4]
        assert len(clean)>=3, f'Insufficient plain-sky samples at offset{d}: {len(clean)}'
        counts[d]=len(clean)
        c[d]=median_rgb([px[x+d,row] for row,x in clean])
        off[d]=median_rgb([rp[x+d,row] for row,x in clean])
    sky=median_rgb([off[d] for d in [*range(-100,-79),*range(80,101)]])
    samples=[c[d] for d in [*range(-40,-9),*range(10,41)]]
    rgb=tuple(statistics.mean(p[i] for p in samples) for i in range(3))
    hue,sat,_=colorsys.rgb_to_hsv(*(v/255 for v in rgb))
    hues=[colorsys.rgb_to_hsv(*(v/255 for v in p))[0]*360 for p in samples]
    saturations=[colorsys.rgb_to_hsv(*(v/255 for v in p))[1] for p in samples]
    pinned=sum(abs(c[d][2]-off[d][2])<=4 and max(c[d][0]-off[d][0],c[d][1]-off[d][1])>=10
        for d in [*range(-40,-9),*range(10,41)])
    radiance={d:y(c[d])-y(off[d]) for d in c}
    steps=[abs(radiance[d+2]-radiance[d]) for d in [*range(-70,-11),*range(10,69)]]
    return dict(medianCurveRGB=rgb,hue=hue*360,saturation=sat,minHue=min(hues),maxHue=max(hues),
        meanHaloLiftY=statistics.mean(radiance[d] for d in [*range(-40,-9),*range(10,41)]),
        minimumSaturation=min(saturations),pinnedBluePixels=pinned,maximumOuter2pxStepY=max(steps),
        huePassed=205<=hue*360<=225,saturationPassed=sat>=.27,
        everyOffsetPassed=min(hues)>=205 and max(hues)<=225 and min(saturations)>=.27,
        edgePassed=max(steps)<=8,pinnedBluePassed=pinned==0,skyMedian=sky,minimumPlainSkySamples=min(counts.values()))
def desk(im):
    areas=[(0,0,2560,1440),(2560,0,6000,1440),(6000,195,7920,1275)]
    total=area=0
    for box in areas:
        count=(box[2]-box[0])*(box[3]-box[1]);total+=y(ImageStat.Stat(im.crop(box)).mean)*count;area+=count
    return total/area
def frames(scene):
    return {int(f['timelineNs']):scene/f['panorama'] for f in json.loads((scene/'manifest.json').read_text())['frames']}
def core(root,out,reference):
    rows=[];checks=[];out.mkdir(parents=True,exist_ok=True)
    for palette in ['default','ocean','spectrum','ember','forest','mono','pastel']:
        name='default-sequence' if palette=='default' else palette+'-probe'
        scene=root/'scenes'/name;fs=frames(scene);refs=frames(reference/'scenes'/name)
        logged=[tuple(map(float,row)) for row in re.findall(r'Matrix bolt fork root: ([\d.-]+),([\d.-]+), stamp ([\d.]+)',(scene/'gpu-tests.log').read_text())]
        assert logged, f'Missing native branch roots: {scene}'
        stamp=max(r[2] for r in logged)
        forks=set((a,b) for a,b,t in logged if t==stamp)
        assert 3<=len(forks)<=8,(palette,forks)
        with Image.open(fs[18133333333]) as source:centres=trace(source.convert('RGB'))
        for age,ns in [('main',18133333333),('after500',18500000000),('after833',18833333333)]:
            with Image.open(fs[ns]) as source,Image.open(refs[ns]) as reference_image:
                im=source.convert('RGB');ref=reference_image.convert('RGB');r=dict(palette=palette,age=age,**halo(im,centres,ref),**body(im,centres,ref));rows.append(r)
                x=round(statistics.median(c[1] for c in centres));im.crop((x-120,900,x+120,1100)).save(out/f'{palette}-{age}-halo-native.png')
                if age=='main':im.save(out/(palette+'-peak.png'))
                if age=='main':
                    trunk=max(y(im.getpixel((x+d,row))) for row,x in centres for d in range(-6,7))
                    junctions=[max(y(im.getpixel((round(x)+dx,round(y0)+dy))) for dx in range(-3,4) for dy in range(-3,4)) for x,y0 in sorted(forks)]
                    r['junctionPeaksY']=junctions;r['trunkPeakY']=trunk
                    r['junctionsPassed']=max(junctions)<=trunk+.00001
                    if not r['junctionsPassed']:checks.append(dict(palette=palette,check='junctionsPassed',values=r))
            print(palette,age,'hue',round(r['hue'],3),'sat',round(r['saturation'],4),'width',round(r['widthNative'],3),flush=True)
        with Image.open(fs[19833333333]) as source,Image.open(refs[19833333333]) as reference_image:
            tail=halo(source.convert('RGB'),centres,reference_image.convert('RGB'))
        last=next(r for r in rows if r['palette']==palette and r['age']=='after833')
        ratio=tail['meanHaloLiftY']/last['meanHaloLiftY']
        last['oneSecondLaterHaloLiftY']=tail['meanHaloLiftY'];last['oneSecondDecayRatio']=ratio
        last['decayPassed']=.35<=ratio<=.65
        if not last['decayPassed']:checks.append(dict(palette=palette,check='oneSecondDecay',values=last))
    weights=[]
    for phase,name,ns in [(0.,'default-sequence',18133333333),(1000/120,'phase-sequence',18125000000)]:
        scene=root/'scenes'/name
        with Image.open(frames(scene)[ns]) as source,Image.open(frames(reference/'scenes'/name)[ns]) as ref:
            im=source.convert('RGB');row=dict(phaseMs=phase,**body(im,trace(im),ref.convert('RGB')))
        row['passed']=22<=row['widthFilm']<=24;weights.append(row)
        if not row['passed']:checks.append(dict(check='mainWeight22to24FilmPixels',values=row))
    (out/'canonical-main-widths.json').write_text(json.dumps(weights,indent=2)+'\n')
    (out/'halo-checks.json').write_text(json.dumps(rows,indent=2)+'\n')
    for r in rows:
        for key in ['huePassed','saturationPassed','everyOffsetPassed','edgePassed','pinnedBluePassed']:
            if not r[key]:checks.append(dict(palette=r['palette'],age=r['age'],check=key,values=r))
    return checks
def many(root,expected,out,reference):
    out.mkdir(parents=True,exist_ok=True);rows=[];failures=[];strokes=[]
    for name,items in json.loads(expected.read_text()).items():
        scene=root/'scenes'/name;fs=frames(scene);rfs=frames(reference/'scenes'/name);cache={};references={}
        for ns in sorted(set([i['mainNs'] for i in items])):
            with Image.open(fs[ns]) as source,Image.open(rfs[ns]) as ref:im=source.convert('RGB');references[ns]=trace(im);cache[ns]=dict(deskY=desk(im),**body(im,references[ns],ref.convert('RGB')))
        for i in items:
            for ns in [i['beforeNs'],i['afterNs']]:
                if ns in cache:continue
                with Image.open(fs[ns]) as source,Image.open(rfs[ns]) as ref:
                    im=source.convert('RGB');cache[ns]=dict(deskY=desk(im),**body(im,references[i['mainNs']],ref.convert('RGB')))
            before=cache[i['beforeNs']];after=cache[i['afterNs']]
            previous=cache[i['mainNs']] if i['returnIndex']==1 else next(r for r in rows if r['scene']==name and r['phaseMs']==i['phaseMs'] and r['returnIndex']==i['returnIndex']-1)['after']
            row=dict(scene=name,**i,main=cache[i['mainNs']],previous=previous,before=before,after=after,riseY=after['deskY']-before['deskY'],
                monotonicWidth=after['widthNative']<=previous['widthNative'],monotonicCore=after['coreY']<=previous['coreY']+.00001,
                monotonicPeakPixel=after['peakPixelY']<=previous['peakPixelY']+.00001,
                rebrightensWidth=after['widthNative']>before['widthNative'],rebrightensCore=after['coreY']>before['coreY'])
            row['passed']=row['riseY']>=8 and all(row[k] for k in ['monotonicWidth','monotonicCore','monotonicPeakPixel','rebrightensWidth','rebrightensCore'])
            rows.append(row)
            if not row['passed']:failures.append(row)
        print(name,'minimum rise',round(min(r['riseY'] for r in rows if r['scene']==name),4),'passed',all(r['passed'] for r in rows if r['scene']==name),flush=True)
    summary=dict(seeds=len(json.loads(expected.read_text())),checks=len(rows),minimumRiseY=min(r['riseY'] for r in rows),failures=failures)
    (out/'many-checks.json').write_text(json.dumps(rows,indent=2)+'\n');(out/'many-summary.json').write_text(json.dumps(summary,indent=2)+'\n')
    return failures
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--capture',type=Path,required=True);p.add_argument('--reference',type=Path);p.add_argument('--output',type=Path,required=True);p.add_argument('--many-expectations',type=Path);p.add_argument('--diagnostic',action='store_true');a=p.parse_args()
    if not a.reference:p.error('Native checks require a paired --reference capture')
    failures=many(a.capture,a.many_expectations,a.output,a.reference) if a.many_expectations else core(a.capture,a.output,a.reference)
    print('Native failures:',len(failures),flush=True)
    if failures and not a.diagnostic:raise SystemExit(1)
