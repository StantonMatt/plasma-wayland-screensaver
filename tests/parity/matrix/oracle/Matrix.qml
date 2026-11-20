// SPDX-License-Identifier: GPL-3.0-or-later
pragma ComponentBehavior: Bound
import QtQuick
import "VisualUtils.js" as Utils

Item {
    id: root
    clip: true

    property var context
    property int frameRate: 30
    property bool reducedMotion: false
    property int seed: 1
    property double animationEpochMs: Date.now()
    property real phase: 0

    readonly property string glyphs: "0123456789ABCDEF<>[]{}アイウエオカキクケコサシスセソ"
    readonly property real density: context ? context.animationDensity : 50
    readonly property real glyphScale: context ? context.animationScale / 100 : 1
    readonly property real glow: context ? context.trailAmount / 100 : 0.35
    readonly property var visualPalette: Utils.colors(context ? context.animationPalette : "forest")
    readonly property int glyphSize: Math.max(11, Math.round((30 - density * 0.12) * glyphScale))
    readonly property real columnWidth: glyphSize * 1.18
    readonly property int columnCount: Math.ceil(width / Math.max(1, columnWidth)) + 1

    function glyphStream(column, length) {
        let result = ""
        for (let row = 0; row < length; ++row) {
            const glyphIndex = Math.floor(Utils.random(column * 97 + row * 29, seed)
                                          * glyphs.length)
            if (row > 0)
                result += "\n"
            result += glyphs.charAt(glyphIndex)
        }
        return result
    }

    Repeater {
        model: root.columnCount

        Item {
            id: rainColumn
            required property int index
            readonly property int streamLength: 6 + Math.floor(
                                                    Utils.random(index * 7 + 1, root.seed)
                                                    * (8 + root.density * 0.16))
            readonly property real fallRate: 0.32 + Utils.random(index * 7 + 2, root.seed) * 0.82
            readonly property real startOffset: Utils.random(index * 7 + 3, root.seed)
            x: index * root.columnWidth
            y: Utils.positiveModulo(root.phase * fallRate * root.height
                                    + startOffset * (root.height + stream.contentHeight),
                                    root.height + stream.contentHeight) - stream.contentHeight
            width: root.columnWidth
            height: stream.contentHeight

            Text {
                id: stream
                width: parent.width
                text: root.glyphStream(rainColumn.index, rainColumn.streamLength)
                color: root.visualPalette[rainColumn.index % root.visualPalette.length]
                opacity: 0.28 + root.glow * 0.52
                horizontalAlignment: Text.AlignHCenter
                font.family: "monospace"
                font.pixelSize: root.glyphSize
                lineHeightMode: Text.FixedHeight
                lineHeight: root.glyphSize * 0.95
                renderType: Text.QtRendering
                layer.enabled: true
                layer.smooth: true
            }

            Text {
                x: 0
                y: Math.max(0, stream.contentHeight - root.glyphSize * 1.05)
                width: parent.width
                text: stream.text.charAt(stream.text.length - 1)
                color: "#ffffff"
                opacity: 0.72 + root.glow * 0.28
                horizontalAlignment: Text.AlignHCenter
                font.family: "monospace"
                font.pixelSize: root.glyphSize
                renderType: Text.QtRendering
            }
        }
    }

    FrameClock {
        presentationClock: root.context ? root.context.presentationClock : null
        running: !root.reducedMotion
        onTick: function(deltaSeconds) {
            const speed = root.context ? root.context.animationSpeed / 100 : 1
            root.phase = (Date.now() - root.animationEpochMs) * 0.00018 * speed
        }
    }
}
