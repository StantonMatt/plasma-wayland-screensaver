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
        const palette = Utils.colors(context ? context.animationPalette : "ocean")
        const count = Math.round(16 + density * 0.72)
        const points = []
        for (let i = 0; i < count; ++i) {
            const baseX = Utils.random(i * 5 + 1, seed) * width
            const baseY = Utils.random(i * 5 + 2, seed) * height
            points.push({
                x: Utils.positiveModulo(baseX + Math.sin(phase * speed + i) * width * 0.025, width),
                y: Utils.positiveModulo(baseY + Math.cos(phase * speed * 0.73 + i) * height * 0.035, height)
            })
        }
        const linkDistance = Math.min(width, height) * (0.09 + density * 0.0008)
        painter.lineWidth = Math.max(0.5, scale)
        for (let i = 0; i < points.length; ++i) {
            for (let j = i + 1; j < points.length; ++j) {
                const dx = points[i].x - points[j].x
                const dy = points[i].y - points[j].y
                const distance = Math.sqrt(dx * dx + dy * dy)
                if (distance < linkDistance) {
                    painter.globalAlpha = (1 - distance / linkDistance) * (0.12 + glow * 0.42)
                    painter.strokeStyle = palette[(i + j) % palette.length]
                    painter.beginPath()
                    painter.moveTo(points[i].x, points[i].y)
                    painter.lineTo(points[j].x, points[j].y)
                    painter.stroke()
                }
            }
        }
        for (let i = 0; i < points.length; ++i) {
            painter.globalAlpha = 0.65 + Math.sin(phase * 2 + i) * 0.25
            painter.fillStyle = palette[i % palette.length]
            painter.beginPath()
            painter.arc(points[i].x, points[i].y, (1.5 + i % 3) * scale, 0, Math.PI * 2)
            painter.fill()
        }
    }
    FrameClock {
        presentationClock: canvas.context ? canvas.context.presentationClock : null
        running: !canvas.reducedMotion
        onTick: function(deltaSeconds) {
            canvas.phase = (Date.now() - canvas.animationEpochMs) * 0.00035
            canvas.requestPaint()
        }
    }
}
