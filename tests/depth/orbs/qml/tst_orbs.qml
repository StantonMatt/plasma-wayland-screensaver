// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
TestCase {
    name: "OrbsSettings"
    function test_galleryControls() {
        testConfig.visualModule = "orbs"
        const component = Qt.createComponent("qrc:/qml/settings/visuals/orbs/Settings.qml")
        const form = createTemporaryObject(component, this, {settings: this, config: testConfig})
        verify(form, component.errorString())
        compare(form.sharedLabels.density, "Orbs")
        compare(form.sharedLabels.scale, "Size")
        compare(form.sharedLabels.trail, "Glow")
        compare(testConfig.visualParameters.look, "glow")
        compare(testConfig.visualParameters.flow, "drift")
        compare(testConfig.visualParameters.depth, 60)
    }
}
