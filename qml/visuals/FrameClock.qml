// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick

Item {
    id: root
    visible: false

    property bool running: false
    property var presentationClock
    signal tick(real deltaSeconds)

    Connections {
        target: root.presentationClock
        enabled: root.running && root.presentationClock !== null
        function onFrameTick(deltaSeconds) {
            root.tick(deltaSeconds)
        }
    }
}
