// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick

Item {
    id: root
    visible: false

    property bool running: false
    property var presentationClock
    property double presentationNanoseconds: 0
    signal tick(real deltaSeconds, real presentationNanoseconds)

    Connections {
        target: root.presentationClock
        enabled: root.running && root.presentationClock !== null
        function onPresentationTick(presentationNanoseconds) {
            root.presentationNanoseconds = presentationNanoseconds
        }
        function onFrameTick(deltaSeconds) {
            root.tick(deltaSeconds, root.presentationNanoseconds)
        }
    }
}
