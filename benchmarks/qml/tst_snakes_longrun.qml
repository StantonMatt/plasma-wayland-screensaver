// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import "../../qml/visuals"

TestCase {
    id: testCase
    name: "SlitheringSnakesLongRunBenchmark"
    width: 64
    height: 64
    visible: true
    when: windowShown

    readonly property int simulatedMinutes: 8
    readonly property int stepsPerMinute: 60 * 30
    property var rendererStub: ({
        syncFrame: function() {},
        presentFrame: function() {}
    })

    QtObject {
        id: visualContext
        property int animationSpeed: 100
        property int animationDensity: 100
        property int animationScale: 100
        property string animationPalette: "spectrum"
        property int trailAmount: 100
        property int snakeIntelligence: 100
        property bool snakeSelfCollisions: true
        property bool snakeDeadlyWalls: true
        property string monitorBehavior: "independent"
        property real virtualX: 0
        property real virtualY: 0
        property real virtualWidth: 3440
        property real virtualHeight: 1440
        property real screenX: 0
        property real screenY: 0
        property var presentationClock: null
    }

    Snakes {
        id: visual
        width: 3440
        height: 1440
        visible: false
        context: visualContext
        nativeRenderer: testCase.rendererStub
        simulationDriver: true
        reducedMotion: false
        seed: 20260814
    }

    function statistics() {
        let alive = 0
        let totalSegments = 0
        let maximumSegments = 0
        let trailPoints = 0
        let retainedTrailPoints = 0
        for (let index = 0; index < visual.snakes.length; ++index) {
            const snake = visual.snakes[index]
            if (!snake.alive)
                continue
            ++alive
            totalSegments += snake.segments.length
            maximumSegments = Math.max(maximumSegments, snake.segments.length)
            if (snake.trailPoints) {
                trailPoints += snake.trailPoints.length
                retainedTrailPoints += Math.max(
                    0, snake.trailPoints.length - (snake.trailStart || 0))
            }
        }
        // This is deliberately an estimate: the native renderer culls
        // off-screen wrap copies. It remains stable enough to expose growth in
        // scene-graph work from one benchmark window to the next.
        const estimatedVertices = visual.food.length * 48
            + totalSegments * 12 + alive * 180
        return {
            alive: alive,
            totalSegments: totalSegments,
            maximumSegments: maximumSegments,
            food: visual.food.length,
            trailPoints: trailPoints,
            retainedTrailPoints: retainedTrailPoints,
            deaths: visual.deathCount,
            estimatedVertices: estimatedVertices
        }
    }

    function profiledStep(seconds, timings) {
        visual.simulationTime += seconds
        let started = Date.now()
        visual.updateFood(seconds)
        visual.foodAnalysisCooldown -= seconds
        if (visual.foodAnalysisCooldown <= 0) {
            visual.analyzeFoodClusters()
            const foodLoad = visual.clamp(
                (visual.food.length - visual.desiredFoodCount)
                / Math.max(1, visual.maximumFoodCount - visual.desiredFoodCount), 0, 1)
            visual.foodAnalysisCooldown = 0.32 + foodLoad * 0.28
        }
        timings.food += Date.now() - started

        started = Date.now()
        visual.growthSlots = Math.max(
            0, visual.maximumWorldSegments() - visual.totalLiveSegments())
        visual.updateSnakeBrains(seconds)
        timings.brains += Date.now() - started

        started = Date.now()
        for (let index = 0; index < visual.snakes.length; ++index) {
            const snake = visual.snakes[index]
            if (!snake.alive) {
                snake.respawn -= seconds
                if (snake.respawn <= 0)
                    visual.snakes[index] = visual.makeSnake(index)
                continue
            }
            visual.moveSnake(snake, index, seconds)
        }
        timings.movement += Date.now() - started

        started = Date.now()
        visual.feedSnakes(seconds)
        timings.feeding += Date.now() - started

        started = Date.now()
        visual.markCollisions()
        for (let index = 0; index < visual.snakes.length; ++index) {
            if (visual.snakes[index].alive && visual.snakes[index].dying)
                visual.explodeSnake(visual.snakes[index])
        }
        timings.collisions += Date.now() - started
    }

    function prepareMaturePlannerFixture() {
        visual.initializeWorld()
        for (let snakeIndex = 0; snakeIndex < visual.snakes.length; ++snakeIndex) {
            const snake = visual.snakes[snakeIndex]
            while (snake.segments.length < 120)
                visual.appendGrowthSegment(snake)
            visual.rebuildSnakeTrail(snake)
        }
        while (visual.food.length < 400)
            visual.addAmbientFood()
        visual.simulationTime = 20
    }

    function resetPlannerSnakeInputs(snake) {
        snake.desiredAngle = snake.angle
        snake.avoidanceSide = 0
        snake.avoidanceCommitUntil = 0
        snake.foodPlanUntil = 0
        snake.foodTargetId = 0
        snake.foodTargetUntil = 0
        snake.foodPathIds = []
        snake.foodPathUntil = 0
        snake.feastId = 0
        snake.feastTargetIndex = -1
        snake.feastDirection = 1
        snake.rush = 0
        snake.lastPlanRisk = 0
        snake.lastPlanCollision = false
        snake.lastPlanHarvest = 0
    }

    function resetPlannerFixtureInputs() {
        for (let snakeIndex = 0; snakeIndex < visual.snakes.length; ++snakeIndex)
            resetPlannerSnakeInputs(visual.snakes[snakeIndex])
        for (let foodIndex = 0; foodIndex < visual.food.length; ++foodIndex) {
            visual.food[foodIndex].claimedBy = -1
            visual.food[foodIndex].claimedUntil = 0
        }
    }

    function test_fixedMaturePlannerThroughput() {
        prepareMaturePlannerFixture()
        const rounds = 6
        let planCount = 0
        let candidateCount = 0
        let hazardCount = 0
        const started = Date.now()
        for (let round = 0; round < rounds; ++round) {
            for (let snakeIndex = 0; snakeIndex < visual.snakes.length; ++snakeIndex) {
                const snake = visual.snakes[snakeIndex]
                resetPlannerFixtureInputs()
                const plan = visual.createSteeringPlan(snake, snakeIndex)
                hazardCount += plan.hazards.length
                while (!visual.advanceSteeringPlan(plan)) {
                    // Intentionally synchronous: this benchmark measures the
                    // complete planner independent of its frame scheduler.
                }
                candidateCount += plan.evaluated.length
                ++planCount
            }
        }
        const elapsed = Date.now() - started
        console.info("SNAKE_PLANNER_CSV,total_ms,plans,ms_per_plan,avg_candidates,avg_hazards,segments,food")
        console.info("SNAKE_PLANNER_CSV,"
                     + elapsed + ","
                     + planCount + ","
                     + (elapsed / planCount).toFixed(4) + ","
                     + (candidateCount / planCount).toFixed(2) + ","
                     + (hazardCount / planCount).toFixed(2) + ","
                     + visual.totalLiveSegments() + ","
                     + visual.food.length)
        verify(planCount > 0)
        verify(elapsed > 0)
    }

    function test_fixedFoodCaptureThroughput() {
        visual.initializeWorld()
        while (visual.food.length < 400)
            visual.addAmbientFood()
        // Keep every particle outside every capture radius so each call does
        // the full food-by-snake search without mutating the fixture.
        for (let snakeIndex = 0; snakeIndex < visual.snakes.length; ++snakeIndex) {
            visual.snakes[snakeIndex].segments[0].x = 40
            visual.snakes[snakeIndex].segments[0].y = 40
        }
        for (let foodIndex = 0; foodIndex < visual.food.length; ++foodIndex) {
            visual.food[foodIndex].x = 1200 + (foodIndex % 40) * 20
            visual.food[foodIndex].y = 700 + Math.floor(foodIndex / 40) * 20
        }

        const calls = 1000
        const started = Date.now()
        for (let call = 0; call < calls; ++call)
            visual.feedSnakes(1 / 30)
        const elapsed = Date.now() - started
        console.info("SNAKE_FEEDING_CSV,total_ms,calls,ms_per_call,snakes,food,comparisons_per_call")
        console.info("SNAKE_FEEDING_CSV,"
                     + elapsed + ","
                     + calls + ","
                     + (elapsed / calls).toFixed(4) + ","
                     + visual.snakes.length + ","
                     + visual.food.length + ","
                     + (visual.snakes.length * visual.food.length))
        compare(visual.food.length, 400)
        verify(elapsed > 0)
    }

    function test_fixedBodyPlacementThroughput() {
        prepareMaturePlannerFixture()
        const calls = 1000
        const started = Date.now()
        for (let call = 0; call < calls; ++call) {
            for (let snakeIndex = 0; snakeIndex < visual.snakes.length; ++snakeIndex)
                visual.placeSegmentsOnTrail(visual.snakes[snakeIndex])
        }
        const elapsed = Date.now() - started
        console.info("SNAKE_MOVEMENT_CSV,total_ms,calls,ms_per_call,snakes,segments")
        console.info("SNAKE_MOVEMENT_CSV,"
                     + elapsed + ","
                     + calls + ","
                     + (elapsed / calls).toFixed(4) + ","
                     + visual.snakes.length + ","
                     + visual.totalLiveSegments())
        verify(elapsed > 0)
    }

    function test_fixedCollisionGridThroughput() {
        prepareMaturePlannerFixture()
        const calls = 500
        const started = Date.now()
        for (let call = 0; call < calls; ++call)
            visual.markCollisions()
        const elapsed = Date.now() - started
        console.info("SNAKE_COLLISION_CSV,total_ms,calls,ms_per_call,snakes,segments")
        console.info("SNAKE_COLLISION_CSV,"
                     + elapsed + ","
                     + calls + ","
                     + (elapsed / calls).toFixed(4) + ","
                     + visual.snakes.length + ","
                     + visual.totalLiveSegments())
        verify(elapsed > 0)
    }

    function test_fixedNearFieldSafetyThroughput() {
        prepareMaturePlannerFixture()
        visual.markCollisions()
        const calls = 500
        const started = Date.now()
        for (let call = 0; call < calls; ++call) {
            visual.simulationTime += 0.06
            for (let snakeIndex = 0; snakeIndex < visual.snakes.length; ++snakeIndex)
                visual.applyCollisionSafety(visual.snakes[snakeIndex], snakeIndex)
        }
        const elapsed = Date.now() - started
        console.info("SNAKE_SAFETY_CSV,total_ms,calls,ms_per_call,snakes,segments")
        console.info("SNAKE_SAFETY_CSV,"
                     + elapsed + ","
                     + calls + ","
                     + (elapsed / calls).toFixed(4) + ","
                     + visual.snakes.length + ","
                     + visual.totalLiveSegments())
        verify(elapsed > 0)
    }

    function test_longRunThroughputByMinute() {
        visual.initializeWorld()
        console.info("SNAKE_BENCHMARK_CSV,minute,wall_ms,ms_per_step,realtime_ratio,alive,total_segments,max_segments,food,trail_points,retained_trail_points,deaths,estimated_vertices")

        let previousDeaths = 0
        for (let minute = 1; minute <= simulatedMinutes; ++minute) {
            const started = Date.now()
            for (let step = 0; step < stepsPerMinute; ++step)
                visual.stepSimulation(1 / 30)
            const wallMilliseconds = Date.now() - started
            const stats = statistics()
            console.info("SNAKE_BENCHMARK_CSV,"
                         + minute + ","
                         + wallMilliseconds + ","
                         + (wallMilliseconds / stepsPerMinute).toFixed(4) + ","
                         + (wallMilliseconds / 60000).toFixed(4) + ","
                         + stats.alive + ","
                         + stats.totalSegments + ","
                         + stats.maximumSegments + ","
                         + stats.food + ","
                         + stats.trailPoints + ","
                         + stats.retainedTrailPoints + ","
                         + stats.deaths + ","
                         + stats.estimatedVertices)

            verify(stats.alive > 0, "all snakes were dead at minute " + minute)
            verify(stats.food <= visual.maximumFoodCount,
                   "food cap exceeded at minute " + minute)
            verify(stats.totalSegments <= visual.maximumWorldSegments(),
                   "world segment cap exceeded at minute " + minute)
            verify(stats.deaths >= previousDeaths)
            previousDeaths = stats.deaths
        }
    }

    function test_profileMatureThirtySeconds() {
        const timings = {
            food: 0,
            brains: 0,
            movement: 0,
            feeding: 0,
            collisions: 0
        }
        const steps = 30 * 30
        const started = Date.now()
        for (let step = 0; step < steps; ++step)
            profiledStep(1 / 30, timings)
        const elapsed = Date.now() - started
        const accounted = timings.food + timings.brains + timings.movement
            + timings.feeding + timings.collisions
        console.info("SNAKE_PROFILE_CSV,total_ms,food_ms,brains_ms,movement_ms,feeding_ms,collisions_ms,unaccounted_ms")
        console.info("SNAKE_PROFILE_CSV,"
                     + elapsed + ","
                     + timings.food + ","
                     + timings.brains + ","
                     + timings.movement + ","
                     + timings.feeding + ","
                     + timings.collisions + ","
                     + Math.max(0, elapsed - accounted))
        verify(elapsed > 0)
    }
}
