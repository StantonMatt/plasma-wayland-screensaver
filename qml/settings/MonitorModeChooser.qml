// SPDX-License-Identifier: GPL-3.0-or-later
// Three illustrated choices for how the animation uses several monitors.
// Same frame/highlight treatment as the animation tiles so both pickers read
// as one family. One radio group: Left/Right change the selection.
pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtQuick.Shapes
import org.kde.kirigami as Kirigami

RowLayout {
    id: root

    property string currentValue
    signal activated(string value)

    readonly property var modes: [
        { value: "independent", name: qsTr("Independent") },
        { value: "synchronized", name: qsTr("Mirrored") },
        { value: "seamless", name: qsTr("Spanning") }
    ]

    spacing: Kirigami.Units.largeSpacing

    function select(index) {
        const i = (index + modes.length) % modes.length
        // Focus first: the old card stops being a tab stop once deselected.
        cards.itemAt(i).forceActiveFocus(Qt.TabFocusReason)
        root.activated(modes[i].value)
    }

    // A short trail with a dot at its head on each screen. Independent:
    // different trails; Mirrored: the same trail; Spanning: one trail that
    // runs across all three screens. Coordinates are 0..1 per screen.
    function trail(mode, screen) {
        const curve = (x0, y0, x1, y1, bend) => {
            const pts = []
            for (let k = 0; k <= 16; ++k) {
                const t = k / 16
                pts.push({ x: x0 + (x1 - x0) * t,
                           y: y0 + (y1 - y0) * t + Math.sin(t * Math.PI) * bend })
            }
            return pts
        }
        if (mode === "seamless") {
            // Global x spans three screens (each 1 wide, gaps ignored).
            const pts = []
            for (let k = 0; k <= 60; ++k) {
                const g = 0.25 + 2.5 * k / 60
                pts.push({ x: g - screen, y: 0.5 + Math.sin(g * Math.PI * 0.9 + 0.3) * 0.26 })
            }
            const inside = pts.filter(p => p.x >= 0 && p.x <= 1)
            return { points: inside, head: screen === 2 ? inside[inside.length - 1] : null }
        }
        const shapes = mode === "synchronized"
            ? [[0.18, 0.72, 0.78, 0.32, -0.18], [0.18, 0.72, 0.78, 0.32, -0.18], [0.18, 0.72, 0.78, 0.32, -0.18]]
            : [[0.15, 0.30, 0.80, 0.70, 0.22], [0.82, 0.72, 0.25, 0.38, -0.12], [0.30, 0.78, 0.62, 0.22, 0.25]]
        const pts = curve(...shapes[screen])
        return { points: pts, head: pts[pts.length - 1] }
    }

    Repeater {
        id: cards
        model: root.modes

        delegate: QQC2.AbstractButton {
            id: card
            required property var modelData
            required property int index

            checkable: true
            autoExclusive: true
            checked: modelData.value === root.currentValue
            hoverEnabled: true
            activeFocusOnTab: checked
            text: modelData.name
            Accessible.role: Accessible.RadioButton
            Accessible.checkable: true
            Accessible.checked: checked
            Accessible.name: text

            onClicked: root.activated(modelData.value)
            Keys.onLeftPressed: root.select(index - (mirrored ? -1 : 1))
            Keys.onRightPressed: root.select(index + (mirrored ? -1 : 1))

            implicitWidth: Kirigami.Units.gridUnit * 8
            implicitHeight: frame.height + Kirigami.Units.smallSpacing + label.implicitHeight

            contentItem: Item {
                Kirigami.ShadowedRectangle {
                    id: frame
                    width: parent.width
                    height: Kirigami.Units.gridUnit * 3.6
                    radius: Kirigami.Units.cornerRadius
                    Kirigami.Theme.inherit: false
                    Kirigami.Theme.colorSet: Kirigami.Theme.View
                    shadow.yOffset: 1
                    shadow.size: 6
                    shadow.color: Qt.rgba(0, 0, 0, 0.25)
                    color: card.checked ? Kirigami.Theme.highlightColor
                         : card.hovered ? Qt.alpha(Kirigami.Theme.highlightColor, 0.5)
                         : Kirigami.Theme.backgroundColor

                    Rectangle {
                        anchors.fill: parent
                        anchors.margins: Kirigami.Units.smallSpacing
                        radius: Math.round(Kirigami.Units.cornerRadius / 2)
                        color: Kirigami.Theme.backgroundColor

                        Row {
                            id: screens
                            anchors.centerIn: parent
                            spacing: 3
                            readonly property real screenWidth: Math.floor((parent.width - 2 * Kirigami.Units.smallSpacing - 2 * spacing) / 3)
                            readonly property real screenHeight: Math.round(screenWidth * 0.62)

                            Repeater {
                                model: 3
                                delegate: Rectangle {
                                    id: screen
                                    required property int index
                                    width: screens.screenWidth
                                    height: screens.screenHeight
                                    radius: 2
                                    // Screens read as screens (dark) in any color scheme.
                                    color: "#101215"
                                    border.width: 1
                                    border.color: Qt.alpha(Kirigami.Theme.textColor, 0.35)

                                    readonly property var trail: root.trail(card.modelData.value, index)
                                    readonly property real inset: 3

                                    Shape {
                                        anchors.fill: parent
                                        preferredRendererType: Shape.CurveRenderer
                                        ShapePath {
                                            strokeColor: Qt.alpha(Kirigami.Theme.highlightColor, 0.85)
                                            strokeWidth: 2
                                            fillColor: "transparent"
                                            capStyle: ShapePath.RoundCap
                                            joinStyle: ShapePath.RoundJoin
                                            PathPolyline {
                                                path: screen.trail.points.map(p => Qt.point(
                                                    screen.inset + p.x * (screen.width - 2 * screen.inset),
                                                    screen.inset + p.y * (screen.height - 2 * screen.inset)))
                                            }
                                        }
                                    }
                                    Rectangle {
                                        visible: screen.trail.head !== null
                                        width: 6; height: 6; radius: 3
                                        color: Kirigami.Theme.highlightColor
                                        x: screen.trail.head ? screen.inset + screen.trail.head.x * (screen.width - 2 * screen.inset) - 3 : 0
                                        y: screen.trail.head ? screen.inset + screen.trail.head.y * (screen.height - 2 * screen.inset) - 3 : 0
                                    }
                                }
                            }
                        }
                    }
                }

                QQC2.Label {
                    id: label
                    anchors.top: frame.bottom
                    anchors.topMargin: Kirigami.Units.smallSpacing
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    text: card.text
                    font.bold: card.checked
                    elide: Text.ElideRight
                }

                Rectangle {
                    anchors.top: label.bottom
                    anchors.horizontalCenter: label.horizontalCenter
                    width: label.paintedWidth
                    height: 1
                    color: Kirigami.Theme.highlightColor
                    visible: card.visualFocus
                }
            }
        }
    }
}
