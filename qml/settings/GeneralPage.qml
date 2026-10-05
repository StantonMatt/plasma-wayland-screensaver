// SPDX-License-Identifier: GPL-3.0-or-later
// When and where the screensaver runs. Every change applies immediately.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Kirigami.ScrollablePage {
    id: page

    required property var settings
    readonly property var config: settings.screensaverConfig
    readonly property bool multipleMonitors: settings.controller.monitorCount > 1

    title: qsTr("General")

    actions: [
        Kirigami.Action {
            text: qsTr("Defaults")
            icon.name: "edit-reset"
            tooltip: qsTr("Restore the default general settings")
            displayHint: Kirigami.DisplayHint.KeepVisible
            enabled: !page.settings.generalIsDefault
            onTriggered: page.settings.restoreDefaults("general")
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

    Kirigami.FormLayout {
        RowLayout {
            Kirigami.FormData.label: qsTr("Start after:")
            Kirigami.FormData.buddyFor: idle
            spacing: Kirigami.Units.smallSpacing

            QQC2.SpinBox {
                id: idle
                from: 1; to: 240
                value: page.config.idleMinutes
                textFromValue: (value, locale) => value === 1 ? qsTr("1 minute") : qsTr("%1 minutes").arg(value)
                valueFromText: (text, locale) => Math.max(1, parseInt(text) || 1)
                onValueModified: page.settings.write("idleMinutes", value)
                Accessible.name: qsTr("Start after")
            }
            QQC2.Label {
                text: qsTr("of inactivity")
            }
        }

        // The one caveat worth stating where activation is decided.
        QQC2.Label {
            Layout.fillWidth: true
            Layout.maximumWidth: Kirigami.Units.gridUnit * 20
            text: qsTr("This doesn't lock your session.")
            wrapMode: Text.WordWrap
            // 0.7 opacity keeps at least 5.5:1 contrast in Breeze Light and 8:1 in Breeze Dark.
            opacity: 0.7
        }

        // ---- Monitors ---------------------------------------------------
        Kirigami.Separator {
            Kirigami.FormData.isSection: true
            Kirigami.FormData.label: qsTr("Monitors")
        }

        MonitorModeChooser {
            Kirigami.FormData.label: qsTr("Multiple monitors:")
            visible: page.multipleMonitors
            currentValue: page.config.monitorBehavior
            onActivated: value => page.settings.write("monitorBehavior", value)
        }

        QQC2.CheckBox {
            Kirigami.FormData.label: page.multipleMonitors ? "" : qsTr("Panels:")
            text: qsTr("Cover panels and taskbars")
            checked: page.config.coverPanels
            onToggled: page.settings.write("coverPanels", checked)
        }

        // ---- Motion -----------------------------------------------------
        Kirigami.Separator {
            Kirigami.FormData.isSection: true
            Kirigami.FormData.label: qsTr("Motion")
        }

        QQC2.ComboBox {
            Kirigami.FormData.label: qsTr("Frame rate:")
            Layout.minimumWidth: Kirigami.Units.gridUnit * 12
            // Reduced motion freezes every animation except Slithering Snakes.
            enabled: !page.config.reducedMotion || page.config.visualModule === "snakes"
            textRole: "text"
            valueRole: "value"
            model: [
                { text: qsTr("Match each monitor"), value: 0 },
                { text: qsTr("15 fps (lowest power)"), value: 15 },
                { text: qsTr("24 fps"), value: 24 },
                { text: qsTr("30 fps (low power)"), value: 30 },
                { text: qsTr("45 fps"), value: 45 },
                { text: qsTr("60 fps"), value: 60 },
                { text: qsTr("75 fps"), value: 75 },
                { text: qsTr("90 fps"), value: 90 },
                { text: qsTr("100 fps"), value: 100 },
                { text: qsTr("120 fps"), value: 120 },
                { text: qsTr("144 fps"), value: 144 },
                { text: qsTr("165 fps"), value: 165 },
                { text: qsTr("175 fps"), value: 175 },
                { text: qsTr("200 fps"), value: 200 },
                { text: qsTr("240 fps"), value: 240 }
            ]
            currentIndex: count > 0 ? indexOfValue(page.config.frameRate) : -1
            onActivated: page.settings.write("frameRate", currentValue)
            Accessible.name: qsTr("Frame rate")
        }

        // Only when the chosen rate can't apply to the current animation.
        QQC2.Label {
            Layout.fillWidth: true
            Layout.maximumWidth: Kirigami.Units.gridUnit * 20
            visible: page.config.visualModule === "snakes"
                     && (page.config.frameRate === 0 || page.config.frameRate > 60)
            text: qsTr("Slithering Snakes runs at up to 60 fps.")
            wrapMode: Text.WordWrap
            opacity: 0.7
        }

        QQC2.CheckBox {
            text: qsTr("Reduce motion")
            checked: page.config.reducedMotion
            onToggled: page.settings.write("reducedMotion", checked)
        }

        // ---- Background service ------------------------------------------
        Kirigami.Separator {
            Kirigami.FormData.isSection: true
        }

        QQC2.Button {
            Kirigami.FormData.label: qsTr("Background service:")
            text: qsTr("Stop")
            icon.name: "media-playback-stop"
            QQC2.ToolTip.text: qsTr("Stops the screensaver until you next log in or open this app")
            QQC2.ToolTip.visible: hovered || visualFocus
            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            onClicked: page.settings.quit()
        }
    }
}
