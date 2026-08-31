// SPDX-License-Identifier: GPL-3.0-or-later
pragma ComponentBehavior: Bound
import QtQuick

Item {
    id: root
    objectName: "fireflyVisualRoot"

    property var context
    property var nativeRenderer: null
    property int frameRate: 30
    property bool reducedMotion: false
    property int seed: 1
    property double animationEpochMs: Date.now()
    property real phase: 0

    readonly property real density: context ? context.animationDensity : 50
    readonly property real sizeScale: context ? context.animationScale / 100 : 1
    readonly property real motionSpeed: context ? context.animationSpeed / 100 : 1
    readonly property real glowAmount: context ? context.trailAmount / 100 : 0.35
    readonly property string paletteName: context ? context.animationPalette : "ember"
    readonly property int fireflyCount: Math.round(12 + density * 0.75)
    readonly property int renderedFireflyCount: nativeRenderer ? fireflyCount : 0

    clip: true

    function syncRenderer() {
        if (root.nativeRenderer) {
            root.nativeRenderer.configure(root.density, root.sizeScale, root.motionSpeed,
                                          root.glowAmount,
                                          root.paletteName,
                                          root.seed)
            root.nativeRenderer.presentFrame(root.phase)
        }
    }

    onNativeRendererChanged: syncRenderer()
    onDensityChanged: syncRenderer()
    onSizeScaleChanged: syncRenderer()
    onMotionSpeedChanged: syncRenderer()
    onGlowAmountChanged: syncRenderer()
    onPaletteNameChanged: syncRenderer()
    onSeedChanged: syncRenderer()

    FrameClock {
        presentationClock: root.context ? root.context.presentationClock : null
        running: !root.reducedMotion
        onTick: function(deltaSeconds) {
            root.phase = (Date.now() - root.animationEpochMs) * 0.001
            if (root.nativeRenderer)
                root.nativeRenderer.presentFrame(root.phase)
        }
    }
}
