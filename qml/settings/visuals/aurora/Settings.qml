// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import "../.."

Kirigami.FormLayout {
    id: form
    required property var settings
    required property var config
    readonly property var sharedLabels: ({speed: qsTr("Speed"), density: qsTr("Curtains"), scale: qsTr("Height"), trail: qsTr("Afterglow")})

    Kirigami.Separator {
        Kirigami.FormData.isSection: true
        Kirigami.FormData.label: qsTr("Sky")
    }

    ValueSlider {
        Kirigami.FormData.label: qsTr("Activity:")
        accessibleName: qsTr("Activity")
        from: 0; to: 100; stepSize: 5
        value: form.config.visualParameters.activity
        onMoved: value => form.settings.writeVisualParameter("activity", value)
    }

    QQC2.ComboBox {
        Kirigami.FormData.label: qsTr("Horizon:")
        Accessible.name: qsTr("Horizon")
        readonly property var keys: ["none", "forest", "mountains"]
        model: [qsTr("None"), qsTr("Forest"), qsTr("Mountains")]
        currentIndex: keys.indexOf(form.config.visualParameters.horizon)
        onActivated: index => form.settings.writeVisualParameter("horizon", keys[index])
    }

    QQC2.CheckBox {
        text: qsTr("Show stars")
        checked: form.config.visualParameters.stars === true
        onToggled: form.settings.writeVisualParameter("stars", checked)
    }

    QQC2.CheckBox {
        text: qsTr("Rare sky events")
        checked: form.config.visualParameters.skyEvents === true
        onToggled: form.settings.writeVisualParameter("skyEvents", checked)
    }
}
