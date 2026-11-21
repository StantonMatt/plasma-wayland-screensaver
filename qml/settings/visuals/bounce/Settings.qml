// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import "../.."

Kirigami.FormLayout {
    id: form
    required property var settings
    required property var config
    readonly property var sharedLabels: ({speed: qsTr("Speed"), density: "", scale: qsTr("Ball size"), trail: qsTr("Light trails")})

    QQC2.SpinBox {
        objectName: "bounceCount"
        Kirigami.FormData.label: qsTr("Balls:")
        Accessible.name: qsTr("Balls")
        from: 1; to: 60
        value: form.config.visualParameters.count
        onValueModified: form.settings.writeVisualParameter("count", value)
    }

    ValueSlider {
        Kirigami.FormData.label: qsTr("Gravity:")
        accessibleName: qsTr("Gravity")
        from: -100; to: 100; stepSize: 5
        valueText: shownValue < -2 ? qsTr("Up %1").arg(-shownValue)
                 : shownValue > 2 ? qsTr("Down %1").arg(shownValue) : qsTr("Zero-G")
        widestText: qsTr("Down %1").arg(100)
        value: form.config.visualParameters.gravity
        onMoved: value => form.settings.writeVisualParameter("gravity", value)
    }

    ValueSlider {
        Kirigami.FormData.label: qsTr("Bounciness:")
        accessibleName: qsTr("Bounciness")
        from: 50; to: 100; stepSize: 1
        value: form.config.visualParameters.elasticity
        onMoved: value => form.settings.writeVisualParameter("elasticity", value)
    }

    QQC2.ComboBox {
        objectName: "bounceMaterial"
        Kirigami.FormData.label: qsTr("Material:")
        Accessible.name: qsTr("Material")
        readonly property var keys: ["mixed", "rubber", "glass", "metal"]
        model: [qsTr("Mixed"), qsTr("Rubber"), qsTr("Glass"), qsTr("Metal")]
        currentIndex: keys.indexOf(form.config.visualParameters.material)
        onActivated: index => form.settings.writeVisualParameter("material", keys[index])
    }

    QQC2.CheckBox {
        text: qsTr("Balls collide with each other")
        checked: form.config.visualParameters.collisions === true
        onToggled: form.settings.writeVisualParameter("collisions", checked)
    }

    QQC2.CheckBox {
        text: qsTr("World events")
        checked: form.config.visualParameters.worldEvents === true
        onToggled: form.settings.writeVisualParameter("worldEvents", checked)
    }
}
