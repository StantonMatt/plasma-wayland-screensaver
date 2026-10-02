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
        }
    }

    function test_sceneGraphStaticInputs_data() {
        return [{tag: "Orbs", visual: "Orbs"}, {tag: "Matrix", visual: "Matrix"}]
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
    function test_pausedFireflySettingsRequestNativePaint() {
        const component = Qt.createComponent("../../qml/visuals/Fireflies.qml")
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
    function test_staticInputsRepaint_data() {
        const rows = []
        const visuals = ["Aurora", "Starfield", "Constellation", "Kaleidoscope", "Ribbons"]
        const inputs = ["animationDensity", "animationScale", "animationSpeed",
                        "trailAmount", "animationPalette", "seed", "reducedMotion", "context"]
        for (const visual of visuals)
            for (const input of inputs)
                rows.push({tag: visual + "-" + input, visual: visual, input: input})
        return rows
    }
    function test_staticInputsRepaint(data) {
        const component = Qt.createComponent("../../qml/visuals/" + data.visual + ".qml")
        compare(component.status, Component.Ready, component.errorString())
        const visual = createTemporaryObject(component, this, {
            width: width, height: height, context: visualContext, reducedMotion: true
        })
        verify(visual !== null)
        paintSpy.target = visual
        verify(paintSpy.valid)
        visual.requestPaint()
        tryVerify(function() { return paintSpy.count > 0 })
        // Finish initial upload before clearing the spy; a delayed initial
        // paint must not masquerade as invalidation by the changed input.
        grabImage(visual)
        paintSpy.clear()
        if (data.input === "seed") visual.seed = 27
        else if (data.input === "reducedMotion") visual.reducedMotion = false
        else if (data.input === "context") visual.context = null
        else if (data.input === "animationPalette") visualContext.animationPalette = "ember"
        else visualContext[data.input] += 25
        tryVerify(function() { return paintSpy.count > 0 })
    }
}
