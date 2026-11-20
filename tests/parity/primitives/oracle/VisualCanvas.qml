// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick

Canvas {
    id: canvas
    property var context
    property int frameRate: 30
    property bool reducedMotion: false
    property int seed: 1
    property double animationEpochMs: Date.now()
    property real phase: 0

    // Share the render thread and invalidate static inputs even when no
    // presentation clock is running. None of these handlers runs per tick.
    renderTarget: Canvas.Image
    renderStrategy: Canvas.Cooperative
    onWidthChanged: requestPaint()
    onHeightChanged: requestPaint()
    onContextChanged: requestPaint()
    onSeedChanged: requestPaint()
    onReducedMotionChanged: requestPaint()

    Connections {
        target: canvas.context
        ignoreUnknownSignals: true
        function onAnimationDensityChanged() { canvas.requestPaint() }
        function onAnimationScaleChanged() { canvas.requestPaint() }
        function onAnimationSpeedChanged() { canvas.requestPaint() }
        function onAnimationPaletteChanged() { canvas.requestPaint() }
        function onTrailAmountChanged() { canvas.requestPaint() }
    }
}
