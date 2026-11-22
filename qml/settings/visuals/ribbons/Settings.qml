// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import "../.."

Kirigami.FormLayout {
    id: form
    required property var settings
    required property var config
    readonly property var sharedLabels: ({speed: qsTr("Speed"), density: qsTr("Ribbons"), scale: qsTr("Thickness"), trail: qsTr("Glow")})

    ValueSlider {
        Kirigami.FormData.label: qsTr("Turbulence:")
        accessibleName: qsTr("Turbulence")
        from: 0; to: 100; stepSize: 5
        valueText: shownValue <= 15 ? qsTr("Calm") : shownValue >= 85 ? qsTr("Stormy") : qsTr("%1%").arg(shownValue)
        widestText: qsTr("Stormy")
        value: form.config.visualParameters.turbulence
        onMoved: value => form.settings.writeVisualParameter("turbulence", value)
    }

    ValueSlider {
        Kirigami.FormData.label: qsTr("Twist:")
        accessibleName: qsTr("Twist")
        from: 0; to: 100; stepSize: 5
        valueText: shownValue === 0 ? qsTr("Flat") : qsTr("%1%").arg(shownValue)
        value: form.config.visualParameters.twist
        onMoved: value => form.settings.writeVisualParameter("twist", value)
    }

    ValueSlider {
        Kirigami.FormData.label: qsTr("Length:")
        accessibleName: qsTr("Length")
        from: 0; to: 100; stepSize: 5
        value: form.config.visualParameters.length
        onMoved: value => form.settings.writeVisualParameter("length", value)
    }

    QQC2.CheckBox {
        text: qsTr("Ribbons dance together")
        checked: form.config.visualParameters.dance === true
        onToggled: form.settings.writeVisualParameter("dance", checked)
    }

    QQC2.CheckBox {
        text: qsTr("World events")
        checked: form.config.visualParameters.worldEvents === true
        onToggled: form.settings.writeVisualParameter("worldEvents", checked)
    }
}
