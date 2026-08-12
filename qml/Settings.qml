// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    id: window
    required property var controller
    required property var screensaverConfig
    width: 620
    height: 650
    minimumWidth: 520
    minimumHeight: 580
    visible: true
    title: qsTr("Plasma Visual Screensaver")

    onClosing: function(close) {
        close.accepted = false
        window.hide()
    }

    function store() {
        window.screensaverConfig.idleMinutes = idleTimeout.value
        window.screensaverConfig.visualModule = visual.currentValue
        window.screensaverConfig.showClock = showClock.checked
        window.screensaverConfig.frameRate = frameRate.currentValue
        window.screensaverConfig.reducedMotion = reducedMotion.checked
        window.screensaverConfig.monitorBehavior = monitors.currentValue
        window.controller.saveSettings()
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

            Label { text: qsTr("Visual") }
            ComboBox {
                id: visual
                Layout.fillWidth: true
                textRole: "text"
                valueRole: "value"
                model: [
                    { text: qsTr("Aurora Drift"), value: "aurora" },
                    { text: qsTr("Floating Orbs"), value: "orbs" }
                ]
                Component.onCompleted: currentIndex = window.screensaverConfig.visualModule === "orbs" ? 1 : 0
            }

            Label { text: qsTr("Clock") }
            CheckBox {
                id: showClock
                text: qsTr("Show clock and date")
                checked: window.screensaverConfig.showClock
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
                    { text: qsTr("Synchronized motion on every monitor"), value: "synchronized" }
                ]
                Component.onCompleted: currentIndex = window.screensaverConfig.monitorBehavior === "synchronized" ? 1 : 0
            }
        }

        Item { Layout.fillHeight: true }

        Label {
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            color: palette.mid
            text: qsTr("Any keyboard, pointer, touch, or resume activity dismisses every overlay immediately. Plasma's normal lock-screen and power settings are unchanged.")
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
                    visual.currentIndex = 0
                    showClock.checked = true
                    frameRate.currentIndex = 1
                    reducedMotion.checked = false
                    monitors.currentIndex = 0
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
