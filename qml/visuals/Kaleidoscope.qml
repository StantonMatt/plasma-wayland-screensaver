// SPDX-License-Identifier: GPL-3.0-or-later
pragma ComponentBehavior: Bound
import QtQuick

Item {
    id: root
    objectName: "kaleidoscopeVisualRoot"
    property var context
    property var nativeRenderer: null
    property int frameRate: 30
    property bool reducedMotion: false
    property int seed: 1
    property double animationEpochMs: Date.now()
    readonly property real density: context ? context.animationDensity : 50
    readonly property real sizeScale: context ? context.animationScale / 100 : 1
    readonly property real motionSpeed: context ? context.animationSpeed / 100 : 1
    readonly property real glowAmount: context ? context.trailAmount / 100 : 0.35
    readonly property string paletteName: context ? context.animationPalette : "spectrum"
    readonly property rect worldBounds: context && context.virtualWidth !== undefined
        ? Qt.rect(context.virtualX, context.virtualY, context.virtualWidth, context.virtualHeight)
        : Qt.rect(0, 0, width, height)
    readonly property point viewportOrigin: context && context.screenX !== undefined
        ? Qt.point(context.screenX, context.screenY) : Qt.point(0, 0)
    clip: true
    // Canvas "lighter" accumulates both RGB and alpha in a transparent RGBA8
    // image, then composites that image source-over. Qt's public item layer
    // provides the same per-window RHI target and owns resize/device recreation.
    layer.enabled: nativeRenderer !== null
    layer.format: ShaderEffectSource.RGBA
    layer.smooth: false
    layer.mipmap: false

    function syncRenderer() {
        if (nativeRenderer) {
            nativeRenderer.setViewport(worldBounds, viewportOrigin)
            nativeRenderer.configureStrokes("kaleidoscope", density, sizeScale, motionSpeed, glowAmount,
                                           paletteName, seed, reducedMotion, animationEpochMs)
        }
    }
    onWorldBoundsChanged: syncRenderer()
    onViewportOriginChanged: syncRenderer()
    onNativeRendererChanged: syncRenderer()
    onDensityChanged: syncRenderer()
    onSizeScaleChanged: syncRenderer()
    onMotionSpeedChanged: syncRenderer()
    onGlowAmountChanged: syncRenderer()
    onPaletteNameChanged: syncRenderer()
    onSeedChanged: syncRenderer()
    onReducedMotionChanged: syncRenderer()
    onAnimationEpochMsChanged: syncRenderer()
}
