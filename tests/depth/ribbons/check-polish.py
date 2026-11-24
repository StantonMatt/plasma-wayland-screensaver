#!/usr/bin/python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Check native Pastel-on-Black polish against its matched seed/time baseline."""
import argparse
import json
from pathlib import Path

import numpy as np
from PIL import Image


def brightest(path):
    rgb = np.asarray(Image.open(path).convert('RGB'), dtype=float) / 255
    value = rgb.max(axis=2)
    saturation = np.divide(value - rgb.min(axis=2), value,
                           out=np.zeros_like(value), where=value > 0)
    count = value.size // 100
    # Report both definitions of brightness; the release gate uses Rec.709.
    result = {}
    for label, brightness in [('value', value), ('luma', rgb @ [.2126, .7152, .0722])]:
        selected = np.argpartition(brightness.ravel(), -count)[-count:]
        result[label] = dict(pixels=count,
                             medianSaturation=float(np.median(saturation.ravel()[selected])),
                             medianValue=float(np.median(value.ravel()[selected])))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('before', type=Path)
    parser.add_argument('after', type=Path)
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    relative = Path('scenes/B-pastel/90000000000-world.png')
    result = {label: brightest(directory / relative)
              for label, directory in [('before', args.before), ('after', args.after)]}
    old, new = result['before']['luma'], result['after']['luma']
    result['passed'] = (new['medianValue'] >= .75
                        and new['medianValue'] >= old['medianValue'] + .10
                        and new['medianSaturation'] >= .22
                        and new['medianSaturation'] >= old['medianSaturation'] + .03)
    args.report.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
