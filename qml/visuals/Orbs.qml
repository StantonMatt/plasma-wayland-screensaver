// SPDX-License-Identifier: GPL-3.0-or-later
pragma ComponentBehavior: Bound
import QtQuick
import "VisualUtils.js" as Utils

Item {
    id: root
    property var context
    property int frameRate: 30
    property bool reducedMotion: false
    property int seed: 1
    property double animationEpochMs: Date.now()
    property real phase: (seed % 997) / 137.0

    readonly property var palette: Utils.colors(context ? context.animationPalette : "ocean")

    Repeater {
        model: root.context ? Math.round(3 + root.context.animationDensity / 7) : 10
        Rectangle {
            required property int index
            readonly property real sizeFactor: (0.055 + (index % 5) * 0.018)
                                                   * (root.context ? root.context.animationScale / 100 : 1)
            width: Math.min(root.width, root.height) * sizeFactor
            height: width
            radius: width / 2
            color: root.palette[index % root.palette.length]
            opacity: (0.12 + (index % 3) * 0.05)
                     + (root.context ? root.context.trailAmount / 500 : 0.07)
            x: root.width * (0.08 + ((index * 0.151 + Math.sin(root.phase + index)) % 0.82))
            y: root.height * (0.08 + ((index * 0.233 + Math.cos(root.phase * 0.73 + index)) % 0.78))
            scale: 0.9 + Math.sin(root.phase * 0.6 + index) * 0.14
        }
    }

    FrameClock {
        presentationClock: root.context ? root.context.presentationClock : null
        running: !root.reducedMotion
        onTick: function(deltaSeconds) {
            const speed = root.context ? root.context.animationSpeed / 100 : 1
            root.phase += deltaSeconds * 0.36 * speed
        }
    }
}
