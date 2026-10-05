// SPDX-License-Identifier: GPL-3.0-or-later
// What the screensaver shows: animation, background, clock, and the options
// of the selected animation. Every change applies immediately.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import "Catalog.js" as Catalog

Kirigami.ScrollablePage {
    id: page

    required property var settings      // the window: write(), restoreDefaults(), preview()
    readonly property var config: settings.screensaverConfig
    readonly property string visual: config.visualModule
    readonly property var animation: Catalog.animation(visual)

    title: qsTr("Appearance")

    actions: [
        Kirigami.Action {
            text: qsTr("Defaults")
            icon.name: "edit-reset"
            tooltip: qsTr("Restore the default appearance")
            displayHint: Kirigami.DisplayHint.KeepVisible
            enabled: !page.settings.appearanceIsDefault
            onTriggered: page.settings.restoreDefaults("appearance")
        },
        Kirigami.Action {
            text: qsTr("Preview")
            icon.name: "media-playback-start"
            tooltip: qsTr("Show the screensaver now")
            displayHint: Kirigami.DisplayHint.KeepVisible
            onTriggered: page.settings.preview()
        }
    ]

    header: PreviewErrorMessage { settings: page.settings }

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing * 2

        // Explains why motion settings have no visible effect.
        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: page.config.reducedMotion
            type: Kirigami.MessageType.Information
            text: qsTr("Reduce motion is on, so animations stay still.")
            actions: Kirigami.Action {
                text: qsTr("Turn Off")
                onTriggered: page.settings.write("reducedMotion", false)
            }
        }

        AnimationGallery {
            Layout.fillWidth: true
            // Keeps tiles from growing huge in a maximized window.
            Layout.maximumWidth: Kirigami.Units.gridUnit * 60
            Layout.alignment: Qt.AlignHCenter
            Layout.preferredHeight: implicitHeight
            currentValue: page.visual
            backgroundStyle: page.config.backgroundStyle
            showClock: page.config.showClock
            onActivated: value => page.settings.write("visualModule", value)
            onPreviewRequested: value => {
                page.settings.write("visualModule", value)
                page.settings.preview()
            }
        }

        Kirigami.FormLayout {
            Layout.fillWidth: true

            SwatchPicker {
                Kirigami.FormData.label: qsTr("Background:")
                accessibleName: qsTr("Background")
                model: Catalog.backgrounds
                currentValue: page.config.backgroundStyle
                onActivated: value => page.settings.write("backgroundStyle", value)
            }

            QQC2.ComboBox {
                Kirigami.FormData.label: qsTr("Clock:")
                Layout.minimumWidth: Kirigami.Units.gridUnit * 10
                // Index = Configuration.clockMode: Off, Centered, Slowly, Drifting, Quickly.
                model: [qsTr("Off"), qsTr("Centered"), qsTr("Drifting slowly"), qsTr("Drifting"), qsTr("Drifting quickly")]
                currentIndex: page.config.clockMode
                onActivated: index => page.settings.write("clockMode", index)
                Accessible.name: qsTr("Clock")
            }

            // ---- Options of the selected animation ----------------------
            Kirigami.Separator {
                Kirigami.FormData.isSection: true
                Kirigami.FormData.label: page.animation.name
                visible: page.visual !== "none"
            }

            SwatchPicker {
                Kirigami.FormData.label: qsTr("Colors:")
                visible: page.visual !== "none"
                accessibleName: qsTr("Colors")
                model: Catalog.palettes
                currentValue: page.config.animationPalette
                onActivated: value => page.settings.write("animationPalette", value)
            }

            ValueSlider {
                Kirigami.FormData.label: qsTr("%1:").arg(page.animation.speed || "")
                visible: page.visual !== "none"
                accessibleName: page.animation.speed || ""
                from: 10; to: 300; stepSize: 10
                value: page.config.animationSpeed
                onMoved: value => page.settings.write("animationSpeed", value)
            }

            QQC2.SpinBox {
                Kirigami.FormData.label: qsTr("Balls:")
                visible: page.visual === "bounce"
                from: 1; to: 20
                value: page.config.ballCount
                onValueModified: page.settings.write("ballCount", value)
                Accessible.name: qsTr("Balls")
            }

            ValueSlider {
                Kirigami.FormData.label: page.animation.density ? qsTr("%1:").arg(page.animation.density) : ""
                visible: page.visual !== "none" && !!page.animation.density
                accessibleName: page.animation.density || ""
                from: 10; to: 100; stepSize: 5
                value: page.config.animationDensity
                onMoved: value => page.settings.write("animationDensity", value)
            }

            ValueSlider {
                Kirigami.FormData.label: qsTr("%1:").arg(page.animation.scale || "")
                visible: page.visual !== "none"
                accessibleName: page.animation.scale || ""
                from: 25; to: 200; stepSize: 5
                value: page.config.animationScale
                onMoved: value => page.settings.write("animationScale", value)
            }

            ValueSlider {
                Kirigami.FormData.label: qsTr("%1:").arg(page.animation.trail || "")
                visible: page.visual !== "none"
                accessibleName: page.animation.trail || ""
                from: 0; to: 100; stepSize: 5
                value: page.config.trailAmount
                onMoved: value => page.settings.write("trailAmount", value)
            }

            // Bouncing Balls ------------------------------------------------
            ValueSlider {
                Kirigami.FormData.label: qsTr("Gravity:")
                visible: page.visual === "bounce"
                accessibleName: qsTr("Gravity")
                from: -100; to: 100; stepSize: 5
                value: page.config.ballGravity
                valueText: shownValue < -2 ? qsTr("Up %1").arg(-shownValue)
                         : shownValue > 2 ? qsTr("Down %1").arg(shownValue) : qsTr("Zero-G")
                widestText: qsTr("Down %1").arg(100)
                onMoved: value => page.settings.write("ballGravity", value)
            }

            ValueSlider {
                Kirigami.FormData.label: qsTr("Bounciness:")
                visible: page.visual === "bounce"
                accessibleName: qsTr("Bounciness")
                from: 50; to: 100; stepSize: 1
                value: page.config.ballElasticity
                onMoved: value => page.settings.write("ballElasticity", value)
            }

            QQC2.CheckBox {
                visible: page.visual === "bounce"
                text: qsTr("Balls collide with each other")
                checked: page.config.ballCollisions
                onToggled: page.settings.write("ballCollisions", checked)
            }

            // Slithering Snakes ---------------------------------------------
            ValueSlider {
                Kirigami.FormData.label: qsTr("Intelligence:")
                visible: page.visual === "snakes"
                accessibleName: qsTr("Intelligence")
                from: 0; to: 100; stepSize: 5
                value: page.config.snakeIntelligence
                onMoved: value => page.settings.write("snakeIntelligence", value)
            }

            ValueSlider {
                Kirigami.FormData.label: qsTr("Aggression:")
                visible: page.visual === "snakes"
                accessibleName: qsTr("Aggression")
                from: 0; to: 100; stepSize: 5
                value: page.config.snakeAggression
                onMoved: value => page.settings.write("snakeAggression", value)
            }

            Item {
                Kirigami.FormData.isSection: true
                visible: page.visual === "snakes"
            }

            QQC2.CheckBox {
                Kirigami.FormData.label: qsTr("Rules:")
                visible: page.visual === "snakes"
                text: qsTr("Power-ups")
                checked: page.config.snakePowerUps
                onToggled: page.settings.write("snakePowerUps", checked)
            }
            QQC2.CheckBox {
                visible: page.visual === "snakes" && Catalog.snakeWorldEventsAvailable
                text: qsTr("World events")
                checked: page.config.snakeWorldEvents
                onToggled: page.settings.write("snakeWorldEvents", checked)
            }
            QQC2.CheckBox {
                visible: page.visual === "snakes"
                text: qsTr("Snakes can crash into themselves")
                checked: page.config.snakeSelfCollisions
                onToggled: page.settings.write("snakeSelfCollisions", checked)
            }
            QQC2.CheckBox {
                visible: page.visual === "snakes"
                text: qsTr("Screen edges are deadly")
                checked: page.config.snakeDeadlyWalls
                onToggled: page.settings.write("snakeDeadlyWalls", checked)
            }
            QQC2.CheckBox {
                visible: page.visual === "snakes"
                text: qsTr("Limit snake length")
                checked: page.config.snakeLengthLimit
                onToggled: page.settings.write("snakeLengthLimit", checked)
            }
        }
    }
}
