// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import "../.."

Kirigami.FormLayout {
    id: form
    required property var settings
    required property var config
    readonly property var sharedLabels: ({speed: qsTr("Speed"), density: qsTr("Swarm size"), scale: qsTr("Size"), trail: qsTr("Light trails")})

    ValueSlider {
        Kirigami.FormData.label: qsTr("Synchrony:")
        accessibleName: qsTr("Synchrony")
        from: 0; to: 100; stepSize: 5
        value: form.config.visualParameters.synchrony
        onMoved: value => form.settings.writeVisualParameter("synchrony", value)
    }

    QQC2.CheckBox {
        text: qsTr("Night passes from dusk to dawn")
        checked: form.config.visualParameters.nightCycle === true
        onToggled: form.settings.writeVisualParameter("nightCycle", checked)
    }

    QQC2.CheckBox {
        text: qsTr("Night events")
        checked: form.config.visualParameters.nightEvents === true
        onToggled: form.settings.writeVisualParameter("nightEvents", checked)
    }

    QQC2.CheckBox {
        text: qsTr("Hunters lure other fireflies")
        checked: form.config.visualParameters.hunters === true
        onToggled: form.settings.writeVisualParameter("hunters", checked)
    }

    QQC2.CheckBox {
        text: qsTr("Grass lit by fireflies")
        checked: form.config.visualParameters.grass === true
        onToggled: form.settings.writeVisualParameter("grass", checked)
    }
}
