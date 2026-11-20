// SPDX-License-Identifier: GPL-3.0-or-later
pragma ComponentBehavior: Bound
import QtQuick

Item {
    id: root
    objectName: "constellationVisualRoot"
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
    readonly property string paletteName: context ? context.animationPalette : "ocean"
    readonly property rect worldBounds: context && context.virtualWidth !== undefined
        ? Qt.rect(context.virtualX, context.virtualY, context.virtualWidth, context.virtualHeight)
        : Qt.rect(0, 0, width, height)
    readonly property point viewportOrigin: context && context.screenX !== undefined
        ? Qt.point(context.screenX, context.screenY) : Qt.point(0, 0)
    clip: true

    function syncRenderer() {
        if (nativeRenderer) {
            nativeRenderer.setViewport(worldBounds, viewportOrigin)
            nativeRenderer.configureConstellation(density, sizeScale, motionSpeed, glowAmount,
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
