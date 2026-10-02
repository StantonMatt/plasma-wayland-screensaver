// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import "../../qml/visuals"

TestCase {
    name: "NativeSnakes"
    width: 640
    height: 360
    visible: true
    when: windowShown

    QtObject {
        id: visualContext
        property var snakeSimulation: snakeTestSimulation
        property string monitorBehavior: "synchronized"
        property bool developerMode: false
        property real virtualX: -640
        property real virtualY: 0
        property real screenX: 0
        property real screenY: 0
    }
    Snakes {
        id: visual
        anchors.fill: parent
        context: visualContext
    }
    function test_loadsNativeView() {
        const renderer = findChild(visual, "snakeNativeRenderer")
        verify(renderer !== null)
        compare(renderer.simulation, snakeTestSimulation)
        compare(renderer.width, visual.width)
        verify(renderer.scaleToViewport)
        renderer.update()
        verify(waitForRendering(visual, 5000))
        const image = grabImage(visual)
        compare(image.width, visual.width)
        compare(image.height, visual.height)
    }
    function test_nativePixels() {
        if (visual.GraphicsInfo.api === GraphicsInfo.Software)
            skip("Qt software scene graph does not draw custom geometry; C++ tests verify its triangles")
        findChild(visual, "snakeNativeRenderer").update()
        verify(waitForRendering(visual, 5000))
        const image = grabImage(visual)
        let coloredPixels = 0
        for (let y = 0; y < image.height; y += 8) {
            for (let x = 0; x < image.width; x += 8) {
                if (image.red(x, y) !== image.green(x, y)
                        || image.green(x, y) !== image.blue(x, y))
                    ++coloredPixels
            }
        }
        verify(coloredPixels > 10, "Native snake geometry must produce colored pixels")
    }
    function test_modePreservesManagerViewportOffset() {
        const renderer = findChild(visual, "snakeNativeRenderer")
        // OverlayManager supplies actual viewport offsets. Screen/virtual
        // context coordinates must not reintroduce full-screen geometry.
        renderer.drawOffsetX = -640
        visualContext.monitorBehavior = "seamless"
        compare(renderer.drawOffsetX, -640)
        verify(!renderer.scaleToViewport)
        visualContext.monitorBehavior = "synchronized"
        compare(renderer.drawOffsetX, -640)
        renderer.drawOffsetX = 0
    }
}
