// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick

Canvas {
    id: canvas
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
        ctx.fillStyle = "#03050b"
        ctx.fillRect(0, 0, width, height)

        const colors = ["#4d46d9a7", "#454e7fff", "#354ad9ce", "#2f995de8"]
        const middleColors = ["#1846d9a7", "#184e7fff", "#144ad9ce", "#14995de8"]
        for (let i = 0; i < colors.length; ++i) {
            const travel = reducedMotion ? 0.08 : 0.20
            const x = width * (0.18 + i * 0.23 + Math.sin(phase * 0.55 + i * 1.7) * travel)
            const y = height * (0.35 + Math.cos(phase * 0.42 + i * 1.3) * 0.24)
            const radius = Math.max(width, height) * (0.38 + i * 0.035)
            const gradient = ctx.createRadialGradient(x, y, 0, x, y, radius)
            gradient.addColorStop(0, colors[i])
            gradient.addColorStop(0.50, middleColors[i])
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
    }

    Timer {
        interval: Math.round(1000 / Math.max(1, canvas.frameRate))
        repeat: true
        running: !canvas.reducedMotion
        onTriggered: {
            canvas.phase = (canvas.seed % 1000) / 159.0
                           + (Date.now() - canvas.animationEpochMs) * 0.00054
            canvas.requestPaint()
        }
    }
}
