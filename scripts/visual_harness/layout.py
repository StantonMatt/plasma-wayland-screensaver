# SPDX-License-Identifier: GPL-3.0-or-later
"""Exact logical L3 layout; generated connector names are mapped to stable roles."""
import json
import subprocess
import time
from .isolation import validated_environment

L3 = [dict(role='HDMI-A-5', x=0, y=0, width=2560, height=1440),
      dict(role='DP-5', x=2560, y=0, width=3440, height=1440),
      dict(role='DP-4', x=6000, y=195, width=1920, height=1080)]


def private_environment(environment):
    return validated_environment(environment, 'client')


def command(arguments, environment):
    return subprocess.check_output(arguments, env=private_environment(environment), text=True,
                                   stderr=subprocess.STDOUT, timeout=20)


def configure_l3(environment, directory):
    private_environment(environment)
    def state():
        return json.loads(command(['kscreen-doctor', '--json'], environment))
    before = state()
    outputs = sorted(before['outputs'], key=lambda o: o['id'])
    if len(outputs) != 3:
        raise RuntimeError('Private KWin did not expose exactly three outputs')
    (directory / 'layout-requested.json').write_text(json.dumps(dict(layout='l3', dpr=1, refreshHz=60, outputs=L3), indent=2))
    additions = []
    for output, role in zip(outputs, L3):
        if not any(m['size'] == dict(width=role['width'], height=role['height']) for m in output['modes']):
            additions.append(f"output.{output['id']}.addCustomMode.{role['width']}.{role['height']}.60000.reduced")
    if additions:
        command(['kscreen-doctor', *additions], environment)
    settings = []
    for output, role in zip(outputs, L3):
        prefix = f"output.{output['id']}"
        settings += [f"{prefix}.mode.{role['width']}x{role['height']}@60",
                     f"{prefix}.position.{role['x']},{role['y']}", f'{prefix}.scale.1']
    command(['kscreen-doctor', *settings], environment)
    deadline = time.monotonic() + 10
    while True:
        actual = state()
        observed = sorted(actual['outputs'], key=lambda o: o['id'])
        matches = len(observed) == 3
        for output, role in zip(observed, L3):
            mode = next((m for m in output['modes'] if m['id'] == output['currentModeId']), {})
            matches &= (mode.get('size') == dict(width=role['width'], height=role['height']) and
                        output['pos'] == dict(x=role['x'], y=role['y']) and output['scale'] == 1 and
                        abs(mode.get('refreshRate', 0) / (1000 if mode.get('refreshRate', 0) > 1000 else 1) - 60) < .1)
        if matches:
            break
        if time.monotonic() >= deadline:
            (directory / 'layout-observed.json').write_text(json.dumps(actual, indent=2))
            raise RuntimeError('Private output layout did not commit the exact L3 geometry/modes')
        time.sleep(.05)
    actual['roleMapping'] = [dict(role=r['role'], connector=o['name'], id=o['id']) for o, r in zip(observed, L3)]
    (directory / 'layout-observed.json').write_text(json.dumps(actual, indent=2))
    return actual
