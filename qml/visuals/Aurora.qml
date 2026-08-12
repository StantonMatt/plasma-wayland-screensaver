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
    property real phase: (seed % 1000) / 159.0

    renderTarget: Canvas.FramebufferObject
    renderStrategy: Canvas.Cooperative

    onWidthChanged: requestPaint()
    onHeightChanged: requestPaint()
    onSeedChanged: requestPaint()

    onPaint: {
        const ctx = getContext("2d")
        ctx.reset()
        ctx.clearRect(0, 0, width, height)

        const colors = Utils.colors(context ? context.animationPalette : "ocean")
        const density = context ? context.animationDensity : 50
        const scale = context ? context.animationScale / 100 : 1
        const glow = context ? context.trailAmount / 100 : 0.35
        const bandCount = Math.max(3, Math.round(3 + density / 18))
        for (let i = 0; i < bandCount; ++i) {
            const travel = reducedMotion ? 0.08 : 0.14 + density * 0.0012
            const x = width * (0.18 + i * 0.23 + Math.sin(phase * 0.55 + i * 1.7) * travel)
            const y = height * (0.35 + Math.cos(phase * 0.42 + i * 1.3) * 0.24)
            const radius = Math.max(width, height) * (0.30 + i * 0.025) * scale
            const gradient = ctx.createRadialGradient(x, y, 0, x, y, radius)
            ctx.globalAlpha = 0.24 + glow * 0.44
            gradient.addColorStop(0, colors[i % colors.length])
            gradient.addColorStop(0.50, colors[(i + 2) % colors.length])
            gradient.addColorStop(1, "#00000000")
            ctx.fillStyle = gradient
            ctx.fillRect(0, 0, width, height)
        }

        const shade = ctx.createLinearGradient(0, 0, 0, height)
        shade.addColorStop(0, "#30000000")
        shade.addColorStop(0.55, "#00000000")
        shade.addColorStop(1, "#78000000")
        ctx.fillStyle = shade
        ctx.fillRect(0, 0, width, height)
        ctx.globalAlpha = 1
    }

    Timer {
        interval: Math.round(1000 / Math.max(1, canvas.frameRate))
        repeat: true
        running: !canvas.reducedMotion
        onTriggered: {
            const speed = canvas.context ? canvas.context.animationSpeed / 100 : 1
            canvas.phase = (canvas.seed % 1000) / 159.0
                           + (Date.now() - canvas.animationEpochMs) * 0.00054 * speed
            canvas.requestPaint()
        }
    }
}
