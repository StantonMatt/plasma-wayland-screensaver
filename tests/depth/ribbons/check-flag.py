#!/usr/bin/python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Measure the flag against the same unflapped material points and retain raw curvature."""
import argparse
import csv
import itertools
import json
from pathlib import Path
import numpy as np


def centreline(vertices):
    points = vertices[:, :2]
    tangent = vertices[:, 2:].copy()
    tangent /= np.maximum(np.linalg.norm(tangent, axis=1)[:, None], 1e-12)
    delta = np.diff(points, axis=0)
    chord = np.linalg.norm(delta, axis=1)[:, None]
    m0, m1 = tangent[:-1] * chord, tangent[1:] * chord
    c2, c3 = 3 * delta - 2 * m0 - m1, m0 + m1 - 2 * delta
    edges = [points[:-1] + t * (m0 + t * (c2 + t * c3)) for t in (0, 1/3, 2/3)]
    return np.vstack((np.stack(edges, axis=1).reshape(-1, 2), points[-1]))


def profile(curve, width, reference=None):
    ref = curve if reference is None else reference
    arc = np.r_[0, np.cumsum(np.linalg.norm(np.diff(ref, axis=0), axis=1))]
    s = np.linspace(0, arc[-1], max(3, int(np.ceil(arc[-1] / (width/32))) + 1))
    ds = s[1]
    def resample(v):
        return np.stack([np.interp(s, arc, coord) for coord in v.T], axis=1)
    q = resample(curve if reference is None else curve-reference)
    tangent = np.gradient(resample(ref), axis=0)
    tangent /= np.maximum(np.linalg.norm(tangent, axis=1)[:, None], 1e-12)
    normal = np.column_stack((-tangent[:, 1], tangent[:, 0]))
    if reference is not None:
        # Compare material offsets in each knot's transported frame. A rigid
        # rotation of the body rotates its normal too and introduces no flag.
        q = np.sum(q * normal, axis=1)[:, None]
    # Linearly continue both ends. This gives exactly zero on an infinite line.
    n = max(1, int(round(6*width/ds)))
    offset = np.arange(1, n+1)[:, None]
    extended = np.vstack((q[0]-(q[1]-q[0])*offset[::-1], q, q[-1]+(q[-1]-q[-2])*offset))
    acc = np.vstack((np.zeros(q.shape[1]), np.cumsum(extended, axis=0)))
    residual = (q - (acc[2*n+1:]-acc[:-2*n-1])/(2*n+1))
    signed = (residual[:, 0] if reference is not None else np.sum(residual * normal, axis=1))/width
    return s/width, signed


def stats(s, y, precision):
    zeros = np.flatnonzero(np.diff(np.signbit(y)))
    cross = s[zeros]-y[zeros]*(s[1]-s[0])/(y[zeros+1]-y[zeros])
    # A pair of sign changes below the precision of the exported float positions
    # cannot distinguish a wave from roundoff. Keep the raw y for amplitude.
    halves = [(i, c-b) for i, (a, b, c, d) in enumerate(zip(zeros, cross, cross[1:], zeros[1:]))
              if np.max(np.abs(y[a+1:d+1])) > precision]
    uniform = 0
    for a,b,c in zip(halves, halves[1:], halves[2:]):
        h = np.array([a[1], b[1], c[1]])
        if a[0]+2 == c[0] and max(abs(h/h.mean()-1)) <= .1:
            uniform += 1
    return dict(peak=float(max(abs(y))), head=float(max(abs(y[s <= .4*s[-1]]))),
                wavelength=min((2*h for _, h in halves), default=None), uniformRuns=uniform)


def summary(rows):
    return dict(samples=len(rows), peak=max(r['peak'] for r in rows), head=max(r['head'] for r in rows),
                minWavelength=min((r['wavelength'] for r in rows if r['wavelength'] is not None), default=None),
                uniformRuns=sum(r['uniformRuns'] for r in rows))


def observed_turn_cycles(timeline, heading):
    """Measure real heading reversals with a one-degree prominence floor."""
    direction = 0
    high = low = 0
    extrema = []
    threshold = np.deg2rad(1.)
    for i, angle in enumerate(heading):
        if direction >= 0 and angle >= heading[high]:
            high = i
        if direction <= 0 and angle <= heading[low]:
            low = i
        if direction >= 0 and angle <= heading[high] - threshold:
            if direction > 0:
                extrema.append(high)
            direction, low = -1, i
        elif direction <= 0 and angle >= heading[low] + threshold:
            if direction < 0:
                extrema.append(low)
            direction, high = 1, i
    return [dict(periodSeconds=float((timeline[c]-timeline[a])/30),
                 semiAmplitudeDegrees=float(np.rad2deg(abs(heading[b]-heading[a])+abs(heading[c]-heading[b]))/4))
            for a, b, c in zip(extrema, extrema[1:], extrema[2:])]


