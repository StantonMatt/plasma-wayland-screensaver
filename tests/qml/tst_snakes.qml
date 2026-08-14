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

    function test_initialPopulation() {
        compare(visual.snakes.length, 9)
        compare(visual.food.length, 82)
        for (let index = 0; index < visual.snakes.length; ++index) {
            verify(visual.snakes[index].alive)
            verify(visual.snakes[index].segments.length >= 14)
        }
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

    function test_cleverSnakeChoosesRichFoodCluster() {
        visualContext.snakeIntelligence = 100
        const snake = visual.snakes[0]
        const head = snake.segments[0]
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

        verify(goal.x > head.x + 150)
        verify(snake.rush > 0)
    }

    function test_routeScoringUsesActualVacuumCorridor() {
        const snake = visual.snakes[0]
        snake.segments[0].x = 300
        snake.segments[0].y = 320
        visual.snakes = [snake]
        visual.food = []
        visual.addFood(560, 320, 1, 0, 0, 0, -1)
        let target = visual.food[0]
        const captureRadius = visual.foodCaptureRadius(snake, target)
        for (let index = 0; index < 5; ++index) {
            visual.addFood(380 + index * 32, 320 + captureRadius * 0.82,
                           1, 1, 0, 0, -1)
        }
        const insideHarvest = visual.routeHarvestValue(snake, target)

        visual.food = []
        visual.addFood(560, 320, 1, 0, 0, 0, -1)
        target = visual.food[0]
        for (let index = 0; index < 5; ++index) {
            visual.addFood(380 + index * 32, 320 + captureRadius * 1.18,
                           1, 1, 0, 0, -1)
        }
        const outsideHarvest = visual.routeHarvestValue(snake, target)

        verify(insideHarvest > outsideHarvest * 2)
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

    function test_foodPathTracesNearestNeighborsInsideClump() {
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

        compare(visual.foodById(path[0]).x, 500)
        compare(visual.foodById(path[1]).x, 540)
        compare(visual.foodById(path[2]).x, 580)
        compare(visual.foodById(path[3]).x, 600)
    }

    function test_unreachableFoodGetsTurningCircleApproach() {
        const snake = visual.snakes[0]
        snake.segments[0].x = 400
        snake.segments[0].y = 320
        snake.angle = 0
        visual.snakes = [snake]
        visual.food = []
        visual.addFood(390, 320, 1, 0, 0, 0, -1)
        const particle = visual.food[0]

        const penalty = visual.turnReachabilityPenalty(snake, particle)
        const approach = visual.foodApproachGoal(snake, particle)

        verify(penalty > 4.5)
        verify(!approach.direct)
        verify(approach.x > snake.segments[0].x)
        verify(Math.abs(approach.y - snake.segments[0].y) > 1)
    }

    function test_largeDeathTrailOutranksNearbyAmbientFood() {
        visualContext.snakeIntelligence = 100
        const snake = visual.snakes[0]
        snake.segments[0].x = 300
        snake.segments[0].y = 320
        snake.feastId = 0
        snake.feastTargetIndex = -1
        visual.snakes = [snake]
        visual.food = []
        visual.addFood(370, 320, 1, 0, 0, 0, -1)
        for (let index = 0; index < 8; ++index) {
            visual.addFood(530 + index * 12, 310 + (index % 3) * 10,
                           1, 1, 0, 0, -1, 19, index, 420)
        }
        visual.analyzeFoodClusters()

        const goal = visual.chooseGoal(snake, 0)

        compare(snake.feastId, 19)
        verify(goal.x > 500)
        verify(snake.rush >= 0.28)
    }

    function test_snakeSweepsAlongDeathParticleTrail() {
        visualContext.snakeIntelligence = 100
        const snake = visual.snakes[0]
        const head = snake.segments[0]
        visual.snakes = [snake]
        visual.food = []
        visual.addFood(head.x + 180, head.y, 1, 0, 0, 0, -1, 7, 5, 12)
        visual.addFood(head.x + 260, head.y, 1, 0, 0, 0, -1, 7, 9, 12)
        snake.feastId = 7
        snake.feastTargetIndex = 5
        snake.feastDirection = 1

        const goal = visual.chooseGoal(snake, 0)

        fuzzyCompare(goal.x, head.x + 260, 0.1)
        verify(goal.harvest > 1)
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
        while (snake.segments.length < 28)
            visual.appendGrowthSegment(snake)
        for (let index = 0; index < 8; ++index) {
            snake.segments[index].x = 300 - spacing * index
            snake.segments[index].y = 360
        }
        for (let index = 8; index < snake.segments.length; ++index) {
            snake.segments[index].x = 390
            snake.segments[index].y = 250 + (index - 8) * spacing
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

    function test_maxDensitySimulationHasRealtimeHeadroom() {
        visualContext.animationDensity = 100
        visualContext.trailAmount = 100
        visualContext.snakeIntelligence = 100
        visualContext.snakeSelfCollisions = true
        visual.initializeWorld()
        const started = Date.now()

        for (let step = 0; step < 600; ++step)
            visual.stepSimulation(1 / 60)

        const elapsed = Date.now() - started
        verify(elapsed < 5000,
               "10 simulated seconds took " + elapsed + " ms")
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

        for (let step = 0; step < 300; ++step)
            visual.stepSimulation(1 / 60)

        const elapsed = Date.now() - started
        verify(elapsed < 2000,
               "5 seconds with a " + targetLength + "-segment snake took "
               + elapsed + " ms")
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
