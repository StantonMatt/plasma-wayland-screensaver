// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest

TestCase {
    name: "StarfieldSettings"
    function test_flightControls() {
        testConfig.visualModule = "starfield"
        const component = Qt.createComponent("qrc:/qml/settings/visuals/starfield/Settings.qml")
        const form = createTemporaryObject(component, this, {settings: this, config: testConfig})
        verify(form)
        compare(form.sharedLabels.speed, "Flight speed")
        compare(form.sharedLabels.density, "Stars")
        compare(form.sharedLabels.trail, "Trails")
        compare(testConfig.visualParameters.flight, "voyage")
        compare(testConfig.visualParameters.steering, 40)
        compare(testConfig.visualParameters.nebulae, 45)
        compare(testConfig.visualParameters.events, true)
    }
}
