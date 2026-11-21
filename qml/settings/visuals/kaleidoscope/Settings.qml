// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import "../.."

Kirigami.FormLayout {
    id: form
    required property var settings
    required property var config
    readonly property var sharedLabels: ({speed: qsTr("Speed"), density: qsTr("Pieces"), scale: qsTr("Zoom"), trail: qsTr("Glow")})

    QQC2.ComboBox {
        Kirigami.FormData.label: qsTr("Mirrors:")
        Accessible.name: qsTr("Mirrors")
        readonly property var keys: ["mandala", "lattice"]
        model: [qsTr("Mandala"), qsTr("Lattice")]
        currentIndex: keys.indexOf(form.config.visualParameters.mirrors)
        onActivated: index => form.settings.writeVisualParameter("mirrors", keys[index])
    }

    ValueSlider {
        Kirigami.FormData.label: qsTr("Points:")
        accessibleName: qsTr("Points")
        from: 3; to: 16; stepSize: 1
        value: form.config.visualParameters.points
        onMoved: value => form.settings.writeVisualParameter("points", value)
    }

    QQC2.ComboBox {
        Kirigami.FormData.label: qsTr("Glass:")
        Accessible.name: qsTr("Glass")
        readonly property var keys: ["mixed", "beads", "shards", "rods-wire"]
        model: [qsTr("Mixed"), qsTr("Beads"), qsTr("Shards"), qsTr("Rods wire")]
        currentIndex: keys.indexOf(form.config.visualParameters.glass)
        onActivated: index => form.settings.writeVisualParameter("glass", keys[index])
    }

    ValueSlider {
        Kirigami.FormData.label: qsTr("Turning:")
        accessibleName: qsTr("Turning")
        from: 0; to: 100; stepSize: 5
        value: form.config.visualParameters.turning
        onMoved: value => form.settings.writeVisualParameter("turning", value)
    }

    QQC2.CheckBox {
        text: qsTr("Events")
        checked: form.config.visualParameters.events === true
        onToggled: form.settings.writeVisualParameter("events", checked)
    }
}
