# SPDX-License-Identifier: GPL-3.0-or-later
"""Fail closed before any subprocess can reach a desktop display or bus."""
import os
from pathlib import Path
import re
import stat

DIRECTORIES = dict(HOME='home', TMPDIR='tmp', XDG_RUNTIME_DIR='runtime',
                   XDG_CONFIG_HOME='config', XDG_CONFIG_DIRS='config',
                   XDG_CACHE_HOME='cache', XDG_DATA_HOME='data')
SOCKET = 'depth-private-capture'
ALLOWED_KEYS = set(DIRECTORIES) | {
    'PATH', 'LANG', 'LC_CTYPE', 'XDG_DATA_DIRS', 'PVS_PRIVATE_HARNESS_ROOT',
    'DBUS_SYSTEM_BUS_ADDRESS', 'DBUS_SESSION_BUS_ADDRESS', 'DBUS_SESSION_BUS_PID',
    'QT_LOGGING_RULES', 'WAYLAND_DISPLAY', 'QT_QPA_PLATFORM', 'QSG_RHI_BACKEND',
    'QSG_INFO', 'QTEST_FUNCTION_TIMEOUT', 'VISUALS_REQUIRE_GPU', 'VISUALS_CAPTURE_DIR', 'PVS_VISUAL_CAPTURE_SCRIPT',
}


def bus_configuration(root):
    return f'''<busconfig><type>session</type><listen>unix:tmpdir={root / 'runtime'}</listen>
<auth>EXTERNAL</auth><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/>
<allow own="*"/></policy></busconfig>'''


def validated_environment(environment, phase):
    """Validate bootstrap (no bus), worker (no display), or private client env."""
    def refuse():
        raise RuntimeError('Refusing capture outside a validated private runtime, config and bus session')

    if phase not in ('bootstrap', 'worker', 'client'):
        refuse()
    if (set(environment) - ALLOWED_KEYS or environment.get('PATH') != os.defpath + ':/usr/local/bin' or
            environment.get('LANG') != 'C.UTF-8'):
        refuse()
    raw_root = environment.get('PVS_PRIVATE_HARNESS_ROOT', '')
    root = Path(raw_root)
    if (not raw_root or not root.is_absolute() or root.resolve() != root or
            not root.name.startswith('pvs-depth-')):
        refuse()
    for directory in [root, *(root / name for name in DIRECTORIES.values())]:
        try:
            info = directory.lstat()
        except OSError:
            refuse()
        if (not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid() or
                stat.S_IMODE(info.st_mode) != 0o700 or directory.resolve() != directory):
            refuse()
    for key, name in DIRECTORIES.items():
        if environment.get(key) != str(root / name):
            refuse()
    if (environment.get('XDG_DATA_DIRS') != '/usr/local/share:/usr/share' or
            environment.get('DBUS_SYSTEM_BUS_ADDRESS') != 'unix:path=' + str(root / 'no-system-bus') or
            (root / 'no-system-bus').exists()):
        refuse()
    # These variables bypass the private socket/bus paths or expose the user's
    # session. Even a valid private directory must not make them acceptable.
    if any(key in environment for key in ('DISPLAY', 'WAYLAND_SOCKET', 'XAUTHORITY', 'SESSION_MANAGER',
            'DBUS_STARTER_ADDRESS', 'DBUS_STARTER_BUS_TYPE', 'XDG_CONFIG_FILE', 'QT_QPA_PLATFORM_PLUGIN_PATH',
            'LD_PRELOAD', 'LD_LIBRARY_PATH')):
        refuse()
    if 'QTEST_FUNCTION_TIMEOUT' in environment and (phase != 'client' or environment['QTEST_FUNCTION_TIMEOUT'] != '3600000'):
        refuse()
    if phase == 'client':
        if environment.get('WAYLAND_DISPLAY') != SOCKET or not (root / 'runtime' / SOCKET).is_socket():
            refuse()
    elif 'WAYLAND_DISPLAY' in environment:
        refuse()
    bus_config = root / 'bus.conf'
    try:
        info = bus_config.lstat()
        if (not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or
                stat.S_IMODE(info.st_mode) & 0o022 or bus_config.read_text() != bus_configuration(root)):
            refuse()
    except OSError:
        refuse()
    address = environment.get('DBUS_SESSION_BUS_ADDRESS')
    if phase == 'bootstrap':
        if address is not None:
            refuse()
    else:
        # D-Bus permits multiple addresses, abstract sockets and percent escapes.
        # Accept only the one filesystem socket our private daemon creates.
        match = re.fullmatch(r'unix:path=([^,;%]+)(?:,guid=[0-9a-fA-F]{32})?', address or '')
        if not match:
            refuse()
        socket = Path(match.group(1))
        if (socket.parent != root / 'runtime' or socket.resolve() != socket or not socket.is_socket() or
                socket.stat().st_uid != os.getuid()):
            refuse()
    return dict(environment)