def head_motion(path):
    traces={}
    with open(path) as stream:
        for scene,tick,r,x,y,*context in csv.reader(stream):
            traces.setdefault((scene,r),[]).append((int(tick),np.arctan2(float(y),float(x)),int(any(int(v) for v in context))))
    measured=[]
    for (scene,r), series in traces.items():
        timeline, angle, active=np.array(series).T
        for ix in np.split(np.arange(len(angle)),np.flatnonzero(np.diff(timeline)>1)+1):
            if len(ix)<180: continue
            # Report the harmonic in actual head-heading acceleration at the flag drive
            # frequency. Convert the acceleration amplitude back to heading degrees.
            speed=3 if scene=='G' else 1
            omega=1.75*speed
            a=np.diff(np.unwrap(angle[ix]),2)
            t=timeline[ix][1:-1]/30
            matrix=np.stack((np.ones_like(t),np.sin(omega*t),np.cos(omega*t)),axis=1)
            coeff=np.linalg.lstsq(matrix,a,rcond=None)[0]
            amp=float(np.rad2deg(np.hypot(*coeff[1:])/(4*np.sin(omega/60)**2)))
            theta=np.unwrap(angle[ix])
            time=timeline[ix]/30
            tt=(time-time.mean())/np.ptp(time)
            trend=np.column_stack((np.ones_like(tt),tt,tt**2))
            basis=np.column_stack((trend,np.sin(omega*time),np.cos(omega*time)))
            fit=np.linalg.lstsq(basis,theta,rcond=None)[0]
            residual=theta-trend@np.linalg.lstsq(trend,theta,rcond=None)[0]
            explained=1-np.sum((theta-basis@fit)**2)/max(1e-16,np.sum(residual**2))
            measured.append(dict(scene=scene,strand=int(r),seconds=len(ix)/30,flagPeriodSeconds=2*np.pi/omega,
                accelerationHarmonicDegrees=amp,actualHeadingHarmonicDegrees=float(np.rad2deg(np.hypot(*fit[-2:]))),
                actualHeadingExcursionDegrees=float(np.rad2deg(np.ptp(theta))),explainedFraction=float(explained),
                observedTurnCycles=observed_turn_cycles(timeline[ix],theta),
                schoolOrWeatherFraction=float(np.mean(active[ix]))))
    return measured


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('csv');p.add_argument('--headings', required=True);p.add_argument('--report', required=True)
    args=p.parse_args()
    rows=[]
    with open(args.csv) as stream:
        for key, group in itertools.groupby(csv.reader(stream), key=lambda row:row[:5]):
            knots=np.asarray([row[5:] for row in group],dtype=float)
            width=float(key[4])
            full, zero=centreline(knots[:,:4]),centreline(knots[:,4:])
            # Four single-precision ULPs at the largest exported coordinate are
            # the conservative position/tangent/interpolation noise floor.
            noise=4*float(np.spacing(np.float32(np.max(abs(knots[:,[0,1,4,5]])))))/width
            s, relative=profile(full,width,zero)
            sf,yf=profile(full,width)
            sz,yz=profile(zero,width)
            rawdiff=float(max(abs(yf-np.interp(sf/sf[-1],sz/sz[-1],yz))))
            tip_full=full[1]-full[0];tip_zero=zero[1]-zero[0]
            heading_delta=float(abs(np.rad2deg(np.arctan2(tip_full[0]*tip_zero[1]-tip_full[1]*tip_zero[0],tip_full@tip_zero))))
            rows.append(dict(headDifferenceDegrees=heading_delta,scene=key[0],tick=int(key[1]),strand=int(key[2]),W=width,precisionW=noise,
                             relative=stats(s,relative,noise),literal=stats(sf,yf,noise),
                             control=stats(sz,yz,noise),literalMinusControl=rawdiff))
    result={k:summary([r[k] for r in rows]) for k in ('relative','literal','control')}
    result['maxLiteralDifference']=max(r['literalMinusControl'] for r in rows)
    result['maxWaveInducedHeadDegrees']=max(r['headDifferenceDegrees'] for r in rows)
    with Path(args.csv).with_suffix('.wave.csv').open() as stream:
        periods=np.array([[float(r[3]),float(r[4])] for r in csv.reader(stream)])
    result['localWavelengthInWidths']={k:dict(minimum=float(min(a)),median=float(np.median(a)),maximum=float(max(a)))
            for k,a in zip(('atFortyPercent','atTip'),periods.T)}
    head=head_motion(args.headings)
    print(json.dumps(result, indent=2))
    with open(args.report,'w') as f: json.dump(dict(summary=result,measurements=rows,headHeading=head),f,indent=2)
    g=result['relative']
    return g['peak']>.2 or g['head']>.05 or (g['minWavelength'] or 6)<6 or g['uniformRuns']>0


if __name__=='__main__':
    raise SystemExit(main())
