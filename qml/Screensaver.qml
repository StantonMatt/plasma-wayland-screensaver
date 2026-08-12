// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import "visuals"

Item {
    id: root
    clip: true

    required property string visualModule
    required property string backgroundStyle
    required property int animationSpeed
    required property int animationDensity
    required property int animationScale
    required property string animationPalette
    required property int trailAmount
    required property int ballCount
    required property int ballGravity
    required property int ballElasticity
    required property bool ballCollisions
    required property bool showClock
    required property string clockMovement
    required property string clockSpeed
    required property int frameRate
    required property bool reducedMotion
    required property string monitorBehavior
    required property int seed
    required property double animationEpochMs
    required property real screenX
    required property real screenY
    required property real virtualX
    required property real virtualY
    required property real virtualWidth
    required property real virtualHeight
    required property var animationState
    required property var presentationClock

    property double clockMotionNowMs: animationEpochMs

    function visualSource() {
        switch (root.visualModule) {
        case "orbs": return "visuals/Orbs.qml"
        case "bounce": return "visuals/Bounce.qml"
        case "starfield": return "visuals/Starfield.qml"
        case "matrix": return "visuals/Matrix.qml"
        case "kaleidoscope": return "visuals/Kaleidoscope.qml"
        case "fireflies": return "visuals/Fireflies.qml"
        case "ribbons": return "visuals/Ribbons.qml"
        case "constellation": return "visuals/Constellation.qml"
        case "none": return ""
        default: return "visuals/Aurora.qml"
        }
    }

    function backgroundTop() {
        switch (root.backgroundStyle) {
        case "black": return "#000000"
        case "ocean": return "#02111d"
        case "plum": return "#18091d"
        default: return "#071329"
        }
    }

    function backgroundBottom() {
        switch (root.backgroundStyle) {
        case "black": return "#000000"
        case "ocean": return "#062f3c"
        case "plum": return "#310d2c"
        default: return "#13091e"
        }
    }

    function clockSpeedMultiplier() {
        if (root.clockSpeed === "slow") return 0.55
        if (root.clockSpeed === "fast") return 1.8
        return 1.0
    }

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

    function clockLocalX(itemWidth) {
        if (root.clockMovement !== "bounce" || root.reducedMotion)
            return (root.width - itemWidth) / 2
        if (root.monitorBehavior === "seamless")
            return root.animationState.clockX - root.screenX
        const elapsed = Math.max(0, root.clockMotionNowMs - root.animationEpochMs) / 1000
        const offset = root.seed * 19.37
        return root.pingPong(elapsed * 31 * root.clockSpeedMultiplier() + offset,
                             root.width - itemWidth)
    }

    function clockLocalY(itemHeight) {
        if (root.clockMovement !== "bounce" || root.reducedMotion)
            return (root.height - itemHeight) / 2
        if (root.monitorBehavior === "seamless")
            return root.animationState.clockY - root.screenY
        const elapsed = Math.max(0, root.clockMotionNowMs - root.animationEpochMs) / 1000
        const offset = root.seed * 11.83
        return root.pingPong(elapsed * 23 * root.clockSpeedMultiplier() + offset,
                             root.height - itemHeight)
    }

    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0; color: root.backgroundTop() }
            GradientStop { position: 1; color: root.backgroundBottom() }
        }
    }

    Loader {
        id: visualLoader
        anchors.fill: parent
        active: root.visualModule !== "none"
        source: root.visualSource()
        onLoaded: {
            item.context = root
            item.frameRate = root.frameRate
            item.reducedMotion = root.reducedMotion
            item.seed = root.seed
            item.animationEpochMs = root.animationEpochMs
        }
    }

    Column {
        id: clockBox
        x: root.clockLocalX(width)
        y: root.clockLocalY(height)
        spacing: 10
        visible: root.showClock
        onWidthChanged: {
            if (root.monitorBehavior === "seamless")
                root.animationState.setClockSize(width, height)
        }
        onHeightChanged: {
            if (root.monitorBehavior === "seamless")
                root.animationState.setClockSize(width, height)
        }

        Text {
            id: clock
            anchors.horizontalCenter: parent.horizontalCenter
            color: "#f6f8ff"
            font.family: "Noto Sans"
            font.pixelSize: root.monitorBehavior === "seamless"
                ? Math.max(48, Math.min(100, root.virtualHeight * 0.07))
                : Math.max(48, Math.min(root.width, root.height) * 0.115)
            font.weight: Font.Light
            font.letterSpacing: 2
            style: Text.Raised
            styleColor: "#40000000"
            text: Qt.formatTime(new Date(), "hh:mm")
        }

        Text {
            id: date
            anchors.horizontalCenter: parent.horizontalCenter
            color: "#c7d0e8"
            font.family: "Noto Sans"
            font.pixelSize: root.monitorBehavior === "seamless"
                ? Math.max(16, Math.min(28, root.virtualHeight * 0.02))
                : Math.max(16, Math.min(root.width, root.height) * 0.025)
            font.weight: Font.Normal
            text: Qt.formatDate(new Date(), "dddd, d MMMM")
        }
    }

    FrameClock {
        presentationClock: root.presentationClock
        running: root.showClock && root.clockMovement === "bounce" && !root.reducedMotion
                 && root.monitorBehavior !== "seamless"
        onTick: function(deltaSeconds) { root.clockMotionNowMs += deltaSeconds * 1000 }
    }

    Timer {
        interval: 1000
        running: root.showClock
        repeat: true
        onTriggered: {
            const now = new Date()
            clock.text = Qt.formatTime(now, "hh:mm")
            date.text = Qt.formatDate(now, "dddd, d MMMM")
        }
    }
}
