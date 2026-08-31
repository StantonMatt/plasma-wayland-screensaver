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
