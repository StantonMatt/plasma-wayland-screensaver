#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Focused regression checks; no desktop windows, input, or daemon mutations."""
import argparse
import configparser
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch
import sys
import time

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location('benchmark_visuals', Path(__file__).with_name('benchmark-visuals.py'))
bench = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)


class BenchmarkTests(unittest.TestCase):
    def setUp(self):
        bench.SCRATCH.mkdir(parents=True, exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(prefix='test-', dir=bench.SCRATCH)
        self.root = Path(self.temporary.name)
        self.addCleanup(self.temporary.cleanup)

    def test_config_preserves_snakes_and_overrides_all_profiles(self):
        original = '[General]\nSnakeSelfCollisions=true\nSnakeIntelligence=100\nShowClock=true\nAnimationSettingsVersion=1\n\n[Animations][aurora]\nAnimationDensity=80\n'
        config = configparser.ConfigParser()
        config.optionxform = str
        config.read_string(bench.config_text(original, 'aurora', dict(AnimationDensity='66', ShowClock='false')))
        self.assertEqual(config['General']['SnakeSelfCollisions'], 'true')
        self.assertEqual(config['General']['SnakeIntelligence'], '100')
        self.assertEqual(config['General']['ShowClock'], 'false')
        self.assertEqual(config['General']['VisualModule'], 'aurora')
        for module in bench.VISUALS:
            self.assertEqual(config[f'Animations][{module}']['AnimationDensity'], '66')
            self.assertEqual(config[f'Animations][{module}']['AnimationPalette'], 'ocean')
        self.assertEqual(original.count('AnimationDensity=80'), 1)

    def test_proc_stat_comm_with_spaces_and_parentheses(self):
        fields = ['0'] * 22
        fields[11], fields[12], fields[21] = '123', '45', '256'
        with patch.object(Path, 'read_text', return_value='999 (odd ) name) ' + ' '.join(fields)):
            sample = bench.proc_sample(999)
        self.assertEqual(sample['ticks'], 168)
        self.assertEqual(sample['rss_mib'], 256 * bench.os.sysconf('SC_PAGE_SIZE') / 2**20)

    def test_virtual_environment_scrubs_desktop_and_isolates_vulkan(self):
        with patch.dict(bench.os.environ, DISPLAY=':0', WAYLAND_DISPLAY='real',
                        DBUS_SESSION_BUS_ADDRESS='real-session', QT_QUICK_BACKEND='inherited'):
            env = bench.session_environment(self.root, 'virtual', 'software', self.root / 'runtime')
        for key in ('DISPLAY', 'WAYLAND_DISPLAY', 'DBUS_SESSION_BUS_ADDRESS', 'QT_QUICK_BACKEND'):
            self.assertNotIn(key, env)
        self.assertEqual(env['LIBGL_ALWAYS_SOFTWARE'], '1')
        self.assertIn('VK_DRIVER_FILES', env)
        self.assertNotIn('nvidia', env['VK_DRIVER_FILES'])

    def test_seed_arguments(self):
        parser = bench.parser()
        self.assertEqual(bench.benchmark_seeds(parser.parse_args([])), [7])
        self.assertEqual(bench.benchmark_seeds(parser.parse_args(['--seed', '7', '--seed', '22'])), [7, 23])
        self.assertEqual(bench.benchmark_seeds(parser.parse_args(['--seed', '0', '--seed', '1'])), [1])
        self.assertEqual(bench.benchmark_seeds(parser.parse_args(['--random-seed'])), [None])
        self.assertEqual(bench.depth_seed('18446744073709551615'), 2**64 - 1)
        self.assertEqual(bench.depth_seed('0' * 5000 + '7'), 7)
        for value in ('', '-1', '+7', ' 7', '7 ', '1.5', '0x7', '18446744073709551616', '9' * 5000):
            with self.subTest(value=value[:30]), self.assertRaises(argparse.ArgumentTypeError):
                bench.depth_seed(value)
        with patch('sys.stderr', new=io.StringIO()), self.assertRaises(SystemExit):
            parser.parse_args(['--seed', '7', '--random-seed'])

    def test_seed_environment_and_acknowledgement(self):
        original = dict(PVS_DEPTH_SEED='999', OTHER='keep')
        self.assertEqual(bench.seed_environment(original, 23), dict(PVS_DEPTH_SEED='23', OTHER='keep'))
        self.assertEqual(bench.seed_environment(original, None), dict(OTHER='keep'))
        self.assertEqual(original['PVS_DEPTH_SEED'], '999')
        bench.validate_seed(dict(seed=23), 'PVS depth seed override: 23\n')
        bench.validate_seed(dict(seed=None), '')
        for log in ('', 'PVS depth seed override: 231\n', 'PVS depth seed override: 7\n'):
            with self.assertRaisesRegex(RuntimeError, 'not acknowledged'):
                bench.validate_seed(dict(seed=23), log)

    def result_fixture(self, entry, cpu):
        return dict(entry, cpu_percent=cpu, rss_mib=dict(peak=100), gpu=dict(devices={}),
                    windows={'1': dict(name='test', size=[100, 200], target_fps=60, refresh_hz=60,
                        fps=60, interval_ms=dict(p50=16, p95=18, p99=20), stalls=0)})

    def test_main_records_seeds_and_complete_abba_per_seed(self):
        binary = self.root / 'binary'
        binary.write_bytes(b'executable fixture')
        binary.chmod(0o700)
        for index, (options, seeds) in enumerate((([], [7]), (['--seed', '7', '--seed', '22'], [7, 23]),
                                                 (['--random-seed'], [None]))):
            root = self.root / str(index)
            def session(job, directory, args):
                for entry in job['entries']:
                    Path(entry['directory']).mkdir()
                    bench.write_json(Path(entry['directory']) / 'result.json', self.result_fixture(entry, 10))
            with patch.object(bench, 'run_session', side_effect=session), patch('sys.stdout', new=io.StringIO()):
                self.assertEqual(bench.main(['--binary', str(binary), '--candidate', str(binary), '--visuals', 'aurora',
                    '--repeats', '2', '--output-dir', str(root), '--config', str(self.root / 'missing'), *options]), 0)
            job = json.loads((root / 'job.json').read_text())
            report = json.loads((root / 'summary.json').read_text())
            self.assertEqual(job['seeds'], seeds)
            self.assertEqual([s['seed'] for s in report['per_seed']], seeds)
            self.assertEqual(len({e['directory'] for e in job['entries']}), 8 * len(seeds))
            for seed in seeds:
                for block in (1, 2):
                    self.assertEqual([r['label'] for r in report['runs'] if r['seed'] == seed and r['block'] == block],
                                     ['baseline', 'candidate', 'candidate', 'baseline'])

    def test_per_seed_deltas_are_not_hidden_by_pooled_improvement(self):
        entries = []
        for seed, baseline, candidate in ((7, 10, 12), (23, 100, 80)):
            for position, label in enumerate(('baseline', 'candidate', 'candidate', 'baseline')):
                directory = self.root / f'{seed}-{position}'
                directory.mkdir()
                entry = dict(visual='aurora', seed=seed, block=1, position=position, label=label, directory=str(directory))
                entries.append(entry)
                bench.write_json(directory / 'result.json', self.result_fixture(entry, baseline if label == 'baseline' else candidate))
        args = argparse.Namespace(repeats=1, max_cpu_regression=.1, max_frame_loss=None, max_p95_regression=None)
        with patch('sys.stdout', new=io.StringIO()):
            self.assertEqual(bench.summarize(dict(visuals=['aurora'], entries=entries, stall_ms=50), self.root, args), 1)
        report = json.loads((self.root / 'summary.json').read_text())
        pooled = report['visuals'][0]
        self.assertAlmostEqual(pooled['candidate_baseline_ratios']['cpu_percent'], 92 / 110)
        self.assertEqual(pooled['cpu_block_ratios'], [1.2, .8])
        self.assertEqual(pooled['cpu_noise_cv_percent'], 0)
        for group, delta in zip(report['per_seed'], (20, -20)):
            self.assertAlmostEqual(group['visuals'][0]['candidate_baseline_percent_changes']['cpu_percent'], delta)
        self.assertTrue(any('seed 7:' in f for f in report['failures']))
        self.assertIn('| 7 | aurora | 1.2000 | 20.00 |', (self.root / 'summary.md').read_text())

    def test_changed_binary_is_rejected_before_launch(self):
        binary = self.root / 'binary'
        binary.write_bytes(b'changed')
        job = dict(binaries=dict(baseline=dict(sha256='old-digest')))
        with self.assertRaisesRegex(RuntimeError, 'Binary changed'):
            bench.measure(job, dict(binary=str(binary), label='baseline'), {}, None)

    def test_output_identity_change_cannot_silently_compare_different_monitors(self):
        baseline = dict(windows={'1': dict(name='left', size=[100, 200], target_fps=60, refresh_hz=60)})
        candidate = dict(windows={'1': dict(name='right', size=[100, 200], target_fps=60, refresh_hz=60)})
        with self.assertRaisesRegex(RuntimeError, 'Output identity'):
            bench.validate_output_identity([baseline, candidate])

    def test_trace_uses_whole_window_and_excludes_boundary_interval(self):
        trace = self.root / 'frames.csv'
        trace.write_text('kind,window,timestamp_ns,interval_ns\n'
                         'frame,1,50000000,10000000\n'   # warmup
                         'frame,1,110000000,20000000\n'  # crosses start
                         'frame,1,140000000,30000000\n'
                         'frame,1,200000000,60000000\n'
                         'frame,1,310000000,110000000\n')  # after end
        log = 'Frame trace window 1 "test" size QSize(100, 200) target FPS 60 refresh Hz 60\n'
        result = bench.trace_summary(trace, log, 100000000, 300000000, 50, 1)['1']
        self.assertEqual(result['frames'], 3)
        self.assertEqual(result['fps'], 15)
        self.assertEqual(result['stalls'], 1)
        self.assertEqual(result['interval_ms']['p50'], 45)
        with self.assertRaisesRegex(RuntimeError, 'Expected 3'):
            bench.trace_summary(trace, log, 100000000, 300000000, 50, 3)

    def guard(self):
        guard = bench.IdleGuard.__new__(bench.IdleGuard)
        guard.process = Mock()
        guard.process.poll.return_value = None
        guard.process.stdout.fileno.return_value = 123
        guard.log = io.StringIO()
        guard.buffer = b''
        guard.ready = guard.idle = guard.inhibited = guard.running = False
        guard.heartbeat = time.monotonic()
        return guard

    def test_idle_resume_abort_and_failed_monitor(self):
        guard = self.guard()
        with patch.object(bench.os, 'read', return_value=b'READY\nIDLE\n'):
            guard.wait()
        self.assertTrue(guard.running)
        with patch.object(bench.os, 'read', return_value=b'RESUMED\n'):
            with self.assertRaisesRegex(InterruptedError, 'User input detected'):
                guard.events()
        guard.process.poll.return_value = 1
        with patch.object(bench.os, 'read', return_value=b''):
            with self.assertRaisesRegex(RuntimeError, 'Idle guard failed'):
                guard.events()

    def test_idle_lost_heartbeat_fails_closed(self):
        guard = self.guard()
        guard.heartbeat = time.monotonic() - 10
        with patch.object(bench.os, 'read', side_effect=BlockingIOError):
            with self.assertRaisesRegex(RuntimeError, 'heartbeat lost'):
                guard.events()

    def test_cleanup_stops_group_even_if_leader_has_exited(self):
        process = Mock(pid=1234)
        process.poll.return_value = 0
        with patch.object(bench.os, 'killpg') as kill:
            bench.stop(process, group=True)
        self.assertEqual([c.args for c in kill.call_args_list],
                         [(1234, bench.signal.SIGTERM), (1234, bench.signal.SIGKILL)])

    def test_daemon_stop_interruption_still_restores_original_unit(self):
        daemon = bench.DaemonPause(self.root)
        daemon.unit = 'original-autostart.service'
        daemon.original_pid = 1234
        with patch.object(bench, 'command', side_effect=['MainPID=1234\nActiveState=active\n', InterruptedError]):
            with self.assertRaises(InterruptedError):
                daemon.pause()
        self.assertTrue(daemon.restore_needed)
        with patch.object(bench, 'command', side_effect=['', 'active\n']) as run:
            daemon.restore()
        self.assertEqual(run.call_args_list[0].args[0], ['systemctl', '--user', 'start', daemon.unit])
        self.assertFalse(daemon.restore_needed)

    def test_daemon_user_stop_is_preserved(self):
        daemon = bench.DaemonPause(self.root)
        daemon.unit = 'original-autostart.service'
        daemon.original_pid = 1234
        with patch.object(bench, 'command', return_value='MainPID=0\nActiveState=inactive\n') as run:
            with self.assertRaisesRegex(RuntimeError, 'Daemon changed'):
                daemon.pause()
            daemon.restore()
        self.assertFalse(daemon.restore_needed)
        self.assertEqual(run.call_count, 1)

    def test_daemon_restore_retries_a_transient_start_failure(self):
        daemon = bench.DaemonPause(self.root)
        daemon.unit = 'original-autostart.service'
        daemon.restore_needed = True
        with patch.object(bench, 'command', side_effect=[RuntimeError('transient'), '', 'active\n']) as run, \
             patch.object(bench.time, 'sleep'):
            daemon.restore()
        self.assertFalse(daemon.restore_needed)
        self.assertEqual(run.call_count, 3)

    def test_no_daemon_never_starts_one(self):
        daemon = bench.DaemonPause(self.root)
        with patch.object(bench, 'command') as run:
            daemon.pause()
            daemon.restore()
        run.assert_not_called()

    def lifecycle(self, scenario):
        events = []
        daemon, guard, process = Mock(), Mock(), Mock()
        def restore():
            events.append('restore')
            if scenario == 'failed-restore':
                raise RuntimeError('mock service start failure')
        daemon.restore.side_effect = restore
        guard.close.side_effect = lambda: events.append('guard-close')
        process.poll.side_effect = [None, None, 0]
        process.returncode = 0
        guard.events.side_effect = [None, InterruptedError('mock input')] if scenario == 'input' else None
        if scenario == 'interrupt-stop':
            daemon.pause.side_effect = InterruptedError('mock Ctrl-C')
        if scenario == 'failed-preview':
            process.returncode = 1
        def teardown(p, group):
            events.append('stop-preview')
            if scenario == 'failed-cleanup':
                raise RuntimeError('mock cleanup failure')
        args = argparse.Namespace(target='real', virtual_renderer='software', wait_idle=1)
        with patch.object(bench, 'DaemonPause', return_value=daemon), \
             patch.object(bench, 'IdleGuard', return_value=guard), \
             patch.object(bench, 'build_idle_helper', return_value=self.root / 'fake-helper'), \
             patch.object(bench.subprocess, 'Popen', return_value=process), \
             patch.object(bench, 'stop', side_effect=teardown), \
             patch.dict(bench.os.environ, WAYLAND_DISPLAY='mock', XDG_RUNTIME_DIR='mock'), \
             patch.object(bench.time, 'sleep'):
            if scenario != 'success':
                with self.assertRaises((InterruptedError, RuntimeError)):
                    bench.run_session({}, self.root, args)
            else:
                bench.run_session({}, self.root, args)
        self.assertEqual(events[-3:], ['stop-preview', 'restore', 'guard-close'])
        self.assertFalse((self.root / 'private').exists())

    def test_lifecycle_success(self):
        self.lifecycle('success')

    def test_lifecycle_user_return(self):
        self.lifecycle('input')

    def test_lifecycle_ctrl_c_during_daemon_stop(self):
        self.lifecycle('interrupt-stop')

    def test_lifecycle_failed_preview(self):
        self.lifecycle('failed-preview')

    def test_lifecycle_restore_failure_still_releases_guard(self):
        self.lifecycle('failed-restore')

    def test_lifecycle_preview_teardown_failure_still_restores(self):
        self.lifecycle('failed-cleanup')

    def test_gate_rejects_single_output_loss_and_cpu_regression(self):
        entries = []
        for position, label in enumerate(('baseline', 'candidate', 'candidate', 'baseline')):
            directory = self.root / str(position)
            directory.mkdir()
            entry = dict(visual='aurora', label=label, block=1, directory=str(directory))
            entries.append(entry)
            run = dict(entry, cpu_percent=10 if label == 'baseline' else 13,
                       rss_mib=dict(peak=100), gpu=dict(devices={}), windows={})
            for window in ('1', '2', '3'):
                run['windows'][window] = dict(fps=30 if label == 'candidate' and window == '2' else 60,
                                            interval_ms=dict(p50=16, p95=18, p99=20), stalls=0)
            bench.write_json(directory / 'result.json', run)
        job = dict(visuals=['aurora'], entries=entries, stall_ms=50)
        args = argparse.Namespace(repeats=1, max_cpu_regression=.1, max_frame_loss=.1, max_p95_regression=None)
        with patch('sys.stdout', new=io.StringIO()):
            self.assertEqual(bench.summarize(job, self.root, args), 1)
        report = json.loads((self.root / 'summary.json').read_text())
        self.assertTrue(any('output 2' in f for f in report['failures']))
        self.assertTrue(any('cpu_percent' in f for f in report['failures']))
        row = report['visuals'][0]
        self.assertEqual(row['cpu_block_ratios'], [1.3])
        self.assertEqual(row['cpu_noise_cv_percent'], 0)
        self.assertEqual(row['output_ratios']['2']['fps'], .5)


if __name__ == '__main__':
    unittest.main()
