// SPDX-License-Identifier: GPL-3.0-or-later
pragma ComponentBehavior: Bound
import QtQuick

Item {
    id: root
    objectName: "bounceVisualRoot"
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
    readonly property rect worldBounds: sharedState && mode !== 0 ? sharedState.bounds
        : context && context.virtualWidth !== undefined
        ? Qt.rect(context.virtualX, context.virtualY, context.virtualWidth, context.virtualHeight)
        : Qt.rect(0, 0, width, height)
    readonly property point viewportOrigin: context && context.screenX !== undefined
        ? Qt.point(context.screenX, context.screenY) : Qt.point(0, 0)
    readonly property int ballCount: context && context.ballCount !== undefined ? context.ballCount : 1
    readonly property real gravity: context && context.ballGravity !== undefined ? context.ballGravity : 0
    readonly property real elasticity: context && context.ballElasticity !== undefined ? context.ballElasticity : 100
    readonly property bool collisions: context && context.ballCollisions !== undefined ? context.ballCollisions : false
    readonly property int mode: !context || context.monitorBehavior === undefined || context.monitorBehavior === "independent"
        ? 0 : context.monitorBehavior === "seamless" ? 2 : 1
    readonly property var sharedState: context && context.animationState !== undefined ? context.animationState : null
    Connections {
        target: root.sharedState
        enabled: root.reducedMotion
        function onFrameChanged() { root.syncRenderer() }
    }
    onBallCountChanged: syncRenderer()
    onGravityChanged: syncRenderer()
    onElasticityChanged: syncRenderer()
    onCollisionsChanged: syncRenderer()
    onModeChanged: syncRenderer()
    clip: true

    function syncRenderer() {
        if (nativeRenderer) {
            nativeRenderer.setViewport(worldBounds, viewportOrigin)
            nativeRenderer.configureBounce(density, sizeScale, motionSpeed, glowAmount,
                                           paletteName, seed, reducedMotion, animationEpochMs, ballCount, gravity, elasticity, collisions, mode)
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
