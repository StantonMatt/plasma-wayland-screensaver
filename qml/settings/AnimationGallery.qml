// SPDX-License-Identifier: GPL-3.0-or-later
// Thumbnail grid for choosing the animation. Uses KCMUtils.GridDelegate, the
// same tile as Plasma's Wallpaper, Global Theme and Splash Screen pages.
// Thumbnails are static PNGs captured on Pure Black with luminance alpha, laid
// over the chosen background so the tiles follow the Background setting.
pragma ComponentBehavior: Bound
import QtQuick
import org.kde.kirigami as Kirigami
import org.kde.kcmutils as KCMUtils
import "Catalog.js" as Catalog

GridView {
    id: view

    property string currentValue
    property string backgroundStyle
    property bool showClock: true
    signal activated(string value)
    signal previewRequested(string value)

    // Columns snap to 6, 4, 3 or 2 so the 11 tiles fill rows evenly.
    readonly property int minimumCellWidth: Kirigami.Units.gridUnit * 7
    readonly property int columns: {
        for (const c of [6, 4, 3, 2])
            if (width / c >= minimumCellWidth) return c
        return 1
    }
    // Properties GridDelegate reads from its view (normally KCMUtils.GridView).
    property int implicitCellWidth: cellWidth - Kirigami.Units.gridUnit
    property int implicitCellHeight: Math.round(implicitCellWidth * 9 / 16) + Kirigami.Units.gridUnit * 3
    property bool neutralHighlight: false

    cellWidth: Math.floor(width / columns)
    cellHeight: implicitCellHeight
    implicitHeight: Math.ceil(count / columns) * cellHeight
    interactive: false
    keyNavigationEnabled: true
    keyNavigationWraps: false
    highlightMoveDuration: 0
    activeFocusOnTab: true
    Keys.onSpacePressed: activated(Catalog.animations[currentIndex].id)
    currentIndex: Math.max(0, Catalog.animationIndex(currentValue))
    // Arrow keys move the selection like a radio group.
    onCurrentIndexChanged: if (activeFocus && Catalog.animations[currentIndex].id !== currentValue)
        activated(Catalog.animations[currentIndex].id)

    Accessible.role: Accessible.List
    Accessible.name: qsTr("Animation")

    model: Catalog.animations

    delegate: KCMUtils.GridDelegate {
        id: tile
        required property var modelData
        required property int index

        text: modelData.name
        thumbnailAvailable: true
        checkable: true
        autoExclusive: true
        checked: modelData.id === view.currentValue
        activeFocusOnTab: false
        Accessible.role: Accessible.RadioButton
        Accessible.checkable: true
        Accessible.checked: GridView.isCurrentItem
        onClicked: {
            view.forceActiveFocus()
            view.activated(modelData.id)
        }
        // Handle Space before GridDelegate's inherited handler opens its
        // action menu. Enter/Menu still expose the preview action.
        Keys.forwardTo: [selectionKeys]
        Item {
            id: selectionKeys
            Keys.onSpacePressed: tile.clicked()
        }
        // Plasma's Splash Screen page uses the same hover button. Shown on
        // hover and on the selected tile; selects the tile, then previews.
        actions: [
            Kirigami.Action {
                icon.name: "media-playback-start"
                text: qsTr("Preview %1").arg(tile.modelData.name)
                onTriggered: view.previewRequested(tile.modelData.id)
            }
        ]

        thumbnail: Rectangle {
            id: backdrop
            anchors.fill: parent
            radius: Math.round(Kirigami.Units.cornerRadius / 2)
            readonly property var bg: Catalog.background(view.backgroundStyle)
            gradient: Gradient {
                GradientStop { position: 0; color: backdrop.bg.top }
                GradientStop { position: 1; color: backdrop.bg.bottom }
            }
            clip: true

            Image {
                anchors.fill: parent
                visible: tile.modelData.id !== "none"
                source: tile.modelData.id === "none" ? ""
                        : Qt.resolvedUrl("../images/thumbnails/" + tile.modelData.id + ".png")
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                smooth: true
                mipmap: true
            }

            // "None" shows only the background and, when enabled, the clock,
            // in the screensaver's clock font.
            Text {
                anchors.centerIn: parent
                visible: tile.modelData.id === "none" && view.showClock
                text: "21:45"
                color: "#f6f8ff"
                opacity: 0.85
                font.family: "Noto Sans"
                font.weight: Font.Light
                font.pixelSize: parent.height * 0.26
                Accessible.ignored: true
            }
        }
    }
}
