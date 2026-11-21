// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
TestCase {
    name: "FirefliesSettings"

    function test_meadowSettings() {
        testConfig.visualModule = "fireflies"
        verify(testConfig.depthAvailable)
        const component = Qt.createComponent("qrc:/qml/settings/visuals/fireflies/Settings.qml")
        const form = createTemporaryObject(component, this, {settings: this, config: testConfig})
        verify(form, component.errorString())
        compare(form.sharedLabels.trail, "Light trails")
        compare(form.sharedLabels.density, "Swarm size")
        compare(testConfig.visualParameters.synchrony, 55)
        for (const key of ["nightCycle", "nightEvents", "hunters", "grass"])
            compare(testConfig.visualParameters[key], true)
    }
}
