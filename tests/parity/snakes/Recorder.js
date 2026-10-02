// SPDX-License-Identifier: GPL-3.0-or-later
.pragma library

var tick = 0
var scenario = null
var events = []
var allEvents = []
var rngDraws = 0
var inserted = 0
var sawLock = false
var deathKillers = {}

function emit(type, value) {
    console.log("SNAKES_PARITY " + JSON.stringify({type: type, value: value}))
}
function event(value) {
    value.tick = tick
    events.push(value)
    allEvents.push(value)
}
function rounded(value) {
    if (!Number.isFinite(value)) throw new Error("non-finite mechanics value")
    return Math.round(value * 1000000) / 1000000
}

function scenarios() {
    return [
        {id: "default-wrap", ticks: 900, long: true},
        {id: "deadly-walls", ticks: 900, long: true, walls: true},
        {id: "self-collisions", ticks: 900, long: true, self: true},
        {id: "maximum-density-trails", ticks: 900, long: true,
         width: 3440, height: 1440, density: 100, trails: 100},
        {id: "head-on-equal", ticks: 4},
        {id: "head-on-length-difference", ticks: 4},
        {id: "body-hit", ticks: 8},
        {id: "self-exclusion", ticks: 2, self: true, operation: "collision-only"},
        {id: "feeding-growth-insertion", ticks: 24},
        {id: "vacuum-lock", ticks: 40, dt: 1 / 240},
        {id: "death-burst-food", ticks: 8, walls: true},
        {id: "respawn", ticks: 190, walls: true},
        {id: "wrap-seam-crossing", ticks: 8},
        {id: "wrap-seam-collision", ticks: 4},
        {id: "world-resize", ticks: 12}
    ]
}

// This is the only producer of desiredAngle. The long script is independent
// of food/hazards, and respawns use the same global tick/index phase.
function desiredAngle(t, index, snake) {
    if (scenario.long) {
        const phase = t / 30
        const sharp = Math.floor(t / 90) % 4
        return (index * 2.399963229728653 + 0.7 * Math.sin(phase * 0.8 + index)
                + sharp * Math.PI / 2)
    }
    if (scenario.id === "vacuum-lock")
        return t < 2 ? 0 : Math.PI
    if (scenario.id === "head-on-equal"
            || scenario.id === "head-on-length-difference")
        return index === 0 ? 0 : Math.PI
    if (scenario.id === "body-hit")
        return index === 0 ? 0 : -Math.PI / 2
    if (scenario.id === "wrap-seam-collision")
        return index === 0 ? 0 : Math.PI
    return 0
}

// Observational attribution only: Snakes has deathReason but no killer field.
// Return all geometrically eligible head/body owners; ambiguous killers stay
// null. Calls the engine's own swept-distance helpers, never mutates mechanics.
function killers(v, index, reason) {
    if (reason === "wall") return []
    if (reason === "self") return [index]
    const a = v.snakes[index]
    const ah = a.segments[0]
    const result = []
    for (let j = 0; j < v.snakes.length; ++j) {
        const b = v.snakes[j]
        if (j === index || !b.alive) continue
        const begin = reason === "head" ? 0 : 1
        const end = reason === "head" ? 1 : b.segments.length
        const reach = (a.radius + b.radius) * (reason === "head" ? 0.82 : 0.78)
        for (let k = begin; k < end; ++k) {
            const bh = b.segments[k]
            if (v.worldDistanceSquared(ah.x, ah.y, bh.x, bh.y) < reach * reach
                    || v.worldSegmentsDistanceSquared(
                        ah.previousX, ah.previousY, ah.x, ah.y,
                        bh.previousX, bh.previousY, bh.x, bh.y) < reach * reach) {
                result.push(j)
                break
            }
        }
    }
    return result
}

