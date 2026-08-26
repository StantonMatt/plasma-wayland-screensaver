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

    function test_canvasUsesSharedRenderThread() {
        compare(fireflies.renderTarget, Canvas.Image)
        compare(fireflies.renderStrategy, Canvas.Cooperative)
    }
}
