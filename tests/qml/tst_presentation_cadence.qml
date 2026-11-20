// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import "../../qml/visuals"

// Physics and retained per-window sampling now run natively. Their unchanged
// contracts are checked in visual-bounce-renderer and Rust frozen-QML fixtures.
TestCase {
    name: "BounceNativeBindings"
    width: 320; height: 180
    visible: true; when: windowShown
    QtObject {
        id: state
        property rect bounds: Qt.rect(-100, 0, 640, 360)
        signal frameChanged()
    }
    QtObject {
        id: settings
        property string monitorBehavior: "independent"
        property var animationState: state
        property int animationDensity: 55
        property real animationScale: 200
        property real animationSpeed: 300
        property real trailAmount: 100
        property string animationPalette: "ocean"
        property int ballCount: 20
        property real ballGravity: 10
        property real ballElasticity: 100
        property bool ballCollisions: true
        property real screenX: -100
        property real screenY: 0
    }
    QtObject {
        id: renderer
        property int updates: 0
        property var inputs: []
        property rect bounds
        property point origin
        function setViewport(b, o) { bounds=b; origin=o }
        function configureBounce(d, scale, speed, trails, palette, seed, reduced, epoch, count, gravity, elasticity, collisions, mode) {
            inputs=[d,scale,speed,trails,palette,seed,reduced,count,gravity,elasticity,collisions,mode]
            ++updates
        }
    }
    Bounce { id: visual; anchors.fill: parent; context: settings; nativeRenderer: renderer; reducedMotion: true }
    function test_pausedInputsReachNativeRenderer() {
        settings.ballCount=2;compare(renderer.inputs[7],2)
        settings.animationScale=125;compare(renderer.inputs[1],1.25)
        settings.animationPalette="ember";compare(renderer.inputs[4],"ember")
        settings.ballGravity=-100;compare(renderer.inputs[8],-100)
        settings.ballElasticity=50;compare(renderer.inputs[9],50)
        settings.ballCollisions=false;compare(renderer.inputs[10],false)
        settings.animationSpeed=10;compare(renderer.inputs[2],.1)
        settings.trailAmount=0;compare(renderer.inputs[3],0)
        visual.seed=-1197;compare(renderer.inputs[5],-1197)
        compare(renderer.inputs[6],true)
    }
    function test_monitorMappingAndPausedSnapshotInvalidation() {
        settings.monitorBehavior="synchronized";compare(renderer.inputs[11],1)
        compare(renderer.bounds,state.bounds)
        const before=renderer.updates;state.frameChanged();verify(renderer.updates>before)
        state.bounds=Qt.rect(-100,-200,1280,720);compare(renderer.bounds,state.bounds)
        settings.monitorBehavior="seamless";compare(renderer.inputs[11],2)
        compare(renderer.origin,Qt.point(-100,0))
        settings.monitorBehavior="independent";compare(renderer.inputs[11],0)
        compare(renderer.bounds,Qt.rect(0,0,320,180))
    }
}
