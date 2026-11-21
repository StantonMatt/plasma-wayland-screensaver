// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import "../.."

Kirigami.FormLayout {
    id: form
    required property var settings
    required property var config
    readonly property var sharedLabels: ({speed: qsTr("Speed"), density: qsTr("Stars"), scale: qsTr("Star size"), trail: qsTr("Lines")})

    ValueSlider {
        Kirigami.FormData.label: qsTr("Gravity:")
        accessibleName: qsTr("Gravity")
        from: 0; to: 100; stepSize: 5
        value: form.config.visualParameters.gravity
        onMoved: value => form.settings.writeVisualParameter("gravity", value)
    }

    ValueSlider {
        Kirigami.FormData.label: qsTr("Twinkle:")
        accessibleName: qsTr("Twinkle")
        from: 0; to: 100; stepSize: 5
        value: form.config.visualParameters.twinkle
        onMoved: value => form.settings.writeVisualParameter("twinkle", value)
    }

    ValueSlider {
        Kirigami.FormData.label: qsTr("Milky Way:")
        accessibleName: qsTr("Milky Way")
        from: 0; to: 100; stepSize: 5
        value: form.config.visualParameters.milkyWay
        onMoved: value => form.settings.writeVisualParameter("milkyWay", value)
    }

    QQC2.CheckBox {
        text: qsTr("Stars are born, age and explode")
        checked: form.config.visualParameters.lifeCycle === true
        onToggled: form.settings.writeVisualParameter("lifeCycle", checked)
    }

    QQC2.CheckBox {
        text: qsTr("Sky events")
        checked: form.config.visualParameters.skyEvents === true
        onToggled: form.settings.writeVisualParameter("skyEvents", checked)
    }
}
