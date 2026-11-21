// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest

TestCase {
    name: "BounceSettings"
    property string lastKey
    property var lastValue
    function writeVisualParameter(key, value) {
        lastKey = key
        lastValue = value
    }
    function test_legacy_count_data() {
        const rows = []
        for (const mode of ["independent", "synchronized", "seamless"])
            for (const count of [20, 21, 60])
                rows.push({tag: mode + "-" + count, mode: mode, count: count})
        return rows
    }
    function test_legacy_count(data) {
        const component = Qt.createComponent("qrc:/qml/visuals/Bounce.qml")
        const context = {ballCount: data.count, monitorBehavior: data.mode,
                         animationDensity: 50, animationScale: 100, animationSpeed: 100,
                         trailAmount: 35, animationPalette: "ocean"}
        const visual = createTemporaryObject(component, this, {context: context})
        verify(visual, component.errorString())
        compare(visual.ballCount, 20)
        compare(context.ballCount, data.count)
    }
    function test_controls() {
        testConfig.visualModule = "bounce"
        const component = Qt.createComponent("qrc:/qml/settings/visuals/bounce/Settings.qml")
        const form = createTemporaryObject(component, this, {settings: this, config: testConfig})
        verify(form, component.errorString())
        compare(form.sharedLabels.density, "")
        compare(form.sharedLabels.scale, "Ball size")
        compare(form.sharedLabels.trail, "Light trails")
        const count = findChild(form, "bounceCount")
        verify(count)
        compare(count.from, 1)
        compare(count.to, 60)
        count.value = 60
        count.valueModified()
        compare(lastKey, "count")
        compare(lastValue, 60)
        const material = findChild(form, "bounceMaterial")
        compare(material.keys, ["mixed", "rubber", "glass", "metal"])
        material.activated(2)
        compare(lastKey, "material")
        compare(lastValue, "glass")
    }
}
