// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest

TestCase {
    name: "ConstellationSettings"
    function test_releasedChartLabels() {
        testConfig.visualModule = "constellation"
        verify(testConfig.depthAvailable)
        const component = Qt.createComponent("qrc:/qml/settings/visuals/constellation/Settings.qml")
        const form = createTemporaryObject(component, this, {settings: this, config: testConfig})
        verify(form)
        compare(form.sharedLabels.trail, "Lines")
    }
}
