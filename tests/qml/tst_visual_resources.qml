// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import "../../qml/visuals"

TestCase {
    name: "VisualResourceLifetime"
    width: 320
    height: 180
    visible: true
    when: windowShown

    Fireflies {
        id: fireflies
        anchors.fill: parent
        reducedMotion: true
        context: null
    }

    Aurora { id: aurora; anchors.fill: parent; reducedMotion: true }
    Ribbons { id: ribbons; anchors.fill: parent; reducedMotion: true }
    Kaleidoscope { id: kaleidoscope; anchors.fill: parent; reducedMotion: true }
    function test_strokesUseSceneGraphNodes() { verify(!containsCanvas(ribbons)); verify(!containsCanvas(kaleidoscope)) }
    function test_auroraUsesSceneGraphNodes() { verify(!containsCanvas(aurora)) }

    Constellation { id: constellation; anchors.fill: parent; reducedMotion: true }
    Starfield { id: starfield; anchors.fill: parent; reducedMotion: true }
    Bounce { id: bounce; anchors.fill: parent; reducedMotion: true }
    function test_bounceUsesSceneGraphNodes() { verify(!containsCanvas(bounce)) }
    function test_primitiveVisualsUseSceneGraphNodes() {
        verify(!containsCanvas(constellation));verify(!containsCanvas(starfield))
    }
    Matrix { id: matrix; anchors.fill: parent; reducedMotion: true }
    function test_matrixHasNoPerColumnQmlItems() {
        compare(matrix.children.length, 0)
        verify(!containsCanvas(matrix))
    }
    function containsCanvas(item) {
        if (item instanceof Canvas)
            return true
        for (let i = 0; i < item.children.length; ++i) {
            if (containsCanvas(item.children[i]))
                return true
        }
        return false
    }

    function test_firefliesUseSceneGraphNodes() {
        verify(!containsCanvas(fireflies))
        compare(fireflies.fireflyCount, 50)
        compare(fireflies.renderedFireflyCount, 0)
    }
}
