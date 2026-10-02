// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import "../../../qml/visuals"
import "Recorder.js" as Recorder

TestCase {
    id: testCase
    name: "SnakesMechanicsRecorder"
    width: 1280
    height: 720
    visible: true
    when: windowShown

    QtObject {
        id: visualContext
        property int animationSpeed: 100
        property int animationDensity: 50
        property int animationScale: 100
        property string animationPalette: "spectrum"
        property int trailAmount: 35
        property int snakeIntelligence: 75
        property bool snakeSelfCollisions: false
        property bool snakeDeadlyWalls: false
        property bool developerMode: false
        property string monitorBehavior: "seamless"
        property real virtualX: 0
        property real virtualY: 0
        property real virtualWidth: 1280
        property real virtualHeight: 720
        property real screenX: 0
        property real screenY: 0
        property var presentationClock: null
    }

    // Qt 6 QML methods are read-only from JS. Rename instrumented entry
    // points in an in-memory copy, then delegate through writable hook slots.
    // Relative production imports still resolve beside the real Snakes.qml.
    function createInstrumentedVisual() {
        const url = Qt.resolvedUrl("../../../qml/visuals/Snakes.qml")
        const request = new XMLHttpRequest()
        request.open("GET", url, false)
        request.send()
        verify(request.status === 0 || request.status === 200)
        let source = request.responseText
        verify(source.indexOf("function stepSimulation(seconds)") >= 0)
        const names = ["updateSnakeBrains", "planSteering", "chooseGoal",
                       "applyCollisionSafety", "applyWallSafety", "random",
                       "makeSnake", "consumeFoodParticle", "markCollisions",
                       "explodeSnake", "moveSnake"]
        let wrappers = "\n    property var parityHooks: ({})\n"
        for (let i = 0; i < names.length; ++i) {
            const name = names[i]
            const declaration = new RegExp("function " + name + "\\(([^)]*)\\)")
            const match = declaration.exec(source)
            verify(match !== null)
            source = source.replace(declaration, "function parityOriginal_" + name + "($1)")
            wrappers += "    function " + name + "(" + match[1] + ") { return parityHooks."
                        + name + "(" + match[1] + ") }\n"
        }
        const end = source.lastIndexOf("}")
        source = source.slice(0, end) + wrappers + source.slice(end)
        const visual = Qt.createQmlObject(source, testCase, url)
        visual.simulationDriver = false
        visual.reducedMotion = true
        visual.nativeRenderer = {syncFrame: function() {}, presentFrame: function() {}}
        visual.width = 1280
        visual.height = 720
        visual.context = visualContext
        return visual
    }

    function test_record() {
        const visual = createInstrumentedVisual()
        Recorder.install(visual)
        const scenarios = Recorder.scenarios()
        for (let i = 0; i < scenarios.length; ++i)
            Recorder.run(visual, visualContext, scenarios[i], testCase)
        visual.destroy()
    }
}
