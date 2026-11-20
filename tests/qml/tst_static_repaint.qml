// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest

TestCase {
    name: "StaticVisualRepaint"
    width: 320
    height: 180
    visible: true
    when: windowShown

    QtObject {
        id: visualContext
        property var presentationClock: null
        property int animationDensity: 50
        property int animationScale: 100
        property int animationSpeed: 100
        property int trailAmount: 35
        property string animationPalette: "ocean"
    }
    SignalSpy { id: paintSpy; signalName: "painted" }
    Component {
        id: rendererComponent
        QtObject {
            property int paints: 0
            property int density: 0
            property real sizeScale: 0
            property real speed: 0
            property real glow: 0
            property string palette: ""
            property int seed: 0
            function configure(d, scale, motion, trail, colors, randomSeed) {
                density = d; sizeScale = scale; speed = motion
                glow = trail; palette = colors; seed = randomSeed
                ++paints
            }
            function presentFrame(phase) { ++paints }
            function setViewport(bounds, origin) {}
            function configureConstellation(d, scale, motion, trail, colors, randomSeed, reduced, epoch) {
                configure(d, scale, motion, trail, colors, randomSeed)
            }
            function configureStrokes(visual, d, scale, motion, trail, colors, randomSeed, reduced, epoch) {
                configure(d, scale, motion, trail, colors, randomSeed)
            }
            function configureStarfield(d, scale, motion, trail, colors, randomSeed, reduced, epoch) {
                configure(d, scale, motion, trail, colors, randomSeed)
            }
            function configureMatrix(d, scale, motion, trail, colors, randomSeed, reduced, epoch) {
                configure(d, scale, motion, trail, colors, randomSeed)
            }
            function configureAurora(d, scale, motion, trail, colors, randomSeed, reduced, epoch) {
                configure(d, scale, motion, trail, colors, randomSeed)
            }
        }
    }

    function test_sceneGraphStaticInputs_data() {
        return [{tag: "Orbs", visual: "Orbs"}]
    }
    function test_sceneGraphStaticInputs(data) {
        const component = Qt.createComponent("../../qml/visuals/" + data.visual + ".qml")
        compare(component.status, Component.Ready, component.errorString())
        const visual = createTemporaryObject(component, this, {
            width: width, height: height, context: visualContext, reducedMotion: true
        })
        verify(visual !== null)
        const before = grabImage(visual)
        visualContext.animationPalette = "ember"
        tryVerify(function() { return !grabImage(visual).equals(before) })
    }
    function test_pausedNativeSettingsRequestPaint_data() {
        return [{tag: "Fireflies", visual: "Fireflies"}, {tag: "Aurora", visual: "Aurora"},
                {tag: "Constellation", visual: "Constellation"}, {tag: "Starfield", visual: "Starfield"},
                {tag: "Ribbons", visual: "Ribbons"}, {tag: "Kaleidoscope", visual: "Kaleidoscope"}, {tag: "Matrix", visual: "Matrix"}]
    }
    function test_pausedNativeSettingsRequestPaint(data) {
        const component = Qt.createComponent("../../qml/visuals/" + data.visual + ".qml")
        compare(component.status, Component.Ready, component.errorString())
        const renderer = createTemporaryObject(rendererComponent, this)
        const visual = createTemporaryObject(component, this, {
            width: width, height: height, context: visualContext,
            reducedMotion: true, nativeRenderer: renderer
        })
        verify(visual !== null)
        let paints = renderer.paints
        visualContext.animationPalette = "ember"
        compare(renderer.palette, "ember")
        verify(renderer.paints > paints)
        paints = renderer.paints
        visualContext.animationDensity = 75
        compare(renderer.density, 75)
        verify(renderer.paints > paints)
        paints = renderer.paints
        visualContext.animationScale = 125
        compare(renderer.sizeScale, 1.25)
        verify(renderer.paints > paints)
        paints = renderer.paints
        visualContext.animationSpeed = 125
        compare(renderer.speed, 1.25)
        verify(renderer.paints > paints)
        paints = renderer.paints
        visualContext.trailAmount = 60
        compare(renderer.glow, 0.6)
        verify(renderer.paints > paints)
        paints = renderer.paints
        visual.seed = 27
        compare(renderer.seed, 27)
        verify(renderer.paints > paints)
    }

    function init() {
        visualContext.animationDensity = 50
        visualContext.animationScale = 100
        visualContext.animationSpeed = 100
        visualContext.trailAmount = 35
        visualContext.animationPalette = "ocean"
    }
    function cleanup() {
        paintSpy.target = null
        paintSpy.clear()
    }
}
