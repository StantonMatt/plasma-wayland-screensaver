// SPDX-License-Identifier: GPL-3.0-or-later
// Slider with its current value shown to the right. Writes on every move;
// the window batches the disk write.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

RowLayout {
    id: root

    property alias from: slider.from
    property alias to: slider.to
    property alias stepSize: slider.stepSize
    property int value
    // The value under the handle, shown while dragging.
    readonly property int shownValue: Math.round(slider.value)
    property string valueText: qsTr("%1%").arg(shownValue)
    // Reserve the width of the longest value so the slider never jumps.
    property string widestText: qsTr("%1%").arg(300)
    property string accessibleName
    signal moved(int value)

    spacing: Kirigami.Units.largeSpacing

    QQC2.Slider {
        id: slider
        Layout.fillWidth: true
        Layout.minimumWidth: Kirigami.Units.gridUnit * 8
        Layout.preferredWidth: Kirigami.Units.gridUnit * 15
        Layout.maximumWidth: Kirigami.Units.gridUnit * 18
        value: root.value
        snapMode: QQC2.Slider.SnapAlways
        onMoved: root.moved(Math.round(value))
        Accessible.name: root.accessibleName
    }

    QQC2.Label {
        // Same width in every row so all sliders end at the same x.
        Layout.preferredWidth: Math.max(widest.advanceWidth, Kirigami.Units.gridUnit * 3.5)
        horizontalAlignment: Text.AlignRight
        text: root.valueText
        font.features: { "tnum": 1 }
        TextMetrics { id: widest; text: root.widestText; font.features: { "tnum": 1 } }
    }
}
