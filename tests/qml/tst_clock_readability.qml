// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import "../../qml"

TestCase {
    name: "ClockReadability"
    width: 1280
    height: 720
    visible: true
    when: windowShown

    QtObject {
        id: sharedState
        property real clockX: 40
        property real clockY: 60
        function clockAt(timestamp) { return {x: clockX, y: clockY} }
        function setClockSize(width, height) {}
        signal frameChanged()
    }
    QtObject {
        id: presentation
        signal frameTick(real deltaSeconds)
        signal presentationTick(real timestamp)
    }
    Component {
        id: screensaverComponent
        Screensaver {
            width: 1280; height: 720
            visualModule: "none"
            backgroundStyle: "black"
            animationSpeed: 100; animationDensity: 35; animationScale: 100
            animationPalette: "ocean"; trailAmount: 75
            ballCount: 1; ballGravity: 0; ballElasticity: 100; ballCollisions: false
            snakeIntelligence: 100; snakeSelfCollisions: false; snakeDeadlyWalls: false
            developerMode: false
            showClock: false; clockMovement: "center"; clockSpeed: "normal"
            frameRate: 30; reducedMotion: false; monitorBehavior: "independent"
            seed: 1; animationEpochMs: 1000
            screenX: -640; screenY: 80
            virtualX: -640; virtualY: 0; virtualWidth: 1920; virtualHeight: 720
            animationState: sharedState
            presentationClock: presentation
            snakeSimulation: snakeTestSimulation
        }
    }
    Component {
        id: textureComponent
        Rectangle {
            width: 256; height: 256
            color: "white"
            Image { anchors.fill: parent; source: "../../qml/images/clock-scrim.png" }
        }
    }

    property var saver
    property var scrim
    property var clockBox
    function init() {
        saver = createTemporaryObject(screensaverComponent, this)
        verify(saver !== null)
        scrim = findChild(saver, "clockScrim")
        clockBox = findChild(saver, "clockBox")
        verify(scrim !== null)
        verify(clockBox !== null)
    }
    function verifyPlacement() {
        fuzzyCompare(scrim.width, clockBox.width * 2, 0.01)
        fuzzyCompare(scrim.height, clockBox.height * 3.2, 0.01)
        fuzzyCompare(scrim.x + scrim.width / 2, clockBox.x + clockBox.width / 2, 0.01)
        fuzzyCompare(scrim.y + scrim.height / 2, clockBox.y + clockBox.height / 2, 0.01)
        verify(saver.children.indexOf(scrim) < saver.children.indexOf(clockBox),
               "Scrim must draw below clock text")
    }
    function test_visibilityAndTexture_data() {
        return [{tag: "none", visual: "none"}, {tag: "snakes", visual: "snakes"},
                {tag: "orbs", visual: "orbs"}]
    }
    function test_visibilityAndTexture(data) {
        saver.visualModule = data.visual
        verify(!scrim.visible)
        saver.showClock = true
        verify(scrim.visible)
        tryCompare(scrim, "status", Image.Ready)
        compare(scrim.sourceSize.width, 256)
        compare(scrim.sourceSize.height, 256)
        verifyPlacement()
        saver.showClock = false
        verify(!scrim.visible)
    }
    function test_tracksClock_data() {
        return [{tag: "center", movement: "center", behavior: "independent"},
                {tag: "independent", movement: "bounce", behavior: "independent"},
                {tag: "synchronized", movement: "bounce", behavior: "synchronized"},
                {tag: "seamless", movement: "bounce", behavior: "seamless"}]
    }
    function test_tracksClock(data) {
        saver.showClock = true
        saver.monitorBehavior = data.behavior
        saver.clockMovement = data.movement
        saver.clockMotionNowMs = 4000
        saver.sharedClockX = -120
        saver.sharedClockY = 200
        verifyPlacement()
        const beforeX = scrim.x
        const beforeY = scrim.y
        saver.clockMotionNowMs += 5000
        saver.sharedClockX += 130
        saver.sharedClockY += 70
        verifyPlacement()
        if (data.movement === "bounce") {
            verify(scrim.x !== beforeX)
            verify(scrim.y !== beforeY)
        }
        saver.reducedMotion = true
        verifyPlacement()
        fuzzyCompare(scrim.x + scrim.width / 2, saver.width / 2, 0.01)
        fuzzyCompare(scrim.y + scrim.height / 2, saver.height / 2, 0.01)
        saver.width = 1000
        saver.height = 600
        verifyPlacement()
    }
    function test_radialAlpha() {
        const texture = createTemporaryObject(textureComponent, this)
        verify(texture !== null)
        tryCompare(texture.children[0], "status", Image.Ready)
        const image = grabImage(texture)
        // Black alpha 0.52 over white rounds to approximately 122 / 255.
        verify(Math.abs(image.red(128, 128) - 122) <= 1)
        compare(image.red(128, 0), 255)
        compare(image.red(0, 0), 255)
        verify(image.red(128, 64) > image.red(128, 128))
        verify(image.red(128, 64) < image.red(128, 0))
    }
    function test_reducedMotionPropagatesAfterLoadAndReload() {
        saver.visualModule = "snakes"
        const visual = findChild(saver, "snakeVisualRoot")
        verify(visual !== null)
        compare(visual.reducedMotion, false)
        const renderer = findChild(visual, "snakeNativeRenderer")
        compare(renderer.shaderTimeFrozen, false)
        saver.reducedMotion = true
        compare(visual.reducedMotion, true)
        compare(renderer.shaderTimeFrozen, true)
        saver.reducedMotion = false
        compare(renderer.shaderTimeFrozen, false)
        saver.visualModule = "none"
        saver.reducedMotion = true
        saver.visualModule = "snakes"
        const reloaded = findChild(saver, "snakeVisualRoot")
        verify(reloaded !== null)
        compare(reloaded.reducedMotion, true)
        compare(findChild(reloaded, "snakeNativeRenderer").shaderTimeFrozen, true)
    }
}
