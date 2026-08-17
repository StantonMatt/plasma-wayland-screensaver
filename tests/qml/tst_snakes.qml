// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import "../../qml/visuals"

TestCase {
    id: testCase
    name: "SlitheringSnakes"
    width: 1280
    height: 720
    visible: true
    when: windowShown

    QtObject {
        id: visualContext
        property int animationSpeed: 100
        property int animationDensity: 50
        property int animationScale: 100
        property string animationPalette: "spectrum"
        property int trailAmount: 35
        property int snakeIntelligence: 75
        property bool snakeSelfCollisions: false
        property bool snakeDeadlyWalls: true
        property bool developerMode: false
        property string monitorBehavior: "independent"
        property real virtualX: 0
        property real virtualY: 0
        property real virtualWidth: 1280
        property real virtualHeight: 720
        property real screenX: 0
        property real screenY: 0
        property var presentationClock: null
    }

    Snakes {
        id: visual
        anchors.fill: parent
        context: visualContext
        reducedMotion: true
        seed: 73
    }

    function init() {
        visual.nativeRenderer = null
        visual.simulationDriver = true
        visualContext.animationSpeed = 100
        visualContext.animationDensity = 50
        visualContext.animationScale = 100
        visualContext.trailAmount = 35
        visualContext.monitorBehavior = "independent"
        visualContext.virtualX = 0
        visualContext.virtualWidth = 1280
        visualContext.screenX = 0
        visualContext.snakeIntelligence = 75
        visualContext.snakeSelfCollisions = false
        visualContext.snakeDeadlyWalls = true
        visual.initializeWorld()
    }

    function test_nativeRendererUnloadsCanvasFallback() {
        tryCompare(visual, "canvasFallbackLoaded", true)
        visual.nativeRenderer = {
            syncFrame: function() {},
            presentFrame: function() {}
        }
        tryCompare(visual, "canvasFallbackLoaded", false)

        visual.nativeRenderer = null
        tryCompare(visual, "canvasFallbackLoaded", true)
    }

    function test_advanceInterpolatesThirtyHzPhysicsAtSixtyFps() {
        visual.nativeRenderer = {
            syncFrame: function() {},
            presentFrame: function() {}
        }
        visual.accumulator = 0
        visual.simulationTime = 0

        visual.advance(1 / 60)
        compare(visual.simulationTime, 0)
        verify(Math.abs(visual.renderAlpha - 0.5) < 0.001)

        visual.advance(1 / 60)
        verify(Math.abs(visual.simulationTime - 1 / 30) < 0.001)
        verify(visual.renderAlpha < 0.001)
    }

    function test_canvasFallbackRetainsSixtyHzPhysics() {
        visual.nativeRenderer = null
        visual.accumulator = 0
        visual.simulationTime = 0

        visual.advance(1 / 60)

        verify(Math.abs(visual.simulationTime - 1 / 60) < 0.001)
        verify(visual.renderAlpha < 0.001)
    }

    function test_canvasFallbackKeepsTimeAtFifteenFps() {
        visual.nativeRenderer = null
        visual.accumulator = 0
        visual.simulationTime = 0

        visual.advance(1 / 15)

        verify(Math.abs(visual.simulationTime - 1 / 15) < 0.001)
        verify(visual.renderAlpha < 0.001)
    }

    function test_segmentMotionUsesPhysicsStepDuration() {
        visual.nativeRenderer = {
            syncFrame: function() {},
            presentFrame: function() {}
        }
        const motion = visual.segmentMotion({
            x: 14,
            y: 17,
            previousX: 4,
            previousY: 7
        }, 1000)

        verify(Math.abs(motion.x - 300) < 0.001)
        verify(Math.abs(motion.y - 300) < 0.001)
    }

    function test_initialPopulation() {
        compare(visual.snakes.length, 9)
        compare(visual.food.length, 82)
        for (let index = 0; index < visual.snakes.length; ++index) {
            verify(visual.snakes[index].alive)
            verify(visual.snakes[index].segments.length >= 14)
        }
    }

    function test_seamlessHighDensityPopulationRemainsFinite() {
        visualContext.monitorBehavior = "seamless"
        visualContext.virtualWidth = 8560
        visualContext.virtualHeight = 1635
        visualContext.animationDensity = 100
        visualContext.animationScale = 25
        visualContext.trailAmount = 100
        visualContext.snakeIntelligence = 100
        visualContext.snakeSelfCollisions = true
        visualContext.snakeDeadlyWalls = true
        visual.initializeWorld()

        let minimumAlive = visual.snakes.length
        let maximumSegments = 0
        let maximumFood = visual.food.length
        let longestEmptyRun = 0
        let emptyRun = 0
        for (let frame = 0; frame < 3600; ++frame) {
            visual.stepSimulation(1 / 30)
            let alive = 0
            let segments = 0
            for (let snakeIndex = 0; snakeIndex < visual.snakes.length; ++snakeIndex) {
                const snake = visual.snakes[snakeIndex]
                if (!snake.alive)
                    continue
                ++alive
                segments += snake.segments.length
                for (let segmentIndex = 0; segmentIndex < snake.segments.length;
                        ++segmentIndex) {
                    const segment = snake.segments[segmentIndex]
                    verify(Number.isFinite(segment.x) && Number.isFinite(segment.y),
                           "non-finite segment at frame " + frame)
                }
            }
            if (frame % 30 === 0) {
                for (let foodIndex = 0; foodIndex < visual.food.length; ++foodIndex) {
                    const particle = visual.food[foodIndex]
                    verify(Number.isFinite(particle.x)
                           && Number.isFinite(particle.y)
                           && Number.isFinite(particle.size),
                           "non-finite food particle at frame " + frame)
                }
            }
            minimumAlive = Math.min(minimumAlive, alive)
            maximumSegments = Math.max(maximumSegments, segments)
            maximumFood = Math.max(maximumFood, visual.food.length)
            emptyRun = alive === 0 ? emptyRun + 1 : 0
            longestEmptyRun = Math.max(longestEmptyRun, emptyRun)
        }
        console.log("seamless soak minAlive=" + minimumAlive
                    + " maxSegments=" + maximumSegments
                    + " maxFood=" + maximumFood
                    + " longestEmptyRun=" + longestEmptyRun
                    + " deaths=" + visual.deathCount)
        verify(longestEmptyRun === 0, "the whole snake population disappeared")
    }

    function test_ambientFoodRelocatesAfterAboutFortySeconds() {
        visual.food = []
        visual.addAmbientFood()
        const expiredParticle = visual.food[0]
        verify(expiredParticle.life >= 34)
        verify(expiredParticle.life <= 46)
        expiredParticle.life = 0.001

        visual.updateFood(0.01)

        verify(visual.food.indexOf(expiredParticle) === -1)
        verify(visual.food.length > 0)
        verify(visual.food[0].life >= 34)
        verify(visual.food[0].life <= 46)
    }

    function test_spawnKeepsClearOfLivingSnakes() {
        for (let index = 0; index < visual.snakes.length; ++index) {
            const snake = visual.snakes[index]
            const head = snake.segments[0]
            let closest = Number.MAX_VALUE
            for (let segmentIndex = 0; segmentIndex < snake.segments.length; ++segmentIndex) {
                const segment = snake.segments[segmentIndex]
                verify(segment.x > 0 && segment.x < visual.worldWidth)
                verify(segment.y > 0 && segment.y < visual.worldHeight)
            }
            for (let otherIndex = 0; otherIndex < index; ++otherIndex) {
                const other = visual.snakes[otherIndex]
                for (let segmentIndex = 0; segmentIndex < other.segments.length;
                        ++segmentIndex) {
                    closest = Math.min(closest, Math.sqrt(visual.worldDistanceSquared(
                        head.x, head.y, other.segments[segmentIndex].x,
                        other.segments[segmentIndex].y)))
                }
            }
            if (index > 0) {
                verify(closest > snake.radius * 6,
                       "snake " + index + " spawned only " + closest + " px away")
            }
        }
    }

    function test_foodGrowsSnake() {
        const snake = visual.snakes[0]
        const head = snake.segments[0]
        const previousLength = snake.segments.length
        const previousRadius = snake.radius
        visual.food = [{
            x: head.x,
            y: head.y,
            value: 2.2,
            colorIndex: 0,
            size: 4,
            vx: 0,
            vy: 0,
            life: -1,
            phase: 0
        }]

        visual.feedSnake(snake)

        compare(visual.food.length, 0)
        compare(snake.segments.length, previousLength)
        for (let frame = 0; frame < 30; ++frame)
            visual.moveSnake(snake, 0, 1 / 60)
        compare(snake.segments.length, previousLength + 2)
        verify(snake.radius > previousRadius)
        const tail = snake.segments[snake.segments.length - 1]
        const beforeTail = snake.segments[snake.segments.length - 2]
        const tailSpacing = Math.sqrt(visual.distanceSquared(
            tail.x, tail.y, beforeTail.x, beforeTail.y))
        verify(tailSpacing > snake.radius * 0.9)
    }

    function test_foodVacuumExtendsOneHeadWidth() {
        const snake = visual.snakes[0]
        const head = snake.segments[0]
        visual.food = [{
            x: head.x + snake.radius * 2.75,
            y: head.y,
            value: 1,
            colorIndex: 0,
            size: 1,
            vx: 0,
            vy: 0,
            life: -1,
            phase: 0
        }]

        const initialX = visual.food[0].x
        visual.feedSnake(snake, 1 / 60)

        compare(visual.food.length, 1)
        verify(visual.food[0].x < initialX)
        verify(visual.food[0].attraction > 0)
        for (let frame = 0; frame < 30 && visual.food.length > 0; ++frame)
            visual.feedSnake(snake, 1 / 60)
        compare(visual.food.length, 0)
    }

    function test_vacuumLockFinishesAfterSnakeTurnsAway() {
        const snake = visual.snakes[0]
        const head = snake.segments[0]
        visual.food = []
        visual.addFood(head.x + snake.radius * 2.7, head.y,
                       1, 0, 0, 0, -1)
        const particle = visual.food[0]

        visual.feedSnake(snake, 1 / 60)
        compare(particle.vacuumOwner, 0)

        // Move the head abruptly outside the original capture radius, then
        // keep it travelling away. A locked particle must continue chasing it.
        head.x += 150
        head.y += 90
        for (let frame = 0; frame < 180 && visual.food.length > 0; ++frame) {
            head.x += 1
            visual.feedSnake(snake, 1 / 60)
        }
        verify(visual.food.length === 0,
               "locked particle remained at (" + particle.x + ", " + particle.y
               + ") while head reached (" + head.x + ", " + head.y + ") owner="
               + particle.vacuumOwner)
    }

    function test_vacuumReleaseRestoresParticleExpiryAfterOwnerDies() {
        const snake = visual.snakes[0]
        const head = snake.segments[0]
        visual.snakes = [snake]
        visual.food = []
        visual.addFood(head.x + snake.radius * 2.7, head.y,
                       1, 0, 0, 0, 37)
        const particle = visual.food[0]

        visual.feedSnake(snake, 1 / 60)
        compare(particle.vacuumOwner, 0)
        compare(particle.life, -1)
        snake.alive = false

        visual.feedSnakes(1 / 60)

        compare(particle.vacuumOwner, -1)
        compare(particle.life, 37)
    }

    function test_growthIsInsertedBehindHeadNotAppendedAtTail() {
        const snake = visual.snakes[0]
        snake.brainCooldown = 10
        snake.growth = 1.1
        const oldNeck = snake.segments[1]
        const oldTail = snake.segments[snake.segments.length - 1]
        const previousLength = snake.segments.length

        for (let frame = 0; frame < 90 && snake.segments.length === previousLength; ++frame)
            visual.moveSnake(snake, 0, 1 / 60)

        compare(snake.segments.length, previousLength + 1)
        verify(snake.segments[2] === oldNeck)
        verify(snake.segments[snake.segments.length - 1] === oldTail)
    }

    function test_bodyReplaysHeadPathWithoutSidewaysDrift() {
        visualContext.snakeDeadlyWalls = true
        visual.snakes = [visual.makeSnake(0)]
        const snake = visual.snakes[0]
        const spacing = snake.radius * 1.18
        for (let index = 0; index < snake.segments.length; ++index) {
            snake.segments[index].x = 500 - spacing * index
            snake.segments[index].y = 320
            snake.segments[index].previousX = snake.segments[index].x
            snake.segments[index].previousY = snake.segments[index].y
        }
        snake.angle = 0
        snake.desiredAngle = 0
        snake.brainCooldown = 10
        visual.rebuildSnakeTrail(snake)

        for (let frame = 0; frame < 8; ++frame)
            visual.moveSnake(snake, 0, 1 / 60)
        const cornerX = snake.segments[0].x
        snake.angle = Math.PI / 2
        snake.desiredAngle = Math.PI / 2
        for (let frame = 0; frame < 12; ++frame)
            visual.moveSnake(snake, 0, 1 / 60)

        // Each point remains on one leg of the exact L-shaped head trail.
        // A neighbour constraint instead creates diagonal points here and
        // visibly strafes the still-horizontal tail toward the new heading.
        for (let index = 1; index < snake.segments.length; ++index) {
            const segment = snake.segments[index]
            verify(Math.abs(segment.x - cornerX) < 0.02
                   || Math.abs(segment.y - 320) < 0.02,
                   "segment " + index + " cut across the head's corner")
        }
        fuzzyCompare(snake.segments[snake.segments.length - 1].y, 320, 0.02)
    }

    function test_highIntelligenceTurnsAwayFromDeadlyWall() {
        visualContext.snakeIntelligence = 100
        visualContext.snakeDeadlyWalls = true
        visual.snakes = [visual.makeSnake(0)]
        visual.food = []
        visual.cancelActiveBrainPlan()
        visual.brainCursor = 0
        const snake = visual.snakes[0]
        const spacing = snake.radius * 1.18
        for (let index = 0; index < snake.segments.length; ++index) {
            snake.segments[index].x = 95 + spacing * index
            snake.segments[index].y = visual.worldHeight / 2
            snake.segments[index].previousX = snake.segments[index].x
            snake.segments[index].previousY = snake.segments[index].y
        }
        snake.angle = Math.PI
        snake.desiredAngle = Math.PI
        snake.brainCooldown = 0
        visual.rebuildSnakeTrail(snake)

        for (let frame = 0; frame < 120 && snake.alive; ++frame)
            visual.stepSimulation(1 / 30)

        verify(snake.alive, "intelligent snake drove through an unobstructed wall; x="
               + (snake.segments.length ? snake.segments[0].x : "exploded")
               + " angle=" + snake.angle + " desired=" + snake.desiredAngle
               + " risk=" + snake.lastPlanRisk
               + " collision=" + snake.lastPlanCollision)
        verify(snake.segments[0].x > snake.radius,
               "intelligent snake failed to preserve wall clearance")
    }

    function test_highIntelligenceAvoidsLiveHeadOnCollision() {
        visualContext.snakeIntelligence = 100
        visualContext.snakeDeadlyWalls = true
        const left = visual.makeSnake(0)
        const right = visual.makeSnake(1)
        const centerY = visual.worldHeight / 2

        function positionSnake(snake, headX, angle) {
            const spacing = snake.radius * 1.18
            for (let index = 0; index < snake.segments.length; ++index) {
                snake.segments[index].x = headX - Math.cos(angle) * spacing * index
                snake.segments[index].y = centerY
                snake.segments[index].previousX = snake.segments[index].x
                snake.segments[index].previousY = snake.segments[index].y
            }
            snake.angle = angle
            snake.desiredAngle = angle
            snake.brainCooldown = 0
            visual.rebuildSnakeTrail(snake)
        }

        positionSnake(left, 390, 0)
        positionSnake(right, 890, Math.PI)
        visual.snakes = [left, right]
        visual.food = []
        visual.cancelActiveBrainPlan()
        visual.brainCursor = 0

        for (let frame = 0; frame < 150 && left.alive && right.alive; ++frame)
            visual.stepSimulation(1 / 30)

        verify(left.alive && right.alive,
               "intelligent snakes failed to resolve a visible head-on approach")
    }

    function test_adaptiveLengthBudgetAllowsLongChampions() {
        const snake = visual.snakes[0]
        const maximum = visual.maximumSnakeSegments(snake)
        const paintedCoverage = maximum * snake.radius * snake.radius * 2.35
                                / (visual.worldWidth * visual.worldHeight)

        verify(maximum > 400)
        verify(maximum <= 1600)
        verify(paintedCoverage <= 0.076)
        verify(visual.maximumWorldSegments() > maximum)
        verify(visual.maximumWorldSegments() <= 6000)
    }

    function test_snakeCanGrowPastFormerLimit() {
        visualContext.snakeDeadlyWalls = false
        visual.snakes = [visual.makeSnake(0)]
        const snake = visual.snakes[0]
        while (snake.segments.length < 125)
            visual.appendGrowthSegment(snake)
        const previousLength = snake.segments.length
        const cost = visual.segmentGrowthCost(snake)
        verify(cost > 1)
        snake.growth = cost + 0.1
        snake.brainCooldown = 10
        visual.growthSlots = 100

        for (let frame = 0; frame < 90 && snake.segments.length === previousLength; ++frame)
            visual.moveSnake(snake, 0, 1 / 60)

        compare(snake.segments.length, previousLength + 1)
    }

    function test_individualAreaCapStopsScreenFilling() {
        visualContext.snakeDeadlyWalls = false
        visual.snakes = [visual.makeSnake(0)]
        const snake = visual.snakes[0]
        while (snake.segments.length < visual.maximumSnakeSegments(snake))
            visual.appendGrowthSegment(snake)
        const cappedLength = snake.segments.length
        snake.growth = 10000
        snake.brainCooldown = 10
        visual.growthSlots = 100

        for (let frame = 0; frame < 120; ++frame)
            visual.moveSnake(snake, 0, 1 / 60)

        compare(snake.segments.length, cappedLength)
        verify(snake.growthBlocked)
    }

    function test_snakeChoosesOnlyNearestReachableFood() {
        visualContext.snakeIntelligence = 100
        const snake = visual.snakes[0]
        const head = snake.segments[0]
        snake.angle = 0
        visual.snakes = [snake]
        visual.food = []
        visual.addFood(head.x + 80, head.y, 1, 0, 0, 0, -1)
        for (let index = 0; index < 9; ++index) {
            visual.addFood(head.x + 220 + (index % 3) * 8,
                           head.y + Math.floor(index / 3) * 8,
                           1, 1, 0, 0, -1)
        }
        visual.analyzeFoodClusters()

        const goal = visual.chooseGoal(snake, 0)

        fuzzyCompare(goal.x, head.x + 80, 0.1)
        compare(snake.foodPathIds.length, 1)
        fuzzyCompare(visual.foodById(snake.foodPathIds[0]).x,
                     head.x + 80, 0.1)
    }

    function test_foodClustersConnectAcrossWraparoundSeam() {
        visualContext.snakeDeadlyWalls = false
        visual.food = []
        visual.addFood(3, 320, 1, 0, 0, 0, -1)
        visual.addFood(visual.worldWidth - 3, 320, 1, 1, 0, 0, -1)

        visual.analyzeFoodClusters()

        verify(visual.food[0].clusterValue > 1.5)
        verify(visual.food[1].clusterValue > 1.5)
    }

    function test_foodPathContainsOnlyNearestReachableParticle() {
        const snake = visual.snakes[0]
        snake.segments[0].x = 300
        snake.segments[0].y = 320
        snake.angle = 0
        visual.snakes = [snake]
        visual.food = []
        visual.addFood(500, 320, 1, 0, 0, 0, -1)
        visual.addFood(540, 320, 1, 0, 0, 0, -1)
        visual.addFood(580, 320, 1, 0, 0, 0, -1)
        visual.addFood(600, 320, 1, 0, 0, 0, -1)
        const anchor = visual.food[3]

        const path = visual.buildFoodPath(snake, anchor)

        compare(path.length, 1)
        compare(visual.foodById(path[0]).x, 500)
    }

    function test_sideFoodOutsideFormerConeCanBeChosen() {
        const snake = visual.snakes[0]
        const head = snake.segments[0]
        snake.angle = 0
        visual.snakes = [snake]
        visual.food = []
        const insideAngle = 74 * Math.PI / 180
        const outsideAngle = 76 * Math.PI / 180
        visual.addFood(head.x + Math.cos(outsideAngle) * 70,
                       head.y + Math.sin(outsideAngle) * 70,
                       1, 0, 0, 0, -1)
        visual.addFood(head.x + Math.cos(insideAngle) * 90,
                       head.y + Math.sin(insideAngle) * 90,
                       1, 1, 0, 0, -1)
        visual.addFood(head.x + 110, head.y, 1, 2, 0, 0, -1)

        const chosen = visual.closestReachableFood(snake)

        verify(chosen !== null)
        fuzzyCompare(chosen.x, head.x + Math.cos(outsideAngle) * 70, 0.1)
        fuzzyCompare(chosen.y, head.y + Math.sin(outsideAngle) * 70, 0.1)
    }

    function test_farFoodDirectlyBehindCanBeChosen() {
        const snake = visual.snakes[0]
        const head = snake.segments[0]
        head.x = 400
        head.y = 320
        snake.angle = 0
        visual.snakes = [snake]
        visual.food = []
        visual.addFood(260, 320, 1, 0, 0, 0, -1)
        visual.addFood(560, 320, 1, 1, 0, 0, -1)

        const chosen = visual.closestReachableFood(snake, 0)

        verify(chosen !== null)
        fuzzyCompare(chosen.x, 260, 0.1)
    }

    function test_selectedFoodStaysLockedWhileTurnReachable() {
        const snake = visual.snakes[0]
        const head = snake.segments[0]
        head.x = 400
        head.y = 320
        snake.angle = 0
        visual.snakes = [snake]
        visual.food = []
        visual.addFood(520, 320, 1, 0, 0, 0, -1)

        const original = visual.chooseGoal(snake, 0)
        const originalId = snake.foodTargetId
        fuzzyCompare(original.x, 520, 0.1)

        // The old target is now side-on and a new particle is much closer, but
        // there is still enough room to complete the committed approach.
        snake.angle = Math.PI / 2
        visual.addFood(400, 370, 1, 1, 0, 0, -1)
        visual.simulationTime += 0.7
        const retained = visual.chooseGoal(snake, 0)

        compare(snake.foodTargetId, originalId)
        fuzzyCompare(retained.x, 520, 0.1)
        fuzzyCompare(retained.y, 320, 0.1)
    }

    function test_missedFoodInsideTurningPocketIsTemporarilyRejected() {
        const snake = visual.snakes[0]
        const head = snake.segments[0]
        head.x = 400
        head.y = 320
        snake.angle = 0
        visual.snakes = [snake]
        visual.food = []
        visual.addFood(520, 320, 1, 0, 0, 0, -1)
        visual.chooseGoal(snake, 0)
        const missedId = snake.foodTargetId

        // Simulate passing the particle. It is now too close behind for this
        // snake's minimum turn radius, while another reachable target is ahead.
        head.x = 565
        visual.addFood(650, 320, 1, 1, 0, 0, -1)
        visual.simulationTime += 0.5
        const replacement = visual.chooseGoal(snake, 0)

        compare(snake.rejectedFoodId, missedId)
        verify(snake.rejectedFoodUntil > visual.simulationTime)
        fuzzyCompare(replacement.x, 650, 0.1)
        compare(snake.foodTargetId, visual.food[1].id)
    }

    function test_foodTargetCanChangeAfterCommitmentExpires() {
        const snake = visual.snakes[0]
        const head = snake.segments[0]
        head.x = 400
        head.y = 320
        snake.angle = 0
        visual.snakes = [snake]
        visual.food = []
        visual.addFood(520, 320, 1, 0, 0, 0, -1)
        visual.chooseGoal(snake, 0)
        const oldDeadline = snake.foodTargetUntil

        snake.angle = Math.PI / 2
        visual.addFood(400, 370, 1, 1, 0, 0, -1)
        visual.simulationTime = oldDeadline + 0.01
        const replacement = visual.chooseGoal(snake, 0)

        fuzzyCompare(replacement.x, 400, 0.1)
        fuzzyCompare(replacement.y, 370, 0.1)
        compare(snake.foodTargetId, visual.food[1].id)
    }

    function test_vacuumLockedTargetIsReleasedImmediately() {
        const snake = visual.snakes[0]
        const head = snake.segments[0]
        head.x = 400
        head.y = 320
        snake.angle = 0
        visual.snakes = [snake]
        visual.food = []
        visual.addFood(470, 320, 1, 0, 0, 0, -1)
        visual.addFood(520, 320, 1, 1, 0, 0, -1)
        visual.chooseGoal(snake, 0)
        visual.food[0].vacuumOwner = 0

        const replacement = visual.chooseGoal(snake, 0)

        fuzzyCompare(replacement.x, 520, 0.1)
        compare(snake.foodTargetId, visual.food[1].id)
    }

    function test_unobstructedPlannerTurnsTowardCommittedFood() {
        visualContext.snakeIntelligence = 100
        const snake = visual.snakes[0]
        const head = snake.segments[0]
        head.x = 400
        head.y = 320
        snake.angle = 0
        snake.desiredAngle = 0
        visual.snakes = [snake]
        visual.food = []
        const targetAngle = 50 * Math.PI / 180
        visual.addFood(head.x + Math.cos(targetAngle) * 100,
                       head.y + Math.sin(targetAngle) * 100,
                       1, 0, 0, 0, -1)

        const plan = visual.createSteeringPlan(snake, 0)
        while (!visual.advanceSteeringPlan(plan)) {
        }
        const plannedAngle = plan.selectedAngle

        verify(Math.abs(visual.normalizeAngle(plannedAngle - targetAngle)) < 0.2,
               "unobstructed planner ignored its food bearing; planned="
               + plannedAngle + " target=" + targetAngle)
    }

    function test_foodPathSkipsCloseParticleInsideTurningPocket() {
        const snake = visual.snakes[0]
        snake.segments[0].x = 400
        snake.segments[0].y = 320
        snake.angle = 0
        visual.snakes = [snake]
        visual.food = []
        visual.addFood(390, 320, 1, 0, 0, 0, -1)
        visual.addFood(500, 320, 1, 1, 0, 0, -1)

        const path = visual.buildFoodPath(snake, null, 0, [])

        verify(path.length > 0)
        compare(visual.foodById(path[0]).x, 500)
    }

    function test_closestTargetRemainsTheOnlyMappedFoodNearCrossing() {
        visualContext.snakeIntelligence = 100
        const snake = visual.snakes[0]
        const rival = visual.snakes[1]
        const spacing = snake.radius * 1.18
        for (let index = 0; index < snake.segments.length; ++index) {
            snake.segments[index].x = 300 - spacing * index
            snake.segments[index].y = 320
        }
        for (let index = 0; index < rival.segments.length; ++index) {
            rival.segments[index].x = 400
            rival.segments[index].y = 160 - spacing * index
            rival.segments[index].previousX = rival.segments[index].x
            rival.segments[index].previousY = rival.segments[index].y
        }
        snake.angle = 0
        snake.desiredAngle = 0
        rival.angle = Math.PI / 2
        rival.desiredAngle = Math.PI / 2
        visual.snakes = [snake, rival]
        visual.food = []
        visual.addFood(650, 320, 1, 0, 0, 0, -1)
        const hazards = visual.collectHazards(snake, 0, 400)

        const path = visual.buildFoodPath(snake, null, 0, hazards)

        compare(path.length, 1)
        compare(visual.foodById(path[0]).x, 650)
    }

    function test_nearbyFoodWinsOverLargeDeathTrail() {
        visualContext.snakeIntelligence = 100
        const snake = visual.snakes[0]
        snake.segments[0].x = 300
        snake.segments[0].y = 320
        snake.angle = 0
        visual.snakes = [snake]
        visual.food = []
        visual.addFood(370, 320, 1, 0, 0, 0, -1)
        for (let index = 0; index < 8; ++index) {
            visual.addFood(530 + index * 12, 310 + (index % 3) * 10,
                           1, 1, 0, 0, -1, 19, index, 420)
        }
        visual.analyzeFoodClusters()

        const goal = visual.chooseGoal(snake, 0)

        fuzzyCompare(goal.x, 370, 0.1)
        compare(snake.foodPathIds.length, 1)
        compare(visual.foodById(snake.foodPathIds[0]).feastId, 0)
    }

    function test_deathTrailAlsoUsesNearestReachableParticle() {
        visualContext.snakeIntelligence = 100
        const snake = visual.snakes[0]
        const head = snake.segments[0]
        snake.angle = 0
        visual.snakes = [snake]
        visual.food = []
        visual.addFood(head.x + 180, head.y, 1, 0, 0, 0, -1, 7, 5, 12)
        visual.addFood(head.x + 260, head.y, 1, 0, 0, 0, -1, 7, 9, 12)

        const goal = visual.chooseGoal(snake, 0)

        fuzzyCompare(goal.x, head.x + 180, 0.1)
        compare(snake.foodPathIds.length, 1)
    }

    function test_bodyCollisionCreatesEdibleExplosion() {
        const victim = visual.snakes[0]
        const rival = visual.snakes[1]
        victim.segments[0].x = rival.segments[5].x
        victim.segments[0].y = rival.segments[5].y
        visual.markCollisions()
        verify(victim.dying)
        const previousFood = visual.food.length

        visual.explodeSnake(victim)

        verify(!victim.alive)
        compare(victim.segments.length, 0)
        verify(visual.food.length > previousFood)
        verify(victim.respawn > 2)
    }

    function test_neckSegmentsHaveNoCollisionGap() {
        const victim = visual.snakes[0]
        const rival = visual.snakes[1]
        visual.snakes = [victim, rival]
        const neck = rival.segments[2]
        victim.segments[0].x = neck.x
        victim.segments[0].y = neck.y
        victim.segments[0].previousX = neck.x
        victim.segments[0].previousY = neck.y

        visual.markCollisions()

        verify(victim.dying, "a rival head passed through the neck collision gap")
    }

    function test_fastHeadCannotTunnelThroughBodyBetweenSteps() {
        const victim = visual.snakes[0]
        const rival = visual.snakes[1]
        visual.snakes = [victim, rival]
        for (let index = 0; index < victim.segments.length; ++index) {
            victim.segments[index].x = 200 - index * victim.radius * 1.18
            victim.segments[index].y = 360
            victim.segments[index].previousX = victim.segments[index].x
            victim.segments[index].previousY = victim.segments[index].y
        }
        for (let index = 0; index < rival.segments.length; ++index) {
            rival.segments[index].x = 900 + index * rival.radius * 1.18
            rival.segments[index].y = 360
            rival.segments[index].previousX = rival.segments[index].x
            rival.segments[index].previousY = rival.segments[index].y
        }
        rival.segments[5].x = 600
        rival.segments[5].y = 360
        rival.segments[5].previousX = 600
        rival.segments[5].previousY = 360
        victim.segments[0].previousX = 570
        victim.segments[0].previousY = 360
        victim.segments[0].x = 630
        victim.segments[0].y = 360

        visual.markCollisions()

        verify(victim.dying, "a swept head crossed a body between simulation steps")
    }

    function test_selfCollisionCanBeEnabled() {
        const snake = visual.snakes[0]
        const body = snake.segments[12]
        snake.segments[0].x = body.x
        snake.segments[0].y = body.y
        visual.snakes = [snake]

        visualContext.snakeSelfCollisions = false
        visual.markCollisions()
        verify(!snake.dying)

        visualContext.snakeSelfCollisions = true
        visual.markCollisions()
        verify(snake.dying)
    }

    function test_selfSafetyBreaksOutwardFromFoodCircle() {
        visualContext.snakeIntelligence = 100
        visualContext.snakeSelfCollisions = true
        const snake = visual.snakes[0]
        visual.snakes = [snake]
        const spacing = snake.radius * 1.18
        for (let index = 0; index < snake.segments.length; ++index) {
            snake.segments[index].x = 500 - spacing * index
            snake.segments[index].y = 350
            snake.segments[index].previousX = snake.segments[index].x
            snake.segments[index].previousY = snake.segments[index].y
        }
        snake.angle = 0
        snake.desiredAngle = 0
        snake.segments[12].x = 500
        snake.segments[12].y = 350 + snake.radius * 2.5
        snake.segments[12].previousX = snake.segments[12].x
        snake.segments[12].previousY = snake.segments[12].y
        snake.avoidanceSide = 1
        snake.avoidanceCommitUntil = visual.simulationTime + 1
        snake.nextSafetyCheck = 0
        visual.markCollisions()
        verify(!snake.dying)

        verify(visual.applyCollisionSafety(snake, 0))

        verify(visual.normalizeAngle(snake.safetyDesiredAngle - snake.angle) < 0,
               "self safety followed the existing inward turn instead of escaping")
        verify(snake.safetyActiveUntil >= visual.simulationTime + 0.27)
    }

    function test_wraparoundPreservesBodySpacing() {
        visualContext.snakeDeadlyWalls = false
        visual.snakes = [visual.makeSnake(0)]
        const snake = visual.snakes[0]
        const spacing = snake.radius * 1.18
        snake.angle = 0
        snake.desiredAngle = 0
        snake.brainCooldown = 10
        for (let index = 0; index < snake.segments.length; ++index) {
            snake.segments[index].x = visual.worldWidth - 0.25 - spacing * index
            snake.segments[index].y = visual.worldHeight / 2
        }

        visual.moveSnake(snake, 0, 1 / 30)

        verify(snake.segments[0].x < 20)
        const distance = Math.sqrt(visual.worldDistanceSquared(
            snake.segments[0].x, snake.segments[0].y,
            snake.segments[1].x, snake.segments[1].y))
        fuzzyCompare(distance, spacing, 0.1)
        visual.markCollisions()
        verify(!snake.dying)
    }

    function test_wraparoundCollisionAcrossNarrowFinalBin() {
        visualContext.snakeDeadlyWalls = false
        const victim = visual.snakes[0]
        const rival = visual.snakes[1]
        victim.radius = 10
        rival.radius = 10
        for (let index = 0; index < victim.segments.length; ++index) {
            victim.segments[index].x = 300
            victim.segments[index].y = 1
        }
        for (let index = 0; index < rival.segments.length; ++index) {
            rival.segments[index].x = 900
            rival.segments[index].y = 360
        }
        // With the former target-sized bins this point occupied the
        // penultimate row, even though it is only ten pixels from row zero in
        // toroidal space. A row-zero head never queried that bin.
        rival.segments[5].x = 300
        rival.segments[5].y = visual.worldHeight - 9
        visual.snakes = [victim, rival]

        visual.markCollisions()

        verify(victim.dying)
    }

    function test_extrapolatedHazardWrapsMoreThanOneWorldExtent() {
        visualContext.snakeDeadlyWalls = false
        visualContext.monitorBehavior = "seamless"
        visualContext.virtualWidth = 120
        visualContext.virtualHeight = 100
        const snake = visual.snakes[0]
        snake.radius = 10
        snake.segments[0].x = 80
        snake.segments[0].y = 50
        const points = [
            { x: 79, y: 50, angle: 0, time: 0 },
            { x: 81, y: 50, angle: 0, time: 1 }
        ]
        const hazards = [{
            x: 20, y: 50, x2: 20, y2: 50,
            vx: 600, vy: 0, vx2: 600, vy2: 0,
            radius: 10, self: false, head: true,
            halfLength: 0, halfStretchSpeed: 0,
            releaseTime: Number.MAX_VALUE
        }]

        const result = visual.evaluateTrajectory(snake, 0, points, hazards, 81, 50)

        verify(result.collides)
    }

    function test_longWraparoundBodiesGenerateEveryVisibleCopy() {
        const offsets = visual.wrappingOffsets(-2600, 400, 1000, 20)

        compare(offsets.length, 4)
        compare(offsets[0], 0)
        compare(offsets[1], 1000)
        compare(offsets[2], 2000)
        compare(offsets[3], 3000)
    }

    function test_wraparoundFoodVacuumCrossesSeam() {
        visualContext.snakeDeadlyWalls = false
        const snake = visual.snakes[0]
        snake.segments[0].x = 1
        snake.segments[0].y = 200
        visual.food = [{
            x: visual.worldWidth - snake.radius,
            y: 200,
            value: 1,
            colorIndex: 0,
            size: 1,
            vx: 0,
            vy: 0,
            life: -1,
            phase: 0
        }]

        visual.feedSnake(snake)

        compare(visual.food.length, 0)
    }

    function test_largerSnakesCreateMoreDeathFood() {
        const small = visual.makeSnake(1)
        const large = visual.makeSnake(2)
        for (let index = 0; index < 45; ++index)
            visual.appendGrowthSegment(large)

        visual.food = []
        const smallCount = visual.explodeSnake(small)
        visual.food = []
        const largeCount = visual.explodeSnake(large)

        verify(largeCount > smallCount * 2)
    }

    function test_veryLongSnakeDropsMoreThanFormerParticleCap() {
        const snake = visual.makeSnake(1)
        while (snake.segments.length < 240)
            visual.appendGrowthSegment(snake)
        visual.food = []

        const particleCount = visual.explodeSnake(snake)

        verify(particleCount > 180)
        compare(visual.food.length, particleCount)
    }

    function test_deathFoodIsBoundedAndFreshCorpseGetsVisibleBurst() {
        visual.food = []
        while (visual.food.length < visual.maximumFoodCount)
            visual.addAmbientFood()

        const snake = visual.makeSnake(1)
        while (snake.segments.length < 240)
            visual.appendGrowthSegment(snake)
        const particleCount = visual.explodeSnake(snake)

        verify(particleCount >= 48)
        verify(visual.food.length <= visual.maximumFoodCount)
        let corpseCount = 0
        let corpseValue = 0
        for (let index = 0; index < visual.food.length; ++index) {
            if (visual.food[index].feastId > 0) {
                ++corpseCount
                corpseValue += visual.food[index].value
            }
        }
        compare(corpseCount, particleCount)
        verify(corpseValue > snake.birthLength * 4,
               "bounded particles lost most of the defeated snake's value")
    }

    function test_continuousCapsulesCloseSamplingGaps() {
        visualContext.snakeIntelligence = 100
        const snake = visual.snakes[0]
        const rival = visual.snakes[1]
        const spacing = snake.radius * 1.18
        for (let index = 0; index < snake.segments.length; ++index) {
            snake.segments[index].x = 300 - spacing * index
            snake.segments[index].y = 360
        }
        for (let index = 0; index < rival.segments.length; ++index) {
            rival.segments[index].x = 1050 + spacing * index
            rival.segments[index].y = 100
        }
        // Only the centreline between sampled body points intersects the
        // route. A point-cloud planner would see an inviting fake gap.
        rival.segments[3].x = 420
        rival.segments[3].y = 300
        rival.segments[4].x = 420
        rival.segments[4].y = 360
        rival.segments[5].x = 420
        rival.segments[5].y = 420
        snake.angle = 0
        snake.desiredAngle = 0
        visual.snakes = [snake, rival]
        visual.food = []
        snake.foodPlanUntil = 100
        snake.plannedGoalX = 560
        snake.plannedGoalY = 360

        const directRisk = visual.headingRisk(snake, 0, 0, 260)
        const plannedAngle = visual.planSteering(snake, 0)
        const plannedRisk = visual.headingRisk(snake, 0, plannedAngle, 260)

        verify(directRisk > 0)
        verify(plannedRisk < directRisk,
               "capsule route risk=" + plannedRisk + ", direct=" + directRisk)
    }

    function test_predictsRivalHeadCrossing() {
        visualContext.snakeIntelligence = 100
        const snake = visual.snakes[0]
        const rival = visual.snakes[1]
        const spacing = snake.radius * 1.18
        for (let index = 0; index < snake.segments.length; ++index) {
            snake.segments[index].x = 300 - spacing * index
            snake.segments[index].y = 360
        }
        for (let index = 0; index < rival.segments.length; ++index) {
            rival.segments[index].x = 420
            rival.segments[index].y = 235 - spacing * index
            rival.segments[index].previousX = rival.segments[index].x
            rival.segments[index].previousY = rival.segments[index].y
        }
        snake.angle = 0
        snake.desiredAngle = 0
        rival.angle = Math.PI / 2
        rival.desiredAngle = Math.PI / 2
        visual.snakes = [snake, rival]
        visual.food = []
        visual.addFood(700, 360, 1, 0, 0, 0, -1)

        const directRisk = visual.headingRisk(snake, 0, 0, 300)
        const plannedAngle = visual.planSteering(snake, 0)

        verify(directRisk > 0)
        verify(Math.abs(plannedAngle) > 0.1,
               "planner held the collision course: " + plannedAngle)
        verify(!snake.lastPlanCollision)
    }

    function test_safeMappedWaypointCannotBeOverriddenByOtherFood() {
        visualContext.snakeIntelligence = 100
        const snake = visual.snakes[0]
        const spacing = snake.radius * 1.18
        for (let index = 0; index < snake.segments.length; ++index) {
            snake.segments[index].x = 300 - spacing * index
            snake.segments[index].y = 360
        }
        snake.angle = 0
        snake.desiredAngle = 0
        visual.snakes = [snake]
        visual.food = []
        visual.addFood(700, 360, 1, 0, 0, 0, -1)
        const mappedTarget = visual.food[0]
        const diversionPath = visual.projectTrajectory(snake, 0.46, 0.28,
                                                       700, 360, 400, 8)
        for (let index = 1; index <= 4; ++index) {
            const point = diversionPath[index]
            visual.addFood(point.x, point.y, 1, 1, 0, 0, -1)
            visual.addFood(point.x + 5, point.y + 4, 1, 1, 0, 0, -1)
        }
        snake.foodPathIds = [mappedTarget.id]
        snake.foodPathUntil = 100
        snake.foodPlanUntil = 100
        snake.plannedGoalX = mappedTarget.x
        snake.plannedGoalY = mappedTarget.y

        const plan = visual.createSteeringPlan(snake, 0)
        while (!visual.advanceSteeringPlan(plan)) {
            // Complete the incremental plan for this focused assertion.
        }
        const selected = plan.evaluated.find(function(result) {
            return Math.abs(visual.normalizeAngle(
                result.candidate.angle - plan.selectedAngle)) < 0.001
        })

        verify(selected !== undefined)
        verify(selected.capturesRouteTarget,
               "selected route missed the explicitly mapped waypoint")
    }

    function test_highRiskTargetCaptureDoesNotOverrideSaferRoute() {
        visualContext.snakeIntelligence = 100
        const snake = visual.snakes[0]
        snake.angle = 0
        snake.avoidanceCommitUntil = 0
        const target = { id: 91 }
        const riskyCapture = {
            candidate: { angle: 0.4, recovery: 0 },
            collides: false,
            capturesRouteTarget: true,
            risk: 9,
            baseScore: 90,
            points: []
        }
        const safeEscape = {
            candidate: { angle: -0.7, recovery: 0.5 },
            collides: false,
            capturesRouteTarget: false,
            risk: 0.2,
            baseScore: 2,
            points: []
        }
        const plan = {
            snake: snake,
            routeTarget: target,
            evaluated: [riskyCapture, safeEscape],
            hasSafeRoute: true,
            goalAngle: 0.4,
            hazards: [{ head: false }],
            commitActive: false,
            complete: false
        }

        visual.finishSteeringPlan(plan)

        fuzzyCompare(plan.selectedAngle, safeEscape.candidate.angle, 0.001)
        verify(plan.complete)
    }

    function test_intelligenceExpandsHazardAwareness() {
        const snake = visual.snakes[0]
        const rival = visual.snakes[1]
        const head = snake.segments[0]
        for (let index = 0; index < rival.segments.length; ++index) {
            rival.segments[index].x = head.x + 80
            rival.segments[index].y = head.y + (index - 5) * rival.radius
        }
        snake.angle = 0

        visualContext.snakeIntelligence = 0
        const recklessRisk = visual.headingRisk(snake, 0, 0, 200)
        visualContext.snakeIntelligence = 100
        const smartRisk = visual.headingRisk(snake, 0, 0, 200)

        verify(smartRisk > recklessRisk)
    }

    function test_highIntelligenceChoosesSaferRoute() {
        const snake = visual.snakes[0]
        const rival = visual.snakes[1]
        const spacing = snake.radius * 1.18
        for (let index = 0; index < snake.segments.length; ++index) {
            snake.segments[index].x = 300 - spacing * index
            snake.segments[index].y = 360
        }
        for (let index = 0; index < rival.segments.length; ++index) {
            rival.segments[index].x = 420
            rival.segments[index].y = 270 + index * rival.radius * 1.5
        }
        snake.angle = 0
        snake.desiredAngle = 0
        visual.snakes = [snake, rival]
        visual.food = []
        visual.addFood(620, 360, 1, 0, 0, 0, -1)

        visualContext.snakeIntelligence = 0
        const recklessAngle = visual.planSteering(snake, 0)
        visualContext.snakeIntelligence = 100
        snake.desiredAngle = 0
        const smartAngle = visual.planSteering(snake, 0)
        const recklessRisk = visual.headingRisk(snake, 0, recklessAngle, 300)
        const smartRisk = visual.headingRisk(snake, 0, smartAngle, 300)

        verify(smartRisk < recklessRisk,
               "smart risk=" + smartRisk + ", reckless risk=" + recklessRisk)
    }

    function test_highIntelligenceAvoidsOwnBody() {
        visualContext.snakeIntelligence = 100
        visualContext.snakeSelfCollisions = true
        const snake = visual.snakes[0]
        const spacing = snake.radius * 1.18
        while (snake.segments.length < 200)
            visual.appendGrowthSegment(snake)
        for (let index = 0; index <= 20; ++index) {
            snake.segments[index].x = 300 - spacing * index
            snake.segments[index].y = 360
        }
        for (let index = 21; index <= 50; ++index) {
            snake.segments[index].x = 300 - spacing * 20
            snake.segments[index].y = 360 - (index - 20) * spacing
        }
        for (let index = 51; index <= 90; ++index) {
            snake.segments[index].x = 300 - spacing * 20
                                      + (index - 50) * spacing
            snake.segments[index].y = 360 - spacing * 30
        }
        for (let index = 91; index <= 141; ++index) {
            snake.segments[index].x = 300 - spacing * 20 + spacing * 40
            snake.segments[index].y = 360 - spacing * 30
                                      + (index - 90) * spacing
        }
        for (let index = 142; index < snake.segments.length; ++index) {
            snake.segments[index].x = 300 - spacing * 20 + spacing * 40
                                      - (index - 141) * spacing
            snake.segments[index].y = 360 - spacing * 30 + spacing * 51
        }
        snake.angle = 0
        snake.desiredAngle = 0
        snake.foodPlanUntil = 0
        visual.snakes = [snake]
        visual.food = []
        visual.addFood(620, 360, 1, 0, 0, 0, -1)

        const directRisk = visual.headingRisk(snake, 0, 0, 280)
        const plannedAngle = visual.planSteering(snake, 0)
        const plannedRisk = visual.headingRisk(snake, 0, plannedAngle, 280)

        verify(directRisk > 0)
        verify(plannedRisk < directRisk,
               "planned self risk=" + plannedRisk + ", direct=" + directRisk)
    }

    function test_avoidanceTurnsWithoutBraking() {
        visualContext.snakeIntelligence = 100
        const snake = visual.snakes[0]
        const rival = visual.snakes[1]
        const head = snake.segments[0]
        for (let index = 0; index < rival.segments.length; ++index) {
            rival.segments[index].x = head.x + 90
            rival.segments[index].y = head.y + (index - 6) * rival.radius
        }
        snake.angle = 0
        snake.desiredAngle = 0
        snake.rush = 0
        visual.snakes = [snake, rival]
        visual.food = []
        visual.addFood(head.x + 300, head.y, 1, 0, 0, 0, -1)
        const speedBefore = visual.snakeSpeed(snake)

        visual.planSteering(snake, 0)

        fuzzyCompare(visual.snakeSpeed(snake), speedBefore, 0.01)
    }

    function test_predictiveBrainPlanningIsBoundedAcrossFrames() {
        visualContext.snakeIntelligence = 100
        visual.cancelActiveBrainPlan()
        visual.brainCursor = 0
        for (let index = 0; index < visual.snakes.length; ++index) {
            visual.snakes[index].brainCooldown = 0
            visual.snakes[index].brainPlanning = false
        }
        const firstSnake = visual.snakes[0]

        visual.updateSnakeBrains(0)
        verify(firstSnake.brainPlanning)
        compare(visual.activeBrainPlan.snake, firstSnake)
        compare(visual.brainCursor, 1)
        verify(visual.lastBrainWorkUnits <= visual.maximumBrainWorkUnits)
        verify(visual.activeBrainPlan.candidateIndex
               <= visual.maximumBrainWorkUnits - 1)

        let ticks = 1
        while (firstSnake.brainPlanning && ticks < 30) {
            visual.updateSnakeBrains(0)
            verify(visual.lastBrainWorkUnits <= visual.maximumBrainWorkUnits)
            ++ticks
        }
        verify(!firstSnake.brainPlanning)
        verify(firstSnake.brainCooldown > 0)
        // A safe direct route can discard the extreme escape candidates after
        // its first rollout, but planning must still remain incremental.
        verify(ticks >= 2)
        verify(ticks < 20)
    }

    function test_safeDirectPlanSkipsExtremeEscapeRollouts() {
        const snake = visual.snakes[0]
        const plan = visual.createSteeringPlan(snake, 0)
        verify(plan.candidates.length > 6)

        // Isolate the candidate-budget decision from the random world state;
        // candidate zero still goes through the real rollout implementation.
        plan.hazards = []
        plan.goal.x = snake.segments[0].x + Math.cos(snake.angle) * 180
        plan.goal.y = snake.segments[0].y + Math.sin(snake.angle) * 180
        plan.candidates[0].angle = snake.angle
        visual.evaluateNextPlanCandidate(plan)

        compare(plan.candidates.length, 6)
    }

    function test_incrementalPlanUsesOneImmutableSnakeState() {
        const snake = visual.snakes[0]
        const originalX = snake.segments[0].x
        const originalY = snake.segments[0].y
        const originalAngle = snake.angle
        const plan = visual.createSteeringPlan(snake, 0)

        visual.evaluateNextPlanCandidate(plan)
        snake.segments[0].x += 180
        snake.segments[0].y += 90
        snake.angle = visual.normalizeAngle(snake.angle + 1.2)
        visual.evaluateNextPlanCandidate(plan)

        fuzzyCompare(plan.evaluated[0].points[0].x, originalX, 0.001)
        fuzzyCompare(plan.evaluated[0].points[0].y, originalY, 0.001)
        fuzzyCompare(plan.evaluated[0].points[0].angle, originalAngle, 0.001)
        fuzzyCompare(plan.evaluated[1].points[0].x, originalX, 0.001)
        fuzzyCompare(plan.evaluated[1].points[0].y, originalY, 0.001)
        fuzzyCompare(plan.evaluated[1].points[0].angle, originalAngle, 0.001)
    }

    function test_maxDensitySimulationHasRealtimeHeadroom() {
        visualContext.animationDensity = 100
        visualContext.trailAmount = 100
        visualContext.snakeIntelligence = 100
        visualContext.snakeSelfCollisions = true
        visual.initializeWorld()
        const started = Date.now()

        for (let step = 0; step < 300; ++step)
            visual.stepSimulation(1 / 30)

        const elapsed = Date.now() - started
        verify(elapsed < 5000,
               "10 simulated seconds took " + elapsed + " ms")
        // This ecosystem assertion detects runaway collision cascades; exact
        // safety decisions are covered by the focused hazard tests above.
        // One changed route can deterministically alter later encounters, so
        // keep a small bound rather than requiring an exact historical count.
        verify(visual.deathCount <= 4,
               visual.deathCount + " deaths for " + visual.desiredSnakeCount
               + " intelligent snakes in 10 seconds (walls="
               + visual.wallDeathCount + ", heads=" + visual.headDeathCount
               + ", bodies=" + visual.bodyDeathCount + ", self="
               + visual.selfDeathCount + ")")
    }

    function test_longSnakeSimulationHasRealtimeHeadroom() {
        visualContext.snakeDeadlyWalls = false
        visual.snakes = [visual.makeSnake(0)]
        const snake = visual.snakes[0]
        const targetLength = Math.min(500, visual.maximumSnakeSegments(snake) - 1)
        while (snake.segments.length < targetLength)
            visual.appendGrowthSegment(snake)
        visual.food = []
        const started = Date.now()

        for (let step = 0; step < 150; ++step)
            visual.stepSimulation(1 / 30)

        const elapsed = Date.now() - started
        verify(elapsed < 2000,
               "5 seconds with a " + targetLength + "-segment snake took "
               + elapsed + " ms")
    }

    function test_matureEcosystemHasRealtimeHeadroom() {
        visualContext.animationDensity = 55
        visualContext.trailAmount = 100
        visualContext.snakeIntelligence = 100
        visualContext.snakeSelfCollisions = true
        visualContext.snakeDeadlyWalls = false
        visual.initializeWorld()
        for (let snakeIndex = 0; snakeIndex < visual.snakes.length; ++snakeIndex) {
            const snake = visual.snakes[snakeIndex]
            while (snake.segments.length < 200)
                visual.appendGrowthSegment(snake)
        }
        const started = Date.now()

        for (let step = 0; step < 150; ++step)
            visual.stepSimulation(1 / 30)

        const elapsed = Date.now() - started
        verify(elapsed < 4000,
               "5 mature ecosystem seconds took " + elapsed + " ms")
    }

    function test_seamlessVirtualDesktopCoordinates() {
        visualContext.monitorBehavior = "seamless"
        visualContext.virtualX = -1920
        visualContext.virtualWidth = 5360
        visualContext.screenX = 0

        tryCompare(visual, "worldWidth", 5360)
        compare(visual.drawOffsetX, -1920)
    }

    function test_simulationSnapshotRestoresAndScalesWorld() {
        visualContext.monitorBehavior = "seamless"
        visualContext.virtualWidth = 1280
        visual.initializeWorld()
        const originalHeadX = visual.snakes[0].segments[0].x
        const originalHeadY = visual.snakes[0].segments[0].y
        visual.simulationTime = 12.5
        visual.randomState = 987654
        const snapshot = visual.simulationSnapshot()

        visualContext.virtualWidth = 640
        visual.snakes = []
        visual.food = []
        verify(visual.restoreSimulationSnapshot(snapshot))

        compare(visual.snakes.length, 9)
        fuzzyCompare(visual.snakes[0].segments[0].x, originalHeadX * 0.5, 0.01)
        fuzzyCompare(visual.snakes[0].segments[0].y, originalHeadY, 0.01)
        compare(visual.simulationTime, 12.5)
        compare(visual.randomState, 987654)
        compare(visual.initializedWidth, 640)
    }

    function test_seamlessArenaResizePreservesLiveSimulation() {
        visualContext.monitorBehavior = "seamless"
        visualContext.virtualWidth = 1280
        visual.initializeWorld()
        const firstSnake = visual.snakes[0]
        const originalHeadX = firstSnake.segments[0].x
        visual.simulationTime = 9.25

        visualContext.virtualWidth = 1920
        visual.synchronizeWorldGeometry()

        verify(visual.snakes[0] === firstSnake)
        fuzzyCompare(firstSnake.segments[0].x, originalHeadX * 1.5, 0.01)
        compare(visual.simulationTime, 9.25)
        compare(visual.initializedWidth, 1920)
    }
}
