#!/usr/bin/env python3
import importlib.util
import json
import subprocess
import sys
import tempfile
from pathlib import Path

width, height, fps = 96, 72, 60
black = bytes((0, 0, 0, 255)) * (width * height)
white = bytes((255, 255, 255, 255)) * (width * height)


def check(frames):
    result = subprocess.run([sys.argv[1], str(width), str(height), str(fps)],
                            input=b''.join(frames), capture_output=True, check=False)
    return result.returncode, json.loads(result.stdout)


def patch(x, y, w, h, rgb=(255, 255, 255), background=black):
    frame = bytearray(background)
    for row in range(y, y+h):
        start = (row*width+x)*4
        frame[start:start+w*4] = bytes((*rgb, 255))*w
    return bytes(frame)


def blink(lit, period, base=black):
    return [lit if 3 <= i % period < 8 else base for i in range(120)]


def staggered_blink(groups, period=15, rgb=(255, 255, 255)):
    frames = []
    for i in range(120):
        frame = black
        row = 18
        for delay, rows in enumerate(groups):
            if 3 <= i % period < 8 + delay:
                frame = patch(24, row, 16, rows, rgb=rgb, background=frame)
            row += rows
        frames.append(frame)
    return frames


assert check([black]*120)[0] == 0
rc, result = check([black]*60 + [white]*60)
assert rc == 1 and result['maxSwing'] > .99 and not result['areaQualifiedFlashes']
# The field is 32x24. A moving specular stays below the area threshold.
rc, result = check([patch(15+i%30, 24, 5, 5) for i in range(120)])
assert rc == 0 and not result['areaQualifiedFlashes']
# Strictly more than one quarter, including a cluster across fixed tile seams.
assert check(blink(patch(24, 18, 16, 12), 15))[0] == 0
large = patch(24, 18, 16, 13)
rc, result = check(blink(large, 15))
assert rc == 1 and result['peakFlashesPerSecond'] == 4 and result['maxSwing'] < .06
assert check(blink(large, 20))[0] == 0  # Exactly three flashes per second.
# Opposing transitions finishing on adjacent frames are one concurrent flash.
for groups in ((7, 6), (5, 4, 4)):
    rc, result = check(staggered_blink(groups))
    assert rc == 1 and result['peakFlashesPerSecond'] == 4, result
    assert result['areaQualifiedFlashes'] == 8, result
    assert result['maxSwing'] < .06, result
    assert check(staggered_blink(groups, period=20))[0] == 0
# A larger flash must not be counted repeatedly as the completion mask persists,
# including when each staggered group can qualify on its own.
rc, result = check(staggered_blink((13, 13, 13), period=20, rgb=(140, 0, 0)))
assert rc == 0 and result['peakFlashesPerSecond'] == 3, result
# Two distant clusters cannot be counted together in one field.
apart = patch(76, 50, 12, 12, background=patch(3, 1, 12, 12))
assert check(blink(apart, 15))[0] == 0
# A red transition can exceed the red criterion below 10% general luminance.
rc, result = check(blink(patch(24, 18, 16, 13, rgb=(140, 0, 0)), 15))
assert rc == 1 and result['peakFlashesPerSecond'] == 4
rc, result = check(staggered_blink((5, 4, 4), rgb=(140, 0, 0)))
assert rc == 1 and result['peakFlashesPerSecond'] == 4, result
# Same-chromaticity red shading below a 10% luminance change is not a red flash.
dim_red = patch(24, 18, 16, 13, rgb=(100, 0, 0))
assert check(blink(patch(24, 18, 16, 13, rgb=(140, 0, 0)), 15, dim_red))[0] == 0
# Both ends above 0.80 are excluded from the general threshold.
bright = bytes((235, 235, 235, 255)) * (width * height)
assert check(blink(patch(24, 18, 16, 13, background=bright), 15, bright))[0] == 0
# A panorama can span unequal screens; each uses its original viewing field,
# including when only a narrow seam detail appears in the encoded frame.
spec = importlib.util.spec_from_file_location('depth_light', Path(__file__).resolve().parents[1] / 'check-depth-luminance.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
scene = dict(outputs=[dict(x=0,y=0,width=2560,height=1440),dict(x=2560,y=0,width=3440,height=1440)],
             video=dict(crop=[2400,200,400,400]))
assert list(map(int,module.field_args(scene,False,3000,720))) == [0,0,1280,720,426,240, 1280,0,1720,720,573,240]
assert list(map(int,module.field_args(scene,True,400,400))) == [0,0,160,400,853,480, 160,0,240,400,1146,480]

# Round 1's lossless pilot seam was captured before the manifest included crop.
pilot = dict(visual='constellation', layout='l3', video=dict(seam=True),
             outputs=scene['outputs'] + [dict(x=6000,y=195,width=1920,height=1080)])
assert list(map(int,module.field_args(pilot,True,768,960))) == [0,0,384,960,853,480, 384,0,384,960,1146,480]

# PNG fallback creates a contact sheet next to the numbered recording frames.
capture_spec = importlib.util.spec_from_file_location(
    'depth_capture', Path(__file__).resolve().parents[1] / 'run-depth-capture.py')
capture = importlib.util.module_from_spec(capture_spec)
capture_spec.loader.exec_module(capture)
from PIL import Image
with tempfile.TemporaryDirectory(prefix='depth-light-gate-') as temporary:
    directory = Path(temporary)
    scene = dict(video=dict(frames=60, fps=fps), videoFrames=60)
    manifest = directory / 'manifest.json'
    manifest.write_text(json.dumps(scene))
    for i in range(60):
        Image.new('RGB', (width, height)).save(directory / f'world-{i:05d}.png')
    capture.finish_video(directory, scene)
    stream, = json.loads(manifest.read_text())['recordings']
    assert (directory / stream['contact']).is_file()
    result = module.check_sequence(manifest, stream, Path(sys.argv[1]))
    assert result['passed'] and result['frames'] == 60, result
    # Finishing again must also ignore the previously generated contact sheet.
    capture.finish_video(directory, scene)

print('Light gate and PNG recording regressions passed')
