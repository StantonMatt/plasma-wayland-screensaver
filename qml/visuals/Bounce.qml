// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick

Item {
    id: root
    clip: true

    property var context
    property int frameRate: 30
    property bool reducedMotion: false
    property int seed: 1
    property double animationEpochMs: Date.now()
    property double nowMs: animationEpochMs

    readonly property real ballSize: context
        ? (context.monitorBehavior === "seamless"
           ? context.animationState.ballSize
           : Math.max(56, Math.min(120, Math.min(width, height) * 0.09)))
        : 80

    function positiveModulo(value, modulus) {
        if (modulus <= 0)
            return 0
        return ((value % modulus) + modulus) % modulus
    }

    function pingPong(distance, span) {
        if (span <= 0)
            return 0
        const position = root.positiveModulo(distance, span * 2)
        return position <= span ? position : span * 2 - position
    }

    function elapsedSeconds() {
        return Math.max(0, root.nowMs - root.animationEpochMs) / 1000
    }

    function localX() {
        if (!root.context)
            return 0
        if (root.context.monitorBehavior === "seamless")
            return root.context.animationState.ballX - root.context.screenX
        const elapsed = root.elapsedSeconds()
        return root.pingPong(elapsed * 176 + root.seed * 37,
                             root.width - root.ballSize)
    }

    function localY() {
        if (!root.context)
            return 0
        if (root.context.monitorBehavior === "seamless")
            return root.context.animationState.ballY - root.context.screenY
        const elapsed = root.elapsedSeconds()
        return root.pingPong(elapsed * 127 + root.seed * 23,
                             root.height - root.ballSize)
    }

    Rectangle {
        id: ball
        x: root.localX()
        y: root.localY()
        width: root.ballSize
        height: width
        radius: width / 2
        border.width: Math.max(2, width * 0.035)
        border.color: "#b7f5ff"
        gradient: Gradient {
            GradientStop { position: 0; color: "#d9fbff" }
            GradientStop { position: 0.28; color: "#3dd6e8" }
            GradientStop { position: 1; color: "#2856c7" }
        }
    }

    Timer {
        interval: Math.round(1000 / Math.max(1, root.frameRate))
        repeat: true
        running: !root.reducedMotion && (!root.context || root.context.monitorBehavior !== "seamless")
        onTriggered: root.nowMs = Date.now()
    }
}