function install(v) {
    const hooks = {}
    hooks.updateSnakeBrains = function(seconds) {
        for (let i = 0; i < v.snakes.length; ++i) {
            if (v.snakes[i].alive)
                v.snakes[i].desiredAngle = v.normalizeAngle(desiredAngle(tick, i, v.snakes[i]))
        }
    }
    hooks.planSteering = function() { throw new Error("AI planner reached") }
    hooks.chooseGoal = function() { throw new Error("AI goal selection reached") }
    hooks.applyCollisionSafety = function() {}
    hooks.applyWallSafety = function() { return false }
    const originalRandom = v.parityOriginal_random
    hooks.random = function() { ++rngDraws; return originalRandom() }
    const originalMake = v.parityOriginal_makeSnake
    hooks.makeSnake = function(index) {
        const snake = originalMake(index)
        // New snakes must also obey the script on their birth/respawn tick.
        snake.desiredAngle = v.normalizeAngle(desiredAngle(tick, index, snake))
        event({type: tick === 0 ? "spawn" : "respawn", snake: index})
        return snake
    }
    const originalEat = v.parityOriginal_consumeFoodParticle
    hooks.consumeFoodParticle = function(eater, particle, index) {
        event({type: "eat", snake: v.snakes.indexOf(eater), food: particle.id,
               value: rounded(particle.value)})
        originalEat(eater, particle, index)
    }
    const originalCollision = v.parityOriginal_markCollisions
    hooks.markCollisions = function() {
        originalCollision()
        deathKillers = {}
        for (let i = 0; i < v.snakes.length; ++i) {
            if (v.snakes[i].alive && v.snakes[i].dying)
                deathKillers[i] = killers(v, i, v.snakes[i].deathReason)
        }
    }
    const originalExplode = v.parityOriginal_explodeSnake
    hooks.explodeSnake = function(snake) {
        const index = v.snakes.indexOf(snake)
        const candidates = deathKillers[index] || []
        const reason = snake.deathReason
        const count = originalExplode(snake)
        event({type: "death", snake: index, reason: reason,
               killer: candidates.length === 1 ? candidates[0] : null,
               killerCandidates: candidates, emittedFood: count})
        return count
    }
    const originalMove = v.parityOriginal_moveSnake
    hooks.moveSnake = function(snake, index, seconds) {
        const length = snake.segments.length
        const neck = snake.segments[1]
        const tail = snake.segments[length - 1]
        originalMove(snake, index, seconds)
        if (snake.segments.length > length) {
            if (snake.segments[2] !== neck || snake.segments[snake.segments.length - 1] !== tail)
                throw new Error("growth was not inserted behind the head")
            ++inserted
            event({type: "growth", snake: index, length: snake.segments.length})
        }
    }
    v.parityHooks = hooks
}

function line(v, snake, x, y, angle, length) {
    snake.baseRadius = 6
    snake.radius = 6
    snake.birthLength = length
    snake.speedBias = 1
    snake.angle = angle
    snake.segments = []
    for (let i = 0; i < length; ++i) {
        const px = v.wrapCoordinate(x - Math.cos(angle) * 7.08 * i, v.worldWidth)
        const py = v.wrapCoordinate(y - Math.sin(angle) * 7.08 * i, v.worldHeight)
        snake.segments.push({x: px, y: py, previousX: px, previousY: py})
    }
    v.rebuildSnakeTrail(snake)
}

function setup(v) {
    if (scenario.long) return
    // Reuse real makeSnake records, so every hidden mechanics field is present.
    const count = scenario.id.indexOf("head-on") === 0 || scenario.id === "body-hit"
                  || scenario.id === "wrap-seam-collision" ? 2 : 1
    v.snakes = v.snakes.slice(0, count)
    v.food = []
    for (let i = 0; i < count; ++i)
        line(v, v.snakes[i], 500 + i * 20, 350, i ? Math.PI : 0,
             scenario.id === "head-on-length-difference" && i === 0 ? 24 : 20)
    if (scenario.id === "body-hit")
        line(v, v.snakes[1], 520, 250, -Math.PI / 2, 24)
    if (scenario.id === "self-exclusion") {
        const s = v.snakes[0]
        for (let i = 1; i < 10; ++i)
            s.segments[i] = {x: 500, y: 350, previousX: 500, previousY: 350}
    }
    if (scenario.id === "feeding-growth-insertion")
        v.addFood(504, 350, 2.2, 0, 0, 0, 40)
    if (scenario.id === "vacuum-lock")
        v.addFood(500 + 6 * 2.7, 350, 1, 0, 0, 0, 37)
    if (scenario.id === "death-burst-food" || scenario.id === "respawn")
        line(v, v.snakes[0], v.worldWidth - 1, 350, 0, 20)
    if (scenario.id === "wrap-seam-crossing")
        line(v, v.snakes[0], v.worldWidth - 1, 350, 0, 20)
    if (scenario.id === "wrap-seam-collision") {
        line(v, v.snakes[0], v.worldWidth - 5, 350, 0, 20)
        line(v, v.snakes[1], 5, 350, Math.PI, 20)
    }
    if (scenario.id === "world-resize")
        v.addFood(800, 400, 1, 0, 30, -10, 40)
}

function frame(v) {
    const snakes = []
    for (let i = 0; i < v.snakes.length; ++i) {
        const s = v.snakes[i]
        snakes.push({index: i, alive: s.alive, angle: rounded(s.angle),
                     desiredAngle: rounded(s.desiredAngle), radius: rounded(s.radius),
                     length: s.segments.length, growth: rounded(s.growth),
                     growthStretch: rounded(s.growthStretch), respawn: rounded(s.respawn),
                     segments: s.segments.map(function(p) { return [rounded(p.x), rounded(p.y)] })})
    }
    const food = v.food.map(function(p) {
        if (p.vacuumOwner >= 0) sawLock = true
        return [p.id, rounded(p.x), rounded(p.y), rounded(p.size), p.vacuumOwner,
                rounded(p.attraction)]
    })
    emit("frame", {tick: tick, simulationTime: rounded(v.simulationTime),
                   rngState: v.randomState, rngDraws: rngDraws,
                   world: [v.worldWidth, v.worldHeight], nextFoodId: v.nextFoodId,
                   nextFeastId: v.nextFeastId, snakes: snakes, food: food, events: events})
    events = []
}

