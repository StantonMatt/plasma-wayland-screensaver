// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import "../../qml/visuals"

TestCase {
    name: "PerWindowPresentationCadence"
    width: 320
    height: 180
    visible: true
    when: windowShown

    QtObject {
        id: state
        property var balls: []
        property double lastSample: 0
        property rect bounds: Qt.rect(0, 0, 320, 180)
        function ballsAt(timestamp) { lastSample = timestamp; return balls }
        signal frameChanged()
    }
    QtObject { id: firstClock; signal frameTick(real deltaSeconds); signal presentationTick(real timestamp) }
    QtObject { id: secondClock; signal frameTick(real deltaSeconds); signal presentationTick(real timestamp) }
    QtObject {
        id: firstContext
        property string monitorBehavior: "seamless"
        property var animationState: state
        property var presentationClock: firstClock
        property string animationPalette: "ocean"
        property int ballCount: 1
        property int animationScale: 100
        property int animationSpeed: 100
        property int ballGravity: 0
        property int ballElasticity: 100
        property bool ballCollisions: false
        property int trailAmount: 0
        property real screenX: 0
        property real screenY: 0
    }
    QtObject {
        id: secondContext
        property string monitorBehavior: "seamless"
        property var animationState: state
        property var presentationClock: secondClock
        property string animationPalette: "ocean"
        property real screenX: 0
        property real screenY: 0
        property int trailAmount: 0
    }
    Bounce { id: first; anchors.fill: parent; context: firstContext }
    Bounce { id: second; anchors.fill: parent; context: secondContext }

    SignalSpy { id: paintSpy; signalName: "painted" }

    function findCanvas(item) {
        if (item instanceof Canvas) return item
        for (let i = 0; i < item.children.length; ++i) {
            const canvas = findCanvas(item.children[i])
            if (canvas) return canvas
        }
        return null
    }

    function test_pausedIndependentInputsRepaint() {
        firstContext.monitorBehavior = "independent"
        first.reducedMotion = true
        paintSpy.target = findCanvas(first)
        paintSpy.target.requestPaint()
        tryVerify(function() { return paintSpy.count > 0 })
        grabImage(first)
        paintSpy.clear()
        firstContext.ballCount = 2
        compare(first.localBalls.length, 2)
        tryVerify(function() { return paintSpy.count > 0 })
        const size = first.localBalls[0].size
        grabImage(first)
        paintSpy.clear()
        firstContext.animationScale = 200
        verify(first.localBalls[0].size > size)
        tryVerify(function() { return paintSpy.count > 0 })
        const before = grabImage(first)
        paintSpy.clear()
        firstContext.animationPalette = "ember"
        tryVerify(function() { return paintSpy.count > 0 })
        tryVerify(function() { return !grabImage(first).equals(before) })
    }

    function test_pausedSynchronizedSnapshotRepaints_data() {
        return [{tag: "balls", changeBounds: false}, {tag: "bounds", changeBounds: true}]
    }
    function test_pausedSynchronizedSnapshotRepaints(data) {
        firstContext.monitorBehavior = "synchronized"
        first.reducedMotion = true
        paintSpy.target = findCanvas(first)
        verify(paintSpy.valid)
        state.balls = [ball(50)]
        state.frameChanged()
        // Establish the initial image without relying on the buggy snapshot
        // invalidation path, then change shared state with both clocks stopped.
        paintSpy.target.requestPaint()
        tryVerify(function() { return paintSpy.count > 0 })
        const before = grabImage(first)
        const background = pixelCode(before, 300, 10)
        verify(pixelCode(before, 65, 105) !== background)
        paintSpy.clear()
        if (data.changeBounds) state.bounds = Qt.rect(0, 0, 160, 180)
        else state.balls = [ball(200)]
        state.frameChanged()
        compare(first.sharedBalls[0].x, data.changeBounds ? 50 : 200)
        tryVerify(function() { return paintSpy.count > 0 })
        // Canvas.painted precedes texture upload; wait for visible pixels too.
        tryVerify(function() { return pixelCode(grabImage(first), 65, 105) === background })
        const after = grabImage(first)
        compare(pixelCode(after, 65, 105), background)
        verify(pixelCode(after, data.changeBounds ? 130 : 215, 105) !== background)
        paintSpy.target = null
    }

    function pixelCode(image, x, y) {
        return image.red(x, y) * 65536 + image.green(x, y) * 256 + image.blue(x, y)
    }

    function ball(x) {
        return {x: x, y: 90, vx: 10, vy: 0, size: 30, colorIndex: 0, trail: []}
    }
    function init() {
        paintSpy.target = null
        paintSpy.clear()
        firstContext.monitorBehavior = "seamless"
        firstContext.ballCount = 1
        firstContext.animationScale = 100
        firstContext.animationPalette = "ocean"
        first.reducedMotion = false
        second.reducedMotion = false
        state.balls = []
        state.bounds = Qt.rect(0, 0, 320, 180)
        firstClock.frameTick(0.02)
        secondClock.frameTick(0.02)
    }
    function test_sharedChangesWaitForEachWindowsTick() {
        state.balls = [ball(100)]
        state.frameChanged()
        compare(first.sharedBalls.length, 0)
        compare(second.sharedBalls.length, 0)
        firstClock.frameTick(0.02)
        compare(first.sharedBalls[0].x, 100)
        compare(second.sharedBalls.length, 0)
        state.balls = [ball(101)]
        secondClock.frameTick(0.017)
        compare(second.sharedBalls[0].x, 101)
        compare(first.sharedBalls[0].x, 100)
    }
    function test_reducedMotionStillShowsSharedBalls() {
        first.reducedMotion = true
        state.balls = [ball(120)]
        state.frameChanged()
        compare(first.sharedBalls[0].x, 120)
    }
    function test_independentPhysicsUsesFixedSteps() {
        firstContext.monitorBehavior = "independent"
        firstContext.ballGravity = 100
        firstContext.ballElasticity = 65
        first.physicsAccumulator = 0
        first.localBalls = [ball(100)]
        for (let i = 0; i < 60; ++i) firstClock.frameTick(1 / 60)
        const expected = JSON.stringify(first.localBalls)
        first.physicsAccumulator = 0
        first.localBalls = [ball(100)]
        for (let i = 0; i < 48; ++i) firstClock.frameTick(1 / 48)
        compare(JSON.stringify(first.localBalls), expected)
        firstContext.ballGravity = 0
        firstContext.ballElasticity = 100
    }
    function test_synchronizedUsesSharedPhysics() {
        firstContext.monitorBehavior = "synchronized"
        state.balls = [ball(100)]
        firstClock.presentationTick(123456789)
        firstClock.frameTick(1 / 60)
        compare(state.lastSample, 123456789)
        compare(first.sharedBalls.length, 1)
        compare(first.sharedBalls[0].x, 100)
    }
}
