// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    id: window
    required property var controller
    required property var screensaverConfig
    width: 620
    height: 840
    minimumWidth: 520
    minimumHeight: 760
    visible: true
    title: qsTr("Plasma Visual Screensaver")

    onClosing: function(close) {
        close.accepted = false
        window.hide()
    }

    function store() {
        window.screensaverConfig.idleMinutes = idleTimeout.value
        window.screensaverConfig.visualModule = visual.currentValue
        window.screensaverConfig.backgroundStyle = background.currentValue
        window.screensaverConfig.showClock = showClock.checked
        window.screensaverConfig.clockMovement = clockMovement.currentValue
        window.screensaverConfig.clockSpeed = clockSpeed.currentValue
        window.screensaverConfig.frameRate = frameRate.currentValue
        window.screensaverConfig.reducedMotion = reducedMotion.checked
        window.screensaverConfig.monitorBehavior = monitors.currentValue
        window.screensaverConfig.coverPanels = coverPanels.checked
        window.controller.saveSettings()
    }

    function visualIndex(value) {
        if (value === "aurora") return 1
        if (value === "orbs") return 2
        if (value === "bounce") return 3
        return 0
    }

    function backgroundIndex(value) {
        if (value === "midnight") return 1
        if (value === "ocean") return 2
        if (value === "plum") return 3
        return 0
    }

    function monitorIndex(value) {
        if (value === "synchronized") return 1
        if (value === "seamless") return 2
        return 0
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 26
        spacing: 16

        Label {
            text: qsTr("Visual Screensaver")
            font.pixelSize: 26
            font.weight: Font.DemiBold
        }

        Label {
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            color: palette.mid
            text: qsTr("A decorative overlay for an unlocked session. It does not lock the computer or protect your data.")
        }

        GridLayout {
            Layout.fillWidth: true
            columns: 2
            columnSpacing: 24
            rowSpacing: 14

            Label { text: qsTr("Start after") }
            RowLayout {
                SpinBox {
                    id: idleTimeout
                    from: 1
                    to: 240
                    value: window.screensaverConfig.idleMinutes
                }
                Label { text: qsTr("minutes of inactivity") }
            }

            Label { text: qsTr("Animation") }
            ComboBox {
                id: visual
                Layout.fillWidth: true
                textRole: "text"
                valueRole: "value"
                model: [
                    { text: qsTr("None"), value: "none" },
                    { text: qsTr("Aurora Drift"), value: "aurora" },
                    { text: qsTr("Floating Orbs"), value: "orbs" },
                    { text: qsTr("Bouncing Orb"), value: "bounce" }
                ]
                Component.onCompleted: currentIndex = window.visualIndex(window.screensaverConfig.visualModule)
            }

            Label { text: qsTr("Background") }
            ComboBox {
                id: background
                Layout.fillWidth: true
                textRole: "text"
                valueRole: "value"
                model: [
                    { text: qsTr("Pure Black"), value: "black" },
                    { text: qsTr("Midnight Gradient"), value: "midnight" },
                    { text: qsTr("Deep Ocean"), value: "ocean" },
                    { text: qsTr("Dark Plum"), value: "plum" }
                ]
                Component.onCompleted: currentIndex = window.backgroundIndex(window.screensaverConfig.backgroundStyle)
            }

            Label { text: qsTr("Clock") }
            CheckBox {
                id: showClock
                text: qsTr("Show clock and date")
                checked: window.screensaverConfig.showClock
            }

            Label { text: qsTr("Clock movement") }
            ComboBox {
                id: clockMovement
                Layout.fillWidth: true
                textRole: "text"
                valueRole: "value"
                model: [
                    { text: qsTr("Move around the display"), value: "bounce" },
                    { text: qsTr("Stay centered"), value: "center" }
                ]
                Component.onCompleted: currentIndex = window.screensaverConfig.clockMovement === "center" ? 1 : 0
                enabled: showClock.checked
            }

            Label { text: qsTr("Clock speed") }
            ComboBox {
                id: clockSpeed
                Layout.fillWidth: true
                textRole: "text"
                valueRole: "value"
                model: [
                    { text: qsTr("Slow"), value: "slow" },
                    { text: qsTr("Normal"), value: "normal" },
                    { text: qsTr("Fast"), value: "fast" }
                ]
                Component.onCompleted: {
                    currentIndex = window.screensaverConfig.clockSpeed === "slow" ? 0
                        : (window.screensaverConfig.clockSpeed === "fast" ? 2 : 1)
                }
                enabled: showClock.checked && clockMovement.currentValue === "bounce"
            }

            Label { text: qsTr("Motion") }
            CheckBox {
                id: reducedMotion
                text: qsTr("Reduced motion (static visual)")
                checked: window.screensaverConfig.reducedMotion
            }

            Label { text: qsTr("Frame rate") }
            ComboBox {
                id: frameRate
                Layout.fillWidth: true
                textRole: "text"
                valueRole: "value"
                model: [
                    { text: qsTr("15 fps — lowest power"), value: 15 },
                    { text: qsTr("30 fps — balanced"), value: 30 },
                    { text: qsTr("60 fps — smooth"), value: 60 }
                ]
                Component.onCompleted: currentIndex = window.screensaverConfig.frameRate === 15 ? 0
                                                        : (window.screensaverConfig.frameRate === 60 ? 2 : 1)
                enabled: !reducedMotion.checked
            }

            Label { text: qsTr("Multiple monitors") }
            ComboBox {
                id: monitors
                Layout.fillWidth: true
                textRole: "text"
                valueRole: "value"
                model: [
                    { text: qsTr("Independent motion on every monitor"), value: "independent" },
                    { text: qsTr("Synchronized motion on every monitor"), value: "synchronized" },
                    { text: qsTr("Seamless virtual desktop"), value: "seamless" }
                ]
                Component.onCompleted: currentIndex = window.monitorIndex(window.screensaverConfig.monitorBehavior)
            }

            Label { text: qsTr("Panels") }
            CheckBox {
                id: coverPanels
                text: qsTr("Cover taskbars and panels")
                checked: window.screensaverConfig.coverPanels
            }
        }

        Item { Layout.fillHeight: true }

        Label {
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            color: palette.mid
            text: qsTr("OLED tip: combine Pure Black with None or Bouncing Orb, a moving clock, seamless mode, and panel coverage to minimize static pixels. Any input dismisses the overlay; Plasma's lock-screen and power settings are unchanged.")
        }

        RowLayout {
            Layout.fillWidth: true

            Button {
                text: qsTr("Stop Background Process")
                onClicked: window.controller.Quit()
            }

            Item { Layout.fillWidth: true }

            Button {
                text: qsTr("Defaults")
                onClicked: {
                    idleTimeout.value = 10
                    visual.currentIndex = 1
                    background.currentIndex = 1
                    showClock.checked = true
                    clockMovement.currentIndex = 0
                    clockSpeed.currentIndex = 1
                    frameRate.currentIndex = 1
                    reducedMotion.checked = false
                    monitors.currentIndex = 0
                    coverPanels.checked = true
                }
            }
            Button {
                text: qsTr("Preview")
                onClicked: {
                    window.store()
                    window.controller.Preview()
                }
            }
            Button {
                text: qsTr("Save")
                highlighted: true
                onClicked: window.store()
            }
        }
    }
}
