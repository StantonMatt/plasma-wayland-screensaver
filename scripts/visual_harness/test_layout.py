# SPDX-License-Identifier: GPL-3.0-or-later
import copy
import tempfile
from pathlib import Path
import unittest
from unittest.mock import patch
from visual_harness.layout import L3, configure_layout, with_refresh_rates


class LayoutTest(unittest.TestCase):
    def test_real_refresh_mode_ids_and_geometry(self):
        roles = with_refresh_rates(L3, [59.95, 174.96, 239.76])
        before = {'outputs': [dict(id=i+1, name=f'Virtual-{i}', currentModeId=str(i*2+1),
                    pos={'x': i*2560, 'y': 0}, scale=1,
                    modes=[dict(id=str(i*2+1), size={'width': 2560, 'height': 1440}, refreshRate=60)])
                    for i in range(3)]}
        added = copy.deepcopy(before)
        committed = copy.deepcopy(before)
        for i, r in enumerate(roles):
            mode = dict(id=str(i*2+2), size={'width': r['width'], 'height': r['height']}, refreshRate=r['refreshHz'])
            added['outputs'][i]['modes'].append(mode)
            committed['outputs'][i].update(modes=added['outputs'][i]['modes'], currentModeId=mode['id'], pos={'x': r['x'], 'y': r['y']})
        import json
        with tempfile.TemporaryDirectory() as root, patch('visual_harness.layout.private_environment'), patch('visual_harness.layout.command') as command:
            command.side_effect = [json.dumps(before), '', json.dumps(added), '', json.dumps(committed)]
            configure_layout({}, Path(root), roles)
            args = command.call_args_list[3].args[0]
            self.assertIn('output.2.mode.4', args)
            self.assertIn('output.3.position.6000,195', args)
            self.assertIn('output.2.addCustomMode.3440.1440.174960.reduced', command.call_args_list[1].args[0])
        self.assertNotIn('refreshHz', L3[0])

    def test_invalid_refresh_requests(self):
        for rates in ([60], [60, float('nan'), 240], [60, 175, 0]):
            with self.assertRaises(ValueError):
                with_refresh_rates(L3, rates)
