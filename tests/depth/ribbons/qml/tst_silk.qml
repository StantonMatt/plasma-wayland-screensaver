import QtQuick
import QtTest

TestCase {
    name: "RibbonSettings"
    function test_controls() {
        testConfig.visualModule = "ribbons"
        const component = Qt.createComponent("qrc:/qml/settings/visuals/ribbons/Settings.qml")
        const form = createTemporaryObject(component, this, {settings: this, config: testConfig})
        verify(form)
        compare(form.sharedLabels.scale, "Thickness")
        compare(form.sharedLabels.trail, "Glow")
        const sliders = []
        for (const child of form.children)
            if (child.accessibleName !== undefined) sliders.push(child)
        compare(sliders.length, 3)
        testConfig.setVisualParameter("turbulence", 0)
        compare(sliders[0].valueText, "Calm")
        testConfig.setVisualParameter("turbulence", 100)
        compare(sliders[0].valueText, "Stormy")
        testConfig.setVisualParameter("twist", 0)
        compare(sliders[1].valueText, "Flat")
    }
}
