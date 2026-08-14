// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    id: window
    required property var controller
    required property var screensaverConfig
    width: 720
    height: 900
    minimumWidth: 600
    minimumHeight: 700
    visible: true
    title: qsTr("Plasma Visual Screensaver")

    onClosing: function(close) {
        close.accepted = false
        window.hide()
    }

    function store() {
        controller.saveSettings({
            idleMinutes: idleTimeout.value,
            visualModule: visual.currentValue,
            backgroundStyle: background.currentValue,
            animationSpeed: Math.round(animationSpeed.value),
            animationDensity: Math.round(animationDensity.value),
            animationScale: Math.round(animationScale.value),
            animationPalette: paletteChoice.currentValue,
            trailAmount: Math.round(trailAmount.value),
            ballCount: ballCount.value,
            ballGravity: Math.round(ballGravity.value),
            ballElasticity: Math.round(ballElasticity.value),
            ballCollisions: ballCollisions.checked,
            snakeIntelligence: Math.round(snakeIntelligence.value),
            snakeSelfCollisions: snakeSelfCollisions.checked,
            snakeDeadlyWalls: snakeDeadlyWalls.checked,
            showClock: showClock.checked,
            clockMovement: clockMovement.currentValue,
            clockSpeed: clockSpeed.currentValue,
            frameRate: frameRate.currentValue,
            reducedMotion: reducedMotion.checked,
            monitorBehavior: monitors.currentValue,
            coverPanels: coverPanels.checked
        })
    }

    function optionTitle() {
        switch (visual.currentValue) {
        case "bounce": return qsTr("Bouncing Balls physics")
        case "starfield": return qsTr("Hyperspace flight controls")
        case "matrix": return qsTr("Digital Rain controls")
        case "kaleidoscope": return qsTr("Kaleidoscope geometry")
        case "fireflies": return qsTr("Firefly swarm controls")
        case "ribbons": return qsTr("Neon Ribbon controls")
        case "constellation": return qsTr("Constellation controls")
        case "snakes": return qsTr("Slithering Snakes ecosystem")
        case "orbs": return qsTr("Floating Orb controls")
        default: return qsTr("Aurora controls")
        }
    }

    function speedLabel() {
        if (visual.currentValue === "bounce") return qsTr("Physics speed")
        if (visual.currentValue === "starfield") return qsTr("Warp speed")
        if (visual.currentValue === "matrix") return qsTr("Fall speed")
        if (visual.currentValue === "fireflies") return qsTr("Swarm speed")
        if (visual.currentValue === "snakes") return qsTr("Slither speed")
        return qsTr("Motion speed")
    }

    function densityLabel() {
        switch (visual.currentValue) {
        case "aurora": return qsTr("Aurora layers")
        case "orbs": return qsTr("Orb count")
        case "starfield": return qsTr("Star count")
        case "matrix": return qsTr("Glyph density")
        case "kaleidoscope": return qsTr("Symmetry & detail")
        case "fireflies": return qsTr("Swarm size")
        case "ribbons": return qsTr("Ribbon count")
        case "constellation": return qsTr("Star count")
        case "snakes": return qsTr("Snake population")
        default: return qsTr("Density")
        }
    }

    function scaleLabel() {
        switch (visual.currentValue) {
        case "bounce": return qsTr("Ball size")
        case "matrix": return qsTr("Glyph size")
        case "kaleidoscope": return qsTr("Pattern spread")
        case "ribbons": return qsTr("Ribbon thickness")
        case "constellation": return qsTr("Star size")
        case "snakes": return qsTr("Snake thickness")
        default: return qsTr("Element size")
        }
    }

    function trailLabel() {
        if (visual.currentValue === "bounce" || visual.currentValue === "starfield")
            return qsTr("Trail length")
        if (visual.currentValue === "constellation")
            return qsTr("Link brightness")
        if (visual.currentValue === "snakes")
            return qsTr("Food abundance")
        return qsTr("Glow intensity")
    }

    header: ToolBar {
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 22
            anchors.rightMargin: 16
            Label {
                text: qsTr("Visual Screensaver")
                font.pixelSize: 23
                font.weight: Font.DemiBold
                Layout.fillWidth: true
            }
            Button {
                text: qsTr("Preview")
                onClicked: { window.store(); window.controller.Preview() }
            }
            Button {
                text: qsTr("Save")
                highlighted: true
                onClicked: window.store()
            }
        }
    }

    ScrollView {
        anchors.fill: parent
        contentWidth: availableWidth

        ColumnLayout {
            width: parent.width
            spacing: 14

            Label {
                Layout.fillWidth: true
                Layout.margins: 22
                Layout.bottomMargin: 0
                wrapMode: Text.WordWrap
                color: palette.mid
                text: qsTr("A decorative overlay for an unlocked session. It does not lock the computer or protect your data.")
            }

            GroupBox {
                title: qsTr("Activation and appearance")
                Layout.fillWidth: true
                Layout.leftMargin: 22
                Layout.rightMargin: 22

                GridLayout {
                    anchors.fill: parent
                    columns: 2
                    columnSpacing: 22
                    rowSpacing: 12

                    Label { text: qsTr("Start after") }
                    RowLayout {
                        SpinBox {
                            id: idleTimeout
                            from: 1; to: 240
                            value: window.screensaverConfig.idleMinutes
                        }
                        Label { text: qsTr("minutes of inactivity") }
                    }

                    Label { text: qsTr("Animation") }
                    ComboBox {
                        id: visual
                        Layout.fillWidth: true
                        textRole: "text"; valueRole: "value"
                        model: [
                            { text: qsTr("None"), value: "none" },
                            { text: qsTr("Aurora Drift"), value: "aurora" },
                            { text: qsTr("Floating Orbs"), value: "orbs" },
                            { text: qsTr("Bouncing Balls"), value: "bounce" },
                            { text: qsTr("Hyperspace"), value: "starfield" },
                            { text: qsTr("Digital Rain"), value: "matrix" },
                            { text: qsTr("Kaleidoscope"), value: "kaleidoscope" },
                            { text: qsTr("Fireflies"), value: "fireflies" },
                            { text: qsTr("Neon Ribbons"), value: "ribbons" },
                            { text: qsTr("Constellation"), value: "constellation" },
                            { text: qsTr("Slithering Snakes"), value: "snakes" }
                        ]
                        Component.onCompleted: currentIndex = indexOfValue(window.screensaverConfig.visualModule)
                    }

                    Label { text: qsTr("Background") }
                    ComboBox {
                        id: background
                        Layout.fillWidth: true
                        textRole: "text"; valueRole: "value"
                        model: [
                            { text: qsTr("Pure Black"), value: "black" },
                            { text: qsTr("Midnight Gradient"), value: "midnight" },
                            { text: qsTr("Deep Ocean"), value: "ocean" },
                            { text: qsTr("Dark Plum"), value: "plum" }
                        ]
                        Component.onCompleted: currentIndex = indexOfValue(window.screensaverConfig.backgroundStyle)
                    }
                }
            }

            GroupBox {
                title: window.optionTitle()
                visible: visual.currentValue !== "none"
                Layout.fillWidth: true
                Layout.leftMargin: 22
                Layout.rightMargin: 22

                GridLayout {
                    anchors.fill: parent
                    columns: 3
                    columnSpacing: 16
                    rowSpacing: 8

                    Label { text: qsTr("Color palette") }
                    ComboBox {
                        id: paletteChoice
                        Layout.columnSpan: 2
                        Layout.fillWidth: true
                        textRole: "text"; valueRole: "value"
                        model: [
                            { text: qsTr("Ocean Electric"), value: "ocean" },
                            { text: qsTr("Full Spectrum"), value: "spectrum" },
                            { text: qsTr("Ember & Gold"), value: "ember" },
                            { text: qsTr("Forest Glow"), value: "forest" },
                            { text: qsTr("Monochrome"), value: "mono" },
                            { text: qsTr("Soft Pastels"), value: "pastel" }
                        ]
                        Component.onCompleted: currentIndex = indexOfValue(window.screensaverConfig.animationPalette)
                    }

                    Label { text: window.speedLabel() }
                    Slider {
                        id: animationSpeed
                        Layout.fillWidth: true
                        from: 10; to: 300; stepSize: 10
                        value: window.screensaverConfig.animationSpeed
                    }
                    Label { text: Math.round(animationSpeed.value) + "%"; horizontalAlignment: Text.AlignRight }

                    Label { text: window.densityLabel(); visible: visual.currentValue !== "bounce" }
                    Slider {
                        id: animationDensity
                        Layout.fillWidth: true
                        from: 10; to: 100; stepSize: 5
                        value: window.screensaverConfig.animationDensity
                        visible: visual.currentValue !== "bounce"
                    }
                    Label {
                        text: Math.round(animationDensity.value) + "%"
                        visible: visual.currentValue !== "bounce"
                        horizontalAlignment: Text.AlignRight
                    }

                    Label { text: window.scaleLabel() }
                    Slider {
                        id: animationScale
                        Layout.fillWidth: true
                        from: 25; to: 200; stepSize: 5
                        value: window.screensaverConfig.animationScale
                    }
                    Label { text: Math.round(animationScale.value) + "%"; horizontalAlignment: Text.AlignRight }

                    Label { text: window.trailLabel() }
                    Slider {
                        id: trailAmount
                        Layout.fillWidth: true
                        from: 0; to: 100; stepSize: 5
                        value: window.screensaverConfig.trailAmount
                    }
                    Label { text: Math.round(trailAmount.value) + "%"; horizontalAlignment: Text.AlignRight }

                    Label { text: qsTr("Number of balls"); visible: visual.currentValue === "bounce" }
                    SpinBox {
                        id: ballCount
                        from: 1; to: 20
                        value: window.screensaverConfig.ballCount
                        visible: visual.currentValue === "bounce"
                    }
                    Item { visible: visual.currentValue === "bounce" }

                    Label { text: qsTr("Gravity"); visible: visual.currentValue === "bounce" }
                    Slider {
                        id: ballGravity
                        Layout.fillWidth: true
                        from: -100; to: 100; stepSize: 5
                        value: window.screensaverConfig.ballGravity
                        visible: visual.currentValue === "bounce"
                    }
                    Label {
                        visible: visual.currentValue === "bounce"
                        text: ballGravity.value < -2 ? qsTr("Up %1").arg(Math.abs(Math.round(ballGravity.value)))
                              : (ballGravity.value > 2 ? qsTr("Down %1").arg(Math.round(ballGravity.value))
                                 : qsTr("Zero-G"))
                    }

                    Label { text: qsTr("Elasticity"); visible: visual.currentValue === "bounce" }
                    Slider {
                        id: ballElasticity
                        Layout.fillWidth: true
                        from: 50; to: 100; stepSize: 1
                        value: window.screensaverConfig.ballElasticity
                        visible: visual.currentValue === "bounce"
                    }
                    Label { text: Math.round(ballElasticity.value) + "%"; visible: visual.currentValue === "bounce" }

                    Label {
                        text: qsTr("Intelligence")
                        visible: visual.currentValue === "snakes"
                    }
                    Slider {
                        id: snakeIntelligence
                        Layout.fillWidth: true
                        from: 0; to: 100; stepSize: 5
                        value: window.screensaverConfig.snakeIntelligence
                        visible: visual.currentValue === "snakes"
                    }
                    Label {
                        text: Math.round(snakeIntelligence.value) + "%"
                        visible: visual.currentValue === "snakes"
                        horizontalAlignment: Text.AlignRight
                    }

                    Label {
                        text: qsTr("Self collision")
                        visible: visual.currentValue === "snakes"
                    }
                    CheckBox {
                        id: snakeSelfCollisions
                        Layout.columnSpan: 2
                        text: qsTr("Snakes can crash into their own bodies")
                        checked: window.screensaverConfig.snakeSelfCollisions
                        visible: visual.currentValue === "snakes"
                    }

                    Label {
                        text: qsTr("Screen edges")
                        visible: visual.currentValue === "snakes"
                    }
                    CheckBox {
                        id: snakeDeadlyWalls
                        Layout.columnSpan: 2
                        text: qsTr("Edges are deadly walls")
                        checked: window.screensaverConfig.snakeDeadlyWalls
                        visible: visual.currentValue === "snakes"
                    }

                    Label {
                        Layout.columnSpan: 3
                        Layout.fillWidth: true
                        visible: visual.currentValue === "snakes"
                        wrapMode: Text.WordWrap
                        color: palette.mid
                        text: qsTr("At high intelligence, snakes predict moving rivals, plan around continuous body shapes and their own turning radius, remember an escape direction, and weave curved vacuum paths through the richest food regions. They also understand when their own tail will vacate a route and sweep efficiently along defeated rivals' particle trails. Champions can become very long, but adaptive screen-area and ecosystem budgets prevent one snake from filling the display. The optimized renderer caps this animation at 60 fps to avoid wasting power on duplicate high-refresh frames.")
                    }

                    Label { text: qsTr("Ball interaction"); visible: visual.currentValue === "bounce" }
                    CheckBox {
                        id: ballCollisions
                        Layout.columnSpan: 2
                        text: qsTr("Balls collide with each other")
                        checked: window.screensaverConfig.ballCollisions
                        visible: visual.currentValue === "bounce"
                    }
                }
            }

            GroupBox {
                title: qsTr("Clock and displays")
                Layout.fillWidth: true
                Layout.leftMargin: 22
                Layout.rightMargin: 22

                GridLayout {
                    anchors.fill: parent
                    columns: 2
                    columnSpacing: 22
                    rowSpacing: 10

                    Label { text: qsTr("Clock") }
                    CheckBox { id: showClock; text: qsTr("Show clock and date"); checked: window.screensaverConfig.showClock }

                    Label { text: qsTr("Clock movement") }
                    ComboBox {
                        id: clockMovement
                        Layout.fillWidth: true
                        textRole: "text"; valueRole: "value"
                        model: [
                            { text: qsTr("Move around the display"), value: "bounce" },
                            { text: qsTr("Stay centered"), value: "center" }
                        ]
                        Component.onCompleted: currentIndex = indexOfValue(window.screensaverConfig.clockMovement)
                        enabled: showClock.checked
                    }

                    Label { text: qsTr("Clock speed") }
                    ComboBox {
                        id: clockSpeed
                        Layout.fillWidth: true
                        textRole: "text"; valueRole: "value"
                        model: [
                            { text: qsTr("Slow"), value: "slow" },
                            { text: qsTr("Normal"), value: "normal" },
                            { text: qsTr("Fast"), value: "fast" }
                        ]
                        Component.onCompleted: currentIndex = indexOfValue(window.screensaverConfig.clockSpeed)
                        enabled: showClock.checked && clockMovement.currentValue === "bounce"
                    }

                    Label { text: qsTr("Multiple monitors") }
                    ComboBox {
                        id: monitors
                        Layout.fillWidth: true
                        textRole: "text"; valueRole: "value"
                        model: [
                            { text: qsTr("Independent motion on every monitor"), value: "independent" },
                            { text: qsTr("Synchronized motion on every monitor"), value: "synchronized" },
                            { text: qsTr("Seamless virtual desktop"), value: "seamless" }
                        ]
                        Component.onCompleted: currentIndex = indexOfValue(window.screensaverConfig.monitorBehavior)
                    }

                    Label { text: qsTr("Panels") }
                    CheckBox { id: coverPanels; text: qsTr("Cover taskbars and panels"); checked: window.screensaverConfig.coverPanels }
                }
            }

            GroupBox {
                title: qsTr("Power and accessibility")
                Layout.fillWidth: true
                Layout.leftMargin: 22
                Layout.rightMargin: 22

                GridLayout {
                    anchors.fill: parent
                    columns: 2
                    columnSpacing: 22

                    Label { text: qsTr("Motion") }
                    CheckBox {
                        id: reducedMotion
                        text: qsTr("Reduced motion (static visual)")
                        checked: window.screensaverConfig.reducedMotion
                    }

                    Label { text: qsTr("Frame rate") }
                    ComboBox {
                        id: frameRate
                        Layout.fillWidth: true
                        textRole: "text"; valueRole: "value"
                        model: [
                            { text: qsTr("Match each monitor — smoothest"), value: 0 },
                            { text: qsTr("15 fps — lowest power"), value: 15 },
                            { text: qsTr("24 fps — cinematic"), value: 24 },
                            { text: qsTr("30 fps — low power"), value: 30 },
                            { text: qsTr("45 fps"), value: 45 },
                            { text: qsTr("60 fps — balanced"), value: 60 },
                            { text: qsTr("75 fps"), value: 75 },
                            { text: qsTr("90 fps"), value: 90 },
                            { text: qsTr("100 fps"), value: 100 },
                            { text: qsTr("120 fps"), value: 120 },
                            { text: qsTr("144 fps"), value: 144 },
                            { text: qsTr("165 fps"), value: 165 },
                            { text: qsTr("175 fps"), value: 175 },
                            { text: qsTr("200 fps"), value: 200 },
                            { text: qsTr("240 fps"), value: 240 }
                        ]
                        Component.onCompleted: currentIndex = indexOfValue(window.screensaverConfig.frameRate)
                        enabled: !reducedMotion.checked
                    }
                }
            }

            Label {
                Layout.fillWidth: true
                Layout.leftMargin: 22
                Layout.rightMargin: 22
                wrapMode: Text.WordWrap
                color: palette.mid
                text: qsTr("OLED tip: Pure Black, moving elements, a moving clock, seamless mode, and panel coverage minimize static pixels. Match each monitor removes timer jitter and follows every display's native refresh, while fixed caps save GPU power.")
            }

            GroupBox {
                title: qsTr("About and updates")
                Layout.fillWidth: true
                Layout.leftMargin: 22
                Layout.rightMargin: 22

                RowLayout {
                    anchors.fill: parent
                    spacing: 16

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 3

                        Label {
                            text: qsTr("Plasma Visual Screensaver %1").arg(window.controller.applicationVersion)
                            font.weight: Font.DemiBold
                        }
                        Label {
                            Layout.fillWidth: true
                            wrapMode: Text.WordWrap
                            color: palette.mid
                            text: qsTr("Updates are delivered through the system update page when the app is installed from the project's Launchpad PPA.")
                        }
                        Label {
                            id: updateStatus
                            Layout.fillWidth: true
                            wrapMode: Text.WordWrap
                            visible: text.length > 0
                        }
                    }

                    Button {
                        text: qsTr("Check for Updates")
                        onClicked: {
                            const opened = window.controller.openUpdateCenter()
                            updateStatus.color = opened ? palette.highlight : palette.brightText
                            updateStatus.text = opened
                                ? qsTr("System updates opened. Available Plasma Visual Screensaver updates will appear there.")
                                : qsTr("Could not open the software manager or release page.")
                        }
                    }
                }
            }

            RowLayout {
                Layout.fillWidth: true
                Layout.leftMargin: 22
                Layout.rightMargin: 22
                Layout.bottomMargin: 22

                Button { text: qsTr("Stop Background Process"); onClicked: window.controller.Quit() }
                Item { Layout.fillWidth: true }
                Button {
                    text: qsTr("Defaults")
                    onClicked: {
                        idleTimeout.value = 10
                        visual.currentIndex = visual.indexOfValue("aurora")
                        background.currentIndex = background.indexOfValue("midnight")
                        animationSpeed.value = 100
                        animationDensity.value = 50
                        animationScale.value = 100
                        paletteChoice.currentIndex = paletteChoice.indexOfValue("ocean")
                        trailAmount.value = 35
                        ballCount.value = 5
                        ballGravity.value = 35
                        ballElasticity.value = 92
                        ballCollisions.checked = true
                        snakeIntelligence.value = 75
                        snakeSelfCollisions.checked = false
                        snakeDeadlyWalls.checked = true
                        showClock.checked = true
                        clockMovement.currentIndex = clockMovement.indexOfValue("bounce")
                        clockSpeed.currentIndex = clockSpeed.indexOfValue("normal")
                        frameRate.currentIndex = frameRate.indexOfValue(30)
                        reducedMotion.checked = false
                        monitors.currentIndex = monitors.indexOfValue("independent")
                        coverPanels.checked = true
                    }
                }
            }
        }
    }
}
