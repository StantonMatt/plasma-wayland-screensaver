// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest

TestCase {
    name: "AuroraSettings"
    function test_skyControls() {
        testConfig.visualModule = "aurora"
        const component = Qt.createComponent("qrc:/qml/settings/visuals/aurora/Settings.qml")
        const form = createTemporaryObject(component, this, {settings: this, config: testConfig})
        verify(form)
        compare(form.sharedLabels.density, "Curtains")
        compare(form.sharedLabels.scale, "Height")
        compare(form.sharedLabels.trail, "Afterglow")
        compare(testConfig.visualParameters.horizon, "forest")
        compare(testConfig.visualParameters.activity, 40)
        compare(testConfig.visualParameters.stars, true)
        compare(testConfig.visualParameters.skyEvents, true)
    }
}
