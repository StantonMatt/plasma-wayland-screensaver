// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import Screensaver.Native 1.0

Item {
    id: root
    objectName: "snakeVisualRoot"
    clip: true

    property var context
    property int frameRate: 30
    property bool reducedMotion: false
    property int seed: 1
    property double animationEpochMs: 0

    SnakeRenderer {
        id: renderer
        objectName: "snakeNativeRenderer"
        anchors.fill: parent
        simulation: root.context ? root.context.snakeSimulation : null
        scaleToViewport: root.context && root.context.monitorBehavior === "synchronized"
        developerMode: root.context && root.context.developerMode === true
        shaderTimeFrozen: root.reducedMotion
    }
}
