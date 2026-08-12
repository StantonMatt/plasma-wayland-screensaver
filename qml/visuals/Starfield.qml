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
        const trails = context ? context.trailAmount / 100 : 0.35
        const palette = Utils.colors(context ? context.animationPalette : "ocean")
        const count = Math.round(45 + density * 3.1)
        const cx = width / 2
        const cy = height / 2
        for (let i = 0; i < count; ++i) {
            const angle = Utils.random(i * 3 + 1, seed) * Math.PI * 2
            const lane = 0.05 + Utils.random(i * 3 + 2, seed) * 0.95
            const travel = Utils.positiveModulo(phase * speed + Utils.random(i * 3 + 3, seed), 1)
            const distance = Math.pow(travel, 1.65) * Math.hypot(width, height) * lane
            const x = cx + Math.cos(angle) * distance
            const y = cy + Math.sin(angle) * distance
            const size = Math.max(0.8, travel * 5.2 * scale)
            const tail = 2 + travel * 34 * trails * speed
            painter.strokeStyle = palette[i % palette.length]
            painter.lineWidth = size
            painter.globalAlpha = 0.20 + travel * 0.80
            painter.beginPath()
            painter.moveTo(x - Math.cos(angle) * tail, y - Math.sin(angle) * tail)
            painter.lineTo(x, y)
            painter.stroke()
        }
        painter.globalAlpha = 0.5
        const core = painter.createRadialGradient(cx, cy, 0, cx, cy, Math.min(width, height) * 0.13)
        core.addColorStop(0, palette[0])
        core.addColorStop(1, "#00000000")
        painter.fillStyle = core
        painter.fillRect(0, 0, width, height)
    }
    FrameClock {
        presentationClock: canvas.context ? canvas.context.presentationClock : null
        running: !canvas.reducedMotion
        onTick: function(deltaSeconds) {
            canvas.phase += deltaSeconds * 0.12
            canvas.requestPaint()
        }
    }
}
