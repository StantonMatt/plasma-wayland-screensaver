// SPDX-License-Identifier: GPL-3.0-or-later
pragma ComponentBehavior: Bound
import QtQuick

Item {
    id: root
    property var context
    property int frameRate: 30
    property bool reducedMotion: false
    property int seed: 1
    property double animationEpochMs: Date.now()
    property real phase: (seed % 997) / 137.0

    Repeater {
        model: 7
        Rectangle {
            required property int index
            readonly property real sizeFactor: 0.10 + (index % 4) * 0.035
            width: Math.min(root.width, root.height) * sizeFactor
            height: width
            radius: width / 2
            color: ["#3a7bd5", "#7358d6", "#2aa889", "#d4619a"][index % 4]
            opacity: 0.18 + (index % 3) * 0.06
            x: root.width * (0.08 + ((index * 0.151 + Math.sin(root.phase + index)) % 0.82))
            y: root.height * (0.08 + ((index * 0.233 + Math.cos(root.phase * 0.73 + index)) % 0.78))
            scale: 0.9 + Math.sin(root.phase * 0.6 + index) * 0.14
        }
    }

    Timer {
        interval: Math.round(1000 / Math.max(1, root.frameRate))
        repeat: true
        running: !root.reducedMotion
        onTriggered: root.phase = (root.seed % 997) / 137.0
                                  + (Date.now() - root.animationEpochMs) * 0.00036
    }
}
