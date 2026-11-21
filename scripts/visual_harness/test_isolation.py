# SPDX-License-Identifier: GPL-3.0-or-later
import importlib.util
import os
from pathlib import Path
import socket
import sys
import tempfile
import unittest
from unittest.mock import patch

from visual_harness.isolation import (DIRECTORIES, SOCKET, bus_configuration,
                                      validated_environment)

CAPTURE_PATH = Path(__file__).resolve().parents[2] / 'tests/run-depth-capture.py'
SPEC = importlib.util.spec_from_file_location('depth_capture', CAPTURE_PATH)
capture = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(capture)


class IsolationTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='pvs-depth-')
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        for name in set(DIRECTORIES.values()):
            (self.root / name).mkdir(mode=0o700)
        (self.root / 'bus.conf').write_text(bus_configuration(self.root))
        (self.root / 'bus.conf').chmod(0o600)
        self.env = {key: str(self.root / name) for key, name in DIRECTORIES.items()}
        self.env.update(PATH=os.defpath + ':/usr/local/bin', LANG='C.UTF-8',
                        XDG_DATA_DIRS='/usr/local/share:/usr/share',
                        PVS_PRIVATE_HARNESS_ROOT=str(self.root),
                        DBUS_SYSTEM_BUS_ADDRESS='unix:path=' + str(self.root / 'no-system-bus'))
        self.bus = socket.socket(socket.AF_UNIX)
        self.addCleanup(self.bus.close)
        self.bus_path = self.root / 'runtime' / 'bus'
        self.bus.bind(str(self.bus_path))

    def worker_environment(self):
        return dict(self.env, DBUS_SESSION_BUS_ADDRESS='unix:path=' + str(self.bus_path))

    def test_bootstrap_worker_and_client(self):
        self.assertEqual(validated_environment(self.env, 'bootstrap'), self.env)
        worker = self.worker_environment()
        self.assertEqual(validated_environment(worker, 'worker'), worker)
        display = socket.socket(socket.AF_UNIX)
        self.addCleanup(display.close)
        display.bind(str(self.root / 'runtime' / SOCKET))
        client = dict(worker, WAYLAND_DISPLAY=SOCKET)
        self.assertEqual(validated_environment(client, 'client'), client)

    def test_direct_worker_real_session_never_launches(self):
        (self.root / 'output').mkdir()
        environment = dict(HOME='/home/test', DISPLAY=':0', WAYLAND_DISPLAY='wayland-0',
                           XDG_RUNTIME_DIR='/run/user/1000', XDG_CONFIG_HOME='/home/test/.config',
                           DBUS_SESSION_BUS_ADDRESS='unix:path=/run/user/1000/bus')
        with patch.dict(os.environ, environment, clear=True), patch.object(capture.subprocess, 'Popen') as popen:
            with patch.object(sys, 'argv', [str(CAPTURE_PATH), '--worker', '--build', str(self.root),
                                           '--output', str(self.root / 'output')]):
                with self.assertRaises(RuntimeError):
                    capture.main()
            popen.assert_not_called()

    def test_private_paths_do_not_authorize_inherited_access(self):
        for key, value in [('DISPLAY', ':0'), ('WAYLAND_DISPLAY', 'wayland-0'),
                           ('WAYLAND_SOCKET', '4'), ('XAUTHORITY', '/home/test/.Xauthority'),
                           ('DBUS_STARTER_ADDRESS', 'unix:path=/run/user/1000/bus'),
                           ('KDEHOME', '/home/test/.kde'), ('XDG_CONFIG_DIRS', '/etc/xdg'),
                           ('XDG_CONFIG_HOME', '/home/test/.config'),
                           ('XDG_RUNTIME_DIR', '/run/user/1000'),
                           ('DBUS_SESSION_BUS_ADDRESS', 'unix:path=/run/user/1000/bus'),
                           ('DBUS_SESSION_BUS_ADDRESS', 'unix:abstract=public-bus'),
                           ('DBUS_SESSION_BUS_ADDRESS', 'unix:path=' + str(self.bus_path) + ';unix:path=/run/user/1000/bus'),
                           ('DBUS_SYSTEM_BUS_ADDRESS', 'unix:path=/run/dbus/system_bus_socket')]:
            with self.subTest(key=key, value=value):
                with patch.dict(os.environ, dict(self.worker_environment(), **{key: value}), clear=True):
                    with patch.object(capture.subprocess, 'Popen') as popen:
                        with self.assertRaisesRegex(RuntimeError, 'Refusing capture'):
                            capture.worker(None) # must fail before reading args or launching anything
                        popen.assert_not_called()

    def test_runtime_permissions_symlinks_and_bus_config(self):
        runtime = self.root / 'runtime'
        runtime.chmod(0o755)
        with self.assertRaises(RuntimeError):
            validated_environment(self.worker_environment(), 'worker')
        runtime.chmod(0o700)
        config = self.root / 'config'
        config.rmdir()
        config.symlink_to(runtime, target_is_directory=True)
        with self.assertRaises(RuntimeError):
            validated_environment(self.worker_environment(), 'worker')
        config.unlink()
        config.mkdir(mode=0o700)
        (self.root / 'bus.conf').write_text('<busconfig><standard_session_servicedirs/></busconfig>')
        with self.assertRaises(RuntimeError):
            validated_environment(self.worker_environment(), 'worker')


if __name__ == '__main__':
    unittest.main()
