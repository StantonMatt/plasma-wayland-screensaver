#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Provide the required ScreenSaver inhibitor on the soak's private session bus."""
import os
import sys
from pathlib import Path

import dbus.service
from dbus.mainloop.glib import DBusGMainLoop
from gi.repository import GLib

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'scripts'))
from visual_harness.isolation import validated_environment


class ScreenSaver(dbus.service.Object):
    @dbus.service.method('org.freedesktop.ScreenSaver', in_signature='ss', out_signature='u')
    def Inhibit(self, application, reason):
        return 1

    @dbus.service.method('org.freedesktop.ScreenSaver', in_signature='u', out_signature='')
    def UnInhibit(self, cookie):
        pass


if __name__ == '__main__':
    validated_environment(dict(os.environ), 'client')
    DBusGMainLoop(set_as_default=True)
    bus = dbus.SessionBus()
    service = ScreenSaver(bus, '/ScreenSaver')
    name = dbus.service.BusName('org.freedesktop.ScreenSaver', bus, do_not_queue=True)
    GLib.MainLoop().run()
