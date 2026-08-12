// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import "VisualUtils.js" as Utils

Canvas {
    id: canvas
    property var context
    property int frameRate: 30
    property bool reducedMotion: false
    property int seed: 1
    property double animationEpochMs: Date.now()
    property real phase: 0
    readonly property string glyphs: "0123456789ABCDEF<>[]{}アイウエオカキクケコサシスセソ"

    renderTarget: Canvas.FramebufferObject
    renderStrategy: Canvas.Cooperative
    onWidthChanged: requestPaint()
    onHeightChanged: requestPaint()
    onPaint: {
        const painter = getContext("2d")
        painter.reset()
        painter.clearRect(0, 0, width, height)
        const density = context ? context.animationDensity : 50
        const scale = context ? context.animationScale / 100 : 1
        const speed = context ? context.animationSpeed / 100 : 1
        const glow = context ? context.trailAmount / 100 : 0.35
        const palette = Utils.colors(context ? context.animationPalette : "forest")
        const fontSize = Math.max(11, Math.round((29 - density * 0.15) * scale))
        const columns = Math.ceil(width / fontSize)
        painter.font = fontSize + "px monospace"
        painter.textAlign = "center"
        for (let column = 0; column < columns; ++column) {
            const length = 5 + Math.floor(Utils.random(column * 7 + 1, seed) * 16 * (density / 50))
            const rate = (0.35 + Utils.random(column * 7 + 2, seed) * 0.85) * speed
            const head = Utils.positiveModulo(phase * rate * height
                                               + Utils.random(column * 7 + 3, seed) * height,
                                               height + length * fontSize)
            for (let row = 0; row < length; ++row) {
                const y = head - row * fontSize
                if (y < -fontSize || y > height + fontSize)
                    continue
                const characterIndex = Math.floor(Utils.random(column * 31 + row * 13
                                                                 + Math.floor(phase * 4), seed)
                                                  * glyphs.length)
                painter.globalAlpha = Math.max(0.04, (1 - row / length) * (0.35 + glow * 0.65))
                painter.fillStyle = row === 0 ? "#ffffff" : palette[column % palette.length]
                painter.fillText(glyphs.charAt(characterIndex), column * fontSize + fontSize / 2, y)
            }
        }
    }
    Timer {
        interval: Math.round(1000 / Math.max(1, canvas.frameRate))
        repeat: true
        running: !canvas.reducedMotion
        onTriggered: {
            canvas.phase = (Date.now() - canvas.animationEpochMs) * 0.00018
            canvas.requestPaint()
        }
    }
}
