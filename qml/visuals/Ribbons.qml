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

    renderTarget: Canvas.FramebufferObject
    renderStrategy: Canvas.Threaded
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
        const palette = Utils.colors(context ? context.animationPalette : "pastel")
        const count = Math.round(3 + density / 12)
        painter.globalCompositeOperation = "lighter"
        for (let i = 0; i < count; ++i) {
            const offset = i / Math.max(1, count - 1)
            const y = height * (0.12 + offset * 0.76)
            const wave = phase * speed + i * 0.71 + seed * 0.01
            painter.strokeStyle = palette[i % palette.length]
            painter.globalAlpha = 0.16 + glow * 0.42
            painter.lineWidth = (2 + (i % 3) * 2.5) * scale
            painter.beginPath()
            painter.moveTo(-width * 0.05, y + Math.sin(wave) * height * 0.08)
            painter.bezierCurveTo(width * 0.22, y + Math.cos(wave * 1.2) * height * 0.28,
                                  width * 0.38, y + Math.sin(wave * 0.83 + 2) * height * 0.25,
                                  width * 0.55, y + Math.cos(wave + 1) * height * 0.12)
            painter.bezierCurveTo(width * 0.72, y + Math.sin(wave * 1.31) * height * 0.27,
                                  width * 0.88, y + Math.cos(wave * 0.77 + 3) * height * 0.24,
                                  width * 1.05, y + Math.sin(wave + 2) * height * 0.08)
            painter.stroke()
            if (glow > 0.25) {
                painter.globalAlpha = glow * 0.08
                painter.lineWidth *= 4
                painter.stroke()
            }
        }
    }
    FrameClock {
        presentationClock: canvas.context ? canvas.context.presentationClock : null
        running: !canvas.reducedMotion
        onTick: function(deltaSeconds) {
            canvas.phase = (Date.now() - canvas.animationEpochMs) * 0.00055
            canvas.requestPaint()
        }
    }
}
