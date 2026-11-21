// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import "../.."

Kirigami.FormLayout {
    id: form
    required property var settings
    required property var config
    readonly property var sharedLabels: ({speed: qsTr("Speed"), density: qsTr("Orbs"), scale: qsTr("Size"), trail: qsTr("Glow")})

    QQC2.ComboBox {
        Kirigami.FormData.label: qsTr("Look:")
        Accessible.name: qsTr("Look")
        readonly property var keys: ["glow", "glass", "bubbles"]
        model: [qsTr("Glow"), qsTr("Glass"), qsTr("Bubbles")]
        currentIndex: keys.indexOf(form.config.visualParameters.look)
        onActivated: index => form.settings.writeVisualParameter("look", keys[index])
    }

    QQC2.ComboBox {
        Kirigami.FormData.label: qsTr("Currents:")
        Accessible.name: qsTr("Currents")
        readonly property var keys: ["drift", "rise", "swirl"]
        model: [qsTr("Drift"), qsTr("Rise"), qsTr("Swirl")]
        currentIndex: keys.indexOf(form.config.visualParameters.flow)
        onActivated: index => form.settings.writeVisualParameter("flow", keys[index])
    }

    ValueSlider {
        Kirigami.FormData.label: qsTr("Merging:")
        accessibleName: qsTr("Merging")
        from: 0; to: 100; stepSize: 5
        value: form.config.visualParameters.merging
        onMoved: value => form.settings.writeVisualParameter("merging", value)
    }

    ValueSlider {
        Kirigami.FormData.label: qsTr("Depth:")
        accessibleName: qsTr("Depth")
        from: 0; to: 100; stepSize: 5
        value: form.config.visualParameters.depth
        onMoved: value => form.settings.writeVisualParameter("depth", value)
    }

    QQC2.CheckBox {
        text: qsTr("World events")
        checked: form.config.visualParameters.worldEvents === true
        onToggled: form.settings.writeVisualParameter("worldEvents", checked)
    }
}