function run(v, c, spec, test) {
    scenario = spec
    tick = 0
    rngDraws = 0
    events = []
    allEvents = []
    inserted = 0
    sawLock = false
    c.animationDensity = spec.density || 50
    c.trailAmount = spec.trails || 35
    c.virtualWidth = spec.width || 1280
    c.virtualHeight = spec.height || 720
    c.snakeSelfCollisions = spec.self || false
    c.snakeDeadlyWalls = spec.walls || false
    v.initializeWorld()
    setup(v)
    // Discard initialization spawns of records deliberately removed by setup.
    events = events.filter(function(e) { return e.snake < v.snakes.length })
    allEvents = events.slice()
    v.updateSnakeBrains(0)
    const dt = spec.operation === "collision-only" ? 0 : (spec.dt || 1 / 30)
    emit("begin", {version: 1, id: spec.id, seed: 1,
                   world: [v.worldWidth, v.worldHeight], ticks: spec.ticks,
                   dt: dt, script: spec.long ? "weave-quarter-turn-v1" : "focused-table-v1",
                   operation: spec.operation || "stepSimulation",
                   config: {animationSpeed: c.animationSpeed,
                            animationDensity: c.animationDensity,
                            animationScale: c.animationScale, animationPalette: c.animationPalette,
                            trailAmount: c.trailAmount, snakeIntelligence: c.snakeIntelligence,
                            snakeSelfCollisions: c.snakeSelfCollisions,
                            snakeDeadlyWalls: c.snakeDeadlyWalls, monitorBehavior: c.monitorBehavior},
                   initialState: JSON.parse(v.simulationSnapshot())})
    frame(v)
    for (tick = 1; tick <= spec.ticks; ++tick) {
        if (spec.id === "vacuum-lock" && tick === 2) {
            const head = v.snakes[0].segments[0]
            test.compare(v.food[0].vacuumOwner, 0)
            head.x = 540
            head.y = 370
            test.verify(v.worldDistanceSquared(head.x, head.y, v.food[0].x, v.food[0].y)
                        > Math.pow(v.foodCaptureRadius(v.snakes[0], v.food[0]), 2))
            event({type: "setHead", snake: 0, position: [540, 370]})
        }
        if (spec.id === "world-resize" && tick === 5) {
            c.virtualWidth = 1920
            c.virtualHeight = 900
            v.synchronizeWorldGeometry()
            event({type: "resize", world: [1920, 900]})
        }
        if (spec.operation === "collision-only") {
            if (tick === 2) {
                const s = v.snakes[0]
                s.segments[10] = {x: 500, y: 350, previousX: 500, previousY: 350}
                event({type: "setSegment", snake: 0, segment: 10, position: [500, 350]})
            }
            v.updateSnakeBrains(0)
            v.markCollisions()
            if (tick === 1) test.verify(!v.snakes[0].dying, "segments 1..9 excluded")
            if (tick === 2) test.compare(v.snakes[0].deathReason, "self")
            for (let j = 0; j < v.snakes.length; ++j)
                if (v.snakes[j].dying) v.explodeSnake(v.snakes[j])
        } else {
            v.stepSimulation(dt)
        }
        if (!spec.long || tick <= 300 || tick % 10 === 0) frame(v)
    }
    const deaths = allEvents.filter(function(e) { return e.type === "death" })
    if (spec.id === "head-on-equal" || spec.id === "wrap-seam-collision") {
        test.compare(deaths.length, 2)
        test.compare(deaths[0].reason, "head")
        test.compare(deaths[1].reason, "head")
    }
    if (spec.id === "head-on-length-difference") {
        test.compare(deaths.length, 1)
        test.compare(deaths[0].snake, 1)
        test.compare(deaths[0].reason, "head")
        test.verify(v.snakes[0].alive)
    }
    if (spec.id === "body-hit") test.verify(deaths.some(function(e) { return e.reason === "body" }))
    if (spec.id === "feeding-growth-insertion") {
        test.verify(allEvents.some(function(e) { return e.type === "eat" }))
        test.verify(inserted >= 2)
    }
    if (spec.id === "vacuum-lock") {
        test.verify(sawLock, "particle captured before being eaten")
        test.verify(allEvents.some(function(e) { return e.type === "eat" }))
    }
    if (spec.id === "respawn") test.verify(allEvents.some(function(e) { return e.type === "respawn" }))
    if (spec.id === "death-burst-food") {
        test.compare(deaths[0].reason, "wall")
        test.verify(deaths[0].emittedFood > 0)
    }
    if (spec.id === "wrap-seam-crossing") {
        test.compare(deaths.length, 0)
        test.verify(v.snakes[0].segments[0].x < 100)
    }
    if (spec.id === "world-resize") test.compare(v.initializedWidth, 1920)
    emit("end", {id: spec.id, totalEvents: allEvents.length})
}
