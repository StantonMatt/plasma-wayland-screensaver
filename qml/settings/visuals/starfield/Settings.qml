// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import "../.."

Kirigami.FormLayout {
    id: form
    required property var settings
    required property var config
    readonly property var sharedLabels: ({speed: qsTr("Flight speed"), density: qsTr("Stars"), scale: qsTr("Star size"), trail: qsTr("Trails")})

    QQC2.ComboBox {
        Kirigami.FormData.label: qsTr("Flight:")
        Accessible.name: qsTr("Flight")
        readonly property var keys: ["voyage", "hyperspace", "cruise"]
        model: [qsTr("Voyage"), qsTr("Endless hyperspace"), qsTr("Calm cruise")]
        currentIndex: keys.indexOf(form.config.visualParameters.flight)
        onActivated: index => form.settings.writeVisualParameter("flight", keys[index])
    }

    ValueSlider {
        Kirigami.FormData.label: qsTr("Steering:")
        accessibleName: qsTr("Steering")
        from: 0; to: 100; stepSize: 5
        valueText: shownValue === 0 ? qsTr("Straight") : qsTr("%1%").arg(shownValue)
        widestText: qsTr("Straight")
        value: form.config.visualParameters.steering
        onMoved: value => form.settings.writeVisualParameter("steering", value)
    }

    ValueSlider {
        Kirigami.FormData.label: qsTr("Nebulae:")
        accessibleName: qsTr("Nebulae")
        from: 0; to: 100; stepSize: 5
        valueText: shownValue === 0 ? qsTr("None") : qsTr("%1%").arg(shownValue)
        value: form.config.visualParameters.nebulae
        onMoved: value => form.settings.writeVisualParameter("nebulae", value)
    }

    QQC2.CheckBox {
        text: qsTr("Celestial events")
        checked: form.config.visualParameters.events === true
        onToggled: form.settings.writeVisualParameter("events", checked)
    }
}
