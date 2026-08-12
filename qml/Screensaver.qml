// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick

Item {
    id: root
    required property string visualModule
    required property bool showClock
    required property int frameRate
    required property bool reducedMotion
    required property int seed
    required property double animationEpochMs
    required property string screenName

    Rectangle {
        anchors.fill: parent
        color: "#03050b"
    }

    Loader {
        anchors.fill: parent
        source: root.visualModule === "orbs" ? "visuals/Orbs.qml" : "visuals/Aurora.qml"
        onLoaded: {
            item.frameRate = root.frameRate
            item.reducedMotion = root.reducedMotion
            item.seed = root.seed
            item.animationEpochMs = root.animationEpochMs
        }
    }

    Column {
        anchors.centerIn: parent
        spacing: 10
        visible: root.showClock

        Text {
            id: clock
            anchors.horizontalCenter: parent.horizontalCenter
            color: "#f6f8ff"
            font.family: "Noto Sans"
            font.pixelSize: Math.max(48, Math.min(root.width, root.height) * 0.115)
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
            font.pixelSize: Math.max(16, Math.min(root.width, root.height) * 0.025)
            font.weight: Font.Normal
            text: Qt.formatDate(new Date(), "dddd, d MMMM")
        }
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

    Text {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 24
        color: "#6697a3bd"
        font.pixelSize: 12
        text: root.screenName
    }
}
