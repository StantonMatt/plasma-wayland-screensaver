// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest

TestCase {
    name: "KaleidoscopeSettings"
    visible: true
    width: 640
    height: 480
    QtObject {
        id: config
        property var visualParameters: ({mirrors: "mandala", points: 8, glass: "mixed", turning: 40, events: true})
    }
    function test_mirrorsAndLabels() {
        const component = Qt.createComponent("qrc:/qml/settings/visuals/kaleidoscope/Settings.qml")
        const form = createTemporaryObject(component, this, {settings: this, config: config})
        verify(form, component.errorString())
        compare(form.sharedLabels.density, "Pieces")
        compare(form.sharedLabels.scale, "Zoom")
        compare(form.sharedLabels.trail, "Glow")
        const points = findChild(form, "kaleidoscopePoints")
        verify(points)
        compare(points.value, 8)
        verify(points.visible)
        config.visualParameters = Object.assign({}, config.visualParameters, {mirrors: "lattice"})
        verify(!points.visible)
    }
}
