// SPDX-License-Identifier: GPL-3.0-or-later
pragma ComponentBehavior: Bound
import QtQuick
import "VisualUtils.js" as Utils

Item {
    id: root
    clip: true

    property var context
    property int frameRate: 30
    property bool reducedMotion: false
    property int seed: 1
    property double animationEpochMs: Date.now()
    property var localBalls: []
    property var sharedBalls: []
    property real physicsAccumulator: 0
    property real priorWidth: 0
    property real priorHeight: 0

    readonly property bool seamless: context && context.monitorBehavior === "seamless"
    readonly property bool sharedMotion: context && context.monitorBehavior !== "independent"
    readonly property var sharedState: context ? context.animationState : null
    readonly property var palette: Utils.colors(context ? context.animationPalette : "ocean")

    function sampleSharedBalls(presentationNanoseconds) {
        root.sharedBalls = root.sharedMotion && root.sharedState
            ? (presentationNanoseconds ? root.sharedState.ballsAt(presentationNanoseconds)
                                       : root.sharedState.balls) : []
    }
    onContextChanged: initializeBalls()
    onSharedStateChanged: sampleSharedBalls()
    onSharedBallsChanged: if (!seamless) localCanvas.requestPaint()
    onLocalBallsChanged: if (!seamless) localCanvas.requestPaint()
    onSeamlessChanged: if (!seamless) localCanvas.requestPaint()
    onSeedChanged: initializeBalls()
    onSharedMotionChanged: { sampleSharedBalls(); initializeBalls() }
    onReducedMotionChanged: { physicsAccumulator = 0; if (reducedMotion) sampleSharedBalls() }

    Connections {
        target: root.sharedState
        enabled: root.reducedMotion
        function onFrameChanged() {
            root.sampleSharedBalls()
            // Bounds can change while the ball snapshot remains identical.
            if (!root.seamless) localCanvas.requestPaint()
        }
    }

    Connections {
        target: root.context
        ignoreUnknownSignals: true
        function onBallCountChanged() { root.initializeBalls() }
        function onAnimationScaleChanged() { root.initializeBalls() }
    }

    function initializeBalls() {
        if (!context || width <= 0 || height <= 0 || sharedMotion)
            return
        const count = context.ballCount
        const baseSize = Math.max(24, Math.min(220, Math.min(width, height) * 0.075
                                               * context.animationScale / 100))
        const items = []
        for (let i = 0; i < count; ++i) {
            const variation = 0.78 + ((i * 37) % 45) / 100
            const size = baseSize * variation
            const x = Utils.random(i * 5 + 1, seed) * Math.max(1, width - size)
            const y = Utils.random(i * 5 + 2, seed) * Math.max(1, height - size)
            const dx = (i % 2 ? -1 : 1) * (118 + (i * 47) % 125)
            const dy = (i % 3 ? 1 : -1) * (86 + (i * 61) % 110)
            items.push({ x: x, y: y, vx: dx, vy: dy, size: size, trail: [] })
        }
        localBalls = items
        physicsAccumulator = 0
        priorWidth = width
        priorHeight = height
    }

    function stepPhysics(seconds) {
        if (!context || localBalls.length === 0)
            return
        const dt = Math.min(0.05, seconds) * context.animationSpeed / 100
        const gravity = context.ballGravity * 8
        const elasticity = context.ballElasticity / 100
        const trailSteps = Math.round(context.trailAmount / 12)
        for (let i = 0; i < localBalls.length; ++i) {
            const ball = localBalls[i]
            ball.vy += gravity * dt
            if (trailSteps > 0) {
                ball.trail.unshift({ x: ball.x, y: ball.y })
                if (ball.trail.length > trailSteps)
                    ball.trail.pop()
            } else {
                ball.trail = []
            }
            ball.x += ball.vx * dt
            ball.y += ball.vy * dt
            if (ball.x < 0) { ball.x = 0; ball.vx = Math.abs(ball.vx) * elasticity }
            if (ball.x + ball.size > width) {
                ball.x = Math.max(0, width - ball.size)
                ball.vx = -Math.abs(ball.vx) * elasticity
            }
            if (ball.y < 0) { ball.y = 0; ball.vy = Math.abs(ball.vy) * elasticity }
            if (ball.y + ball.size > height) {
                ball.y = Math.max(0, height - ball.size)
                ball.vy = -Math.abs(ball.vy) * elasticity
                if (Math.abs(ball.vy) < 28 && Math.abs(gravity) > 1)
                    ball.vy = gravity > 0 ? -72 : 72
            }
        }
        if (context.ballCollisions)
            resolveCollisions()
    }

    function resolveCollisions() {
        for (let i = 0; i < localBalls.length; ++i) {
            for (let j = i + 1; j < localBalls.length; ++j) {
                const a = localBalls[i]
                const b = localBalls[j]
                const dx = (b.x + b.size / 2) - (a.x + a.size / 2)
                const dy = (b.y + b.size / 2) - (a.y + a.size / 2)
                const distance = Math.sqrt(dx * dx + dy * dy)
                const minimum = (a.size + b.size) / 2
                if (distance <= 0.001 || distance >= minimum)
                    continue
                const nx = dx / distance
                const ny = dy / distance
                const relative = (b.vx - a.vx) * nx + (b.vy - a.vy) * ny
                if (relative < 0) {
                    const impulse = -(1 + context.ballElasticity / 100) * relative / 2
                    a.vx -= nx * impulse; a.vy -= ny * impulse
                    b.vx += nx * impulse; b.vy += ny * impulse
                }
                const correction = (minimum - distance) / 2
                a.x -= nx * correction; a.y -= ny * correction
                b.x += nx * correction; b.y += ny * correction
                a.x = Math.max(0, Math.min(width - a.size, a.x))
                a.y = Math.max(0, Math.min(height - a.size, a.y))
                b.x = Math.max(0, Math.min(width - b.size, b.x))
                b.y = Math.max(0, Math.min(height - b.size, b.y))
            }
        }
    }

    VisualCanvas {
        id: localCanvas
        anchors.fill: parent
        visible: !root.seamless
        context: root.context
        seed: root.seed
        reducedMotion: root.reducedMotion
        onPaint: {
            const painter = getContext("2d")
            painter.reset()
            painter.clearRect(0, 0, width, height)
            if (root.sharedMotion) {
                painter.scale(root.width / Math.max(1, root.context.animationState.bounds.width),
                              root.height / Math.max(1, root.context.animationState.bounds.height))
            }
            const balls = root.sharedMotion ? root.sharedBalls : root.localBalls
            for (let i = 0; i < balls.length; ++i) {
                const ball = balls[i]
                const color = root.palette[i % root.palette.length]
                const trailLength = ball.trail ? ball.trail.length : Math.round(root.context.trailAmount / 12)
                const velocityLength = Math.max(1, Math.sqrt(ball.vx * ball.vx + ball.vy * ball.vy))
                for (let t = trailLength - 1; t >= 0; --t) {
                    const trailX = ball.trail ? ball.trail[t].x
                        : ball.x - ball.vx / velocityLength * (t + 1) * ball.size * 0.28
                    const trailY = ball.trail ? ball.trail[t].y
                        : ball.y - ball.vy / velocityLength * (t + 1) * ball.size * 0.28
                    painter.globalAlpha = (1 - t / Math.max(1, trailLength)) * 0.16
                    painter.fillStyle = color
                    painter.beginPath()
                    painter.arc(trailX + ball.size / 2, trailY + ball.size / 2,
                                ball.size * 0.42, 0, Math.PI * 2)
                    painter.fill()
                }
                painter.globalAlpha = 1
                const gradient = painter.createRadialGradient(ball.x + ball.size * 0.35,
                                                               ball.y + ball.size * 0.30, 1,
                                                               ball.x + ball.size / 2,
                                                               ball.y + ball.size / 2, ball.size * 0.56)
                gradient.addColorStop(0, "#ffffff")
                gradient.addColorStop(0.22, color)
                gradient.addColorStop(1, "#131b45")
                painter.fillStyle = gradient
                painter.beginPath()
                painter.arc(ball.x + ball.size / 2, ball.y + ball.size / 2,
                            ball.size / 2, 0, Math.PI * 2)
                painter.fill()
            }
        }
    }

    Item {
        visible: root.seamless
        Repeater {
            model: root.seamless ? root.sharedBalls : []
            Rectangle {
                id: seamlessBall
                required property var modelData
                readonly property real velocityLength: Math.max(1, Math.sqrt(modelData.vx * modelData.vx
                                                                             + modelData.vy * modelData.vy))
                x: modelData.x - (root.seamless ? root.context.screenX : 0)
                y: modelData.y - (root.seamless ? root.context.screenY : 0)
                width: modelData.size
                height: width
                radius: width / 2
                border.width: Math.max(1, width * 0.025)
                border.color: "#80ffffff"
                color: root.palette[modelData.colorIndex % root.palette.length]

                Repeater {
                    model: root.context ? Math.round(root.context.trailAmount / 20) : 0
                    Rectangle {
                        required property int index
                        z: -1
                        readonly property real distance: (index + 1) * seamlessBall.width * 0.28
                        x: -seamlessBall.modelData.vx / seamlessBall.velocityLength * distance
                        y: -seamlessBall.modelData.vy / seamlessBall.velocityLength * distance
                        width: seamlessBall.width * (0.72 - index * 0.08)
                        height: width
                        radius: width / 2
                        color: seamlessBall.color
                        opacity: Math.max(0.04, 0.22 - index * 0.035)
                    }
                }
            }
        }
    }

    FrameClock {
        presentationClock: root.context ? root.context.presentationClock : null
        running: !root.reducedMotion
        onTick: function(deltaSeconds, presentationNanoseconds) {
            if (root.sharedMotion) {
                root.sampleSharedBalls(presentationNanoseconds)
                return
            }
            root.physicsAccumulator += Math.min(0.1, deltaSeconds)
            while (root.physicsAccumulator + 1e-10 >= 1 / 60) {
                root.stepPhysics(1 / 60)
                root.physicsAccumulator -= 1 / 60
            }
            localCanvas.requestPaint()
        }
    }

    onWidthChanged: if (Math.abs(width - priorWidth) > 1) initializeBalls()
    onHeightChanged: if (Math.abs(height - priorHeight) > 1) initializeBalls()
    Component.onCompleted: initializeBalls()
}
