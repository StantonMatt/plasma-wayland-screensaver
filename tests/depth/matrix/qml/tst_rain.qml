// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest

TestCase {
    name: "DigitalRainSettings"
    function test_labelsAndLightning() {
        testConfig.visualModule = "matrix"
        const component = Qt.createComponent("qrc:/qml/settings/visuals/matrix/Settings.qml")
        const form = createTemporaryObject(component, this, {settings: this, config: testConfig})
        verify(form)
        compare(form.sharedLabels.density,"Rainfall")
        compare(form.sharedLabels.trail,"Afterglow")
        const lightning = findChild(form, "rainLightning")
        verify(lightning)
        testConfig.setVisualParameter("weather","steady")
        compare(lightning.enabled,false)
        testConfig.setVisualParameter("weather","stormy")
        compare(lightning.enabled,true)
    }
}
