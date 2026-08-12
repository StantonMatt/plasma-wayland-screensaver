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
        const palette = Utils.colors(context ? context.animationPalette : "spectrum")
        const segments = Math.max(5, Math.round(5 + density / 9))
        const layers = Math.max(3, Math.round(3 + density / 18))
        const radius = Math.min(width, height) * 0.46 * scale
        painter.save()
        painter.translate(width / 2, height / 2)
        painter.rotate(phase * speed * 0.32)
        painter.globalCompositeOperation = "lighter"
        for (let segment = 0; segment < segments; ++segment) {
            painter.save()
            painter.rotate(segment * Math.PI * 2 / segments)
            for (let layer = 0; layer < layers; ++layer) {
                const radial = radius * (0.18 + layer / layers * 0.82)
                const wobble = Math.sin(phase * speed + layer * 1.7 + seed) * radius * 0.09
                const size = Math.max(3, radius * (0.035 + glow * 0.045))
                painter.globalAlpha = 0.22 + glow * 0.34
                painter.fillStyle = palette[(segment + layer) % palette.length]
                painter.beginPath()
                painter.arc(radial, wobble, size, 0, Math.PI * 2)
                painter.arc(radial * 0.72, -wobble * 1.3, size * 0.62, 0, Math.PI * 2)
                painter.fill()
                painter.strokeStyle = palette[(segment + layer + 2) % palette.length]
                painter.lineWidth = 1 + glow * 3
                painter.beginPath()
                painter.moveTo(radial * 0.28, 0)
                painter.quadraticCurveTo(radial * 0.62, wobble * 1.8, radial, wobble)
                painter.stroke()
            }
            painter.restore()
        }
        painter.restore()
    }
    Timer {
        interval: Math.round(1000 / Math.max(1, canvas.frameRate))
        repeat: true
        running: !canvas.reducedMotion
        onTriggered: {
            canvas.phase = (Date.now() - canvas.animationEpochMs) * 0.001
            canvas.requestPaint()
        }
    }
}
