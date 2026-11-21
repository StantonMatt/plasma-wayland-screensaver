// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import "../.."

Kirigami.FormLayout {
    id: form
    required property var settings
    required property var config
    readonly property var sharedLabels: ({speed: qsTr("Fall speed"), density: qsTr("Rainfall"), scale: qsTr("Glyph size"), trail: qsTr("Afterglow")})

    QQC2.ComboBox {
        Kirigami.FormData.label: qsTr("Weather:")
        Accessible.name: qsTr("Weather")
        readonly property var keys: ["steady", "changing", "stormy"]
        model: [qsTr("Steady"), qsTr("Changing"), qsTr("Stormy")]
        currentIndex: keys.indexOf(form.config.visualParameters.weather)
        onActivated: index => form.settings.writeVisualParameter("weather", keys[index])
    }

    ValueSlider {
        Kirigami.FormData.label: qsTr("Wind:")
        accessibleName: qsTr("Wind")
        from: 0; to: 100; stepSize: 5
        valueText: shownValue === 0 ? qsTr("Still") : qsTr("%1%").arg(shownValue)
        value: form.config.visualParameters.wind
        onMoved: value => form.settings.writeVisualParameter("wind", value)
    }

    QQC2.CheckBox {
        objectName: "rainLightning"
        text: qsTr("Lightning during storms")
        enabled: form.config.visualParameters.weather !== "steady"
        checked: form.config.visualParameters.lightning === true
        onToggled: form.settings.writeVisualParameter("lightning", checked)
    }

    QQC2.ComboBox {
        Kirigami.FormData.label: qsTr("Glyphs:")
        Accessible.name: qsTr("Glyphs")
        readonly property var keys: ["weather", "numbers", "hex", "katakana", "letters", "lines-blocks", "braille"]
        model: [qsTr("Changing with the weather"), qsTr("Numbers"), qsTr("Hex code"), qsTr("Katakana"), qsTr("Letters"), qsTr("Lines & blocks"), qsTr("Braille dots")]
        currentIndex: keys.indexOf(form.config.visualParameters.glyphs)
        onActivated: index => form.settings.writeVisualParameter("glyphs", keys[index])
    }

    ValueSlider {
        Kirigami.FormData.label: qsTr("Depth:")
        accessibleName: qsTr("Depth")
        from: 0; to: 100; stepSize: 5
        valueText: shownValue === 0 ? qsTr("Flat") : qsTr("%1%").arg(shownValue)
        value: form.config.visualParameters.depth
        onMoved: value => form.settings.writeVisualParameter("depth", value)
    }
}
