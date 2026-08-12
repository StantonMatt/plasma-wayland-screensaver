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
        const palette = Utils.colors(context ? context.animationPalette : "ember")
        const count = Math.round(12 + density * 0.75)
        for (let i = 0; i < count; ++i) {
            const t = phase * speed * (0.45 + Utils.random(i * 9 + 1, seed) * 0.9)
            const baseX = Utils.random(i * 9 + 2, seed) * width
            const baseY = Utils.random(i * 9 + 3, seed) * height
            const x = Utils.positiveModulo(baseX + Math.sin(t + i * 2.13) * width * 0.10
                                           + t * width * 0.015, width)
            const y = Utils.positiveModulo(baseY + Math.cos(t * 0.73 + i * 1.47) * height * 0.12,
                                           height)
            const pulse = 0.45 + Math.sin(t * 2.2 + i) * 0.35
            const coreSize = (1.7 + Utils.random(i * 9 + 4, seed) * 3.2) * scale
            const haloSize = coreSize * (3.5 + glow * 8)
            const halo = painter.createRadialGradient(x, y, 0, x, y, haloSize)
            halo.addColorStop(0, palette[i % palette.length])
            halo.addColorStop(0.15, palette[i % palette.length])
            halo.addColorStop(1, "#00000000")
            painter.globalAlpha = pulse
            painter.fillStyle = halo
            painter.fillRect(x - haloSize, y - haloSize, haloSize * 2, haloSize * 2)
            painter.globalAlpha = 0.8
            painter.fillStyle = "#fffbd0"
            painter.beginPath()
            painter.arc(x, y, coreSize, 0, Math.PI * 2)
            painter.fill()
        }
    }
    FrameClock {
        presentationClock: canvas.context ? canvas.context.presentationClock : null
        running: !canvas.reducedMotion
        onTick: function(deltaSeconds) {
            canvas.phase = (Date.now() - canvas.animationEpochMs) * 0.001
            canvas.requestPaint()
        }
    }
}
