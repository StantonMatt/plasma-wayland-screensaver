// SPDX-License-Identifier: GPL-3.0-or-later
// A row of round color swatches with the selected name after it. Entries with
// `colors` draw as six wedges (palettes); entries with `top`/`bottom` draw as
// a vertical gradient (backgrounds). Behaves as one radio group: Tab enters
// the group at the selected swatch, Left/Right change the selection.
pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtQuick.Shapes
import org.kde.kirigami as Kirigami

RowLayout {
    id: root

    property var model: []
    property string currentValue
    property string accessibleName
    signal activated(string value)

    readonly property int currentIndex: {
        for (let i = 0; i < model.length; ++i)
            if (model[i].value === currentValue) return i
        return -1
    }

    spacing: Kirigami.Units.smallSpacing

    component Wedge: ShapePath {
        id: wedge
        required property int slice
        required property real r
        required property color color
        strokeWidth: -1
        fillColor: color
        startX: r; startY: r
        PathAngleArc {
            moveToStart: false
            centerX: wedge.r; centerY: wedge.r
            radiusX: wedge.r; radiusY: wedge.r
            startAngle: -90 + wedge.slice * 60
            sweepAngle: 60
        }
        PathLine { x: wedge.r; y: wedge.r }
    }

    function select(index) {
        const i = (index + model.length) % model.length
        // Focus first: the old item stops being a tab stop once deselected.
        swatches.itemAt(i).forceActiveFocus(Qt.TabFocusReason)
        root.activated(model[i].value)
    }

    Repeater {
        id: swatches
        model: root.model

        delegate: QQC2.AbstractButton {
            id: swatch
            required property var modelData
            required property int index

            readonly property int size: Math.round(Kirigami.Units.gridUnit * 1.75)
            implicitWidth: size
            implicitHeight: size
            checkable: true
            autoExclusive: true
            checked: index === root.currentIndex
            hoverEnabled: true
            activeFocusOnTab: checked || (root.currentIndex < 0 && index === 0)
            text: modelData.name

            Accessible.role: Accessible.RadioButton
            Accessible.checkable: true
            Accessible.checked: checked
            Accessible.name: text
            Accessible.description: root.accessibleName

            QQC2.ToolTip.text: text
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay

            onClicked: root.activated(modelData.value)
            Keys.onLeftPressed: root.select(index - (mirrored ? -1 : 1))
            Keys.onRightPressed: root.select(index + (mirrored ? -1 : 1))

            background: Rectangle {
                radius: width / 2
                color: "transparent"
                border.width: 2
                border.color: swatch.checked || swatch.visualFocus ? Kirigami.Theme.highlightColor
                            : swatch.hovered ? Qt.alpha(Kirigami.Theme.highlightColor, 0.5)
                            : "transparent"
            }

            contentItem: Item {
                // Gradient swatch (backgrounds)
                Rectangle {
                    anchors.fill: parent
                    anchors.margins: 4
                    radius: width / 2
                    visible: !swatch.modelData.colors
                    gradient: Gradient {
                        GradientStop { position: 0; color: swatch.modelData.top || "black" }
                        GradientStop { position: 1; color: swatch.modelData.bottom || "black" }
                    }
                    border.width: 1
                    border.color: Qt.alpha(Kirigami.Theme.textColor, 0.25)
                }
                // Wedge swatch (palettes)
                Shape {
                    id: pie
                    anchors.fill: parent
                    anchors.margins: 4
                    visible: !!swatch.modelData.colors
                    preferredRendererType: Shape.CurveRenderer
                    readonly property real r: width / 2

                    readonly property var colors: swatch.modelData.colors || []

                    Wedge { slice: 0; r: pie.r; color: pie.colors[0] || "transparent" }
                    Wedge { slice: 1; r: pie.r; color: pie.colors[1] || "transparent" }
                    Wedge { slice: 2; r: pie.r; color: pie.colors[2] || "transparent" }
                    Wedge { slice: 3; r: pie.r; color: pie.colors[3] || "transparent" }
                    Wedge { slice: 4; r: pie.r; color: pie.colors[4] || "transparent" }
                    Wedge { slice: 5; r: pie.r; color: pie.colors[5] || "transparent" }
                }
                // Outline so pale palettes stay visible on light themes.
                Rectangle {
                    anchors.fill: pie
                    visible: pie.visible
                    radius: width / 2
                    color: "transparent"
                    border.width: 1
                    border.color: Qt.alpha(Kirigami.Theme.textColor, 0.25)
                }
            }
        }
    }

    QQC2.Label {
        Layout.leftMargin: Kirigami.Units.smallSpacing
        text: root.currentIndex >= 0 ? root.model[root.currentIndex].name : ""
        elide: Text.ElideRight
        Layout.fillWidth: true
        Accessible.ignored: true
    }
}
