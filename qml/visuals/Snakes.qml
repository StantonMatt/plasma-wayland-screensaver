// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import "VisualUtils.js" as Utils

Item {
    id: root
    objectName: "snakeVisualRoot"
    clip: true

    property var context
    property var nativeRenderer: null
    property bool simulationDriver: true
    property int frameRate: 30
    property bool reducedMotion: false
    property int seed: 1
    property double animationEpochMs: Date.now()
    property var snakes: []
    property var food: []
    property real accumulator: 0
    property real renderAlpha: 0
    property real simulationTime: 0
    property double randomState: 1
    property real initializedWidth: 0
    property real initializedHeight: 0
    property int nextFeastId: 1
    property int nextFoodId: 1
    property real foodAnalysisCooldown: 0
    property int growthSlots: 0
    property int deathCount: 0
    property int wallDeathCount: 0
    property int headDeathCount: 0
    property int bodyDeathCount: 0
    property int selfDeathCount: 0
    property int brainCursor: 0
    property var safetyCells: ({})
    property int safetyCellColumns: 0
    property int safetyCellRows: 0
    property real safetyCellWidth: 1
    property real safetyCellHeight: 1
    property int safetyCellSnakeCount: 0
    property var activeBrainPlan: null
    property int lastBrainWorkUnits: 0
    readonly property int maximumBrainWorkUnits: 3

    readonly property bool seamless: context && context.monitorBehavior === "seamless"
    readonly property real worldWidth: seamless && context ? context.virtualWidth : width
    readonly property real worldHeight: seamless && context ? context.virtualHeight : height
    readonly property real drawOffsetX: seamless && context ? context.virtualX - context.screenX : 0
    readonly property real drawOffsetY: seamless && context ? context.virtualY - context.screenY : 0
    readonly property int desiredSnakeCount: context
        ? Math.max(3, Math.min(14, Math.round(3 + context.animationDensity / 9))) : 8
    readonly property int desiredFoodCount: context
        ? Math.max(28, Math.min(190, Math.round(28 + context.trailAmount * 1.55))) : 82
    readonly property real intelligence: context
        ? clamp(context.snakeIntelligence / 100, 0, 1) : 0.75
    readonly property bool selfCollisions: context ? context.snakeSelfCollisions : false
    readonly property bool deadlyWalls: context ? context.snakeDeadlyWalls : true
    readonly property real canvasScale: width * height > 3000000 ? 0.5
        : (width * height > 1500000 ? 0.67 : 1.0)
    readonly property var palette: Utils.colors(context ? context.animationPalette : "spectrum")

    function random() {
        randomState = (randomState * 1664525 + 1013904223) % 4294967296
        return randomState / 4294967296
    }

    function clamp(value, minimum, maximum) {
        return Math.max(minimum, Math.min(maximum, value))
    }

    function normalizeAngle(angle) {
        while (angle > Math.PI)
            angle -= Math.PI * 2
        while (angle < -Math.PI)
            angle += Math.PI * 2
        return angle
    }

    function distanceSquared(ax, ay, bx, by) {
        const dx = ax - bx
        const dy = ay - by
        return dx * dx + dy * dy
    }

    function wrapCoordinate(value, extent) {
        if (extent <= 0)
            return 0
        return ((value % extent) + extent) % extent
    }

    function wrappingOffsets(minimum, maximum, extent, margin) {
        if (extent <= 0)
            return [0]
        const offsets = []
        const first = Math.ceil((-margin - maximum) / extent)
        const last = Math.floor((extent + margin - minimum) / extent)
        for (let multiple = first; multiple <= last; ++multiple)
            offsets.push(multiple * extent)
        return offsets
    }

    function axisDelta(from, to, extent) {
        let delta = to - from
        if (!deadlyWalls && extent > 0) {
            if (delta > extent / 2)
                delta -= extent
            else if (delta < -extent / 2)
                delta += extent
        }
        return delta
    }

    function worldDistanceSquared(ax, ay, bx, by) {
        const dx = axisDelta(ax, bx, worldWidth)
        const dy = axisDelta(ay, by, worldHeight)
        return dx * dx + dy * dy
    }

    // Distance from a point to a body capsule centreline. Coordinates are
    // unfolded around the point first, so the same calculation is correct for
    // both deadly walls and wraparound arenas.
    function worldSegmentDistanceSquared(px, py, ax, ay, bx, by) {
        const localAx = px + axisDelta(px, ax, worldWidth)
        const localAy = py + axisDelta(py, ay, worldHeight)
        const segmentX = axisDelta(ax, bx, worldWidth)
        const segmentY = axisDelta(ay, by, worldHeight)
        const lengthSquared = segmentX * segmentX + segmentY * segmentY
        if (lengthSquared < 0.0001)
            return distanceSquared(px, py, localAx, localAy)
        const projection = clamp(((px - localAx) * segmentX
                                  + (py - localAy) * segmentY) / lengthSquared, 0, 1)
        const closestX = localAx + segmentX * projection
        const closestY = localAy + segmentY * projection
        return distanceSquared(px, py, closestX, closestY)
    }

    function planarPointSegmentDistanceSquared(px, py, ax, ay, bx, by) {
        const segmentX = bx - ax
        const segmentY = by - ay
        const lengthSquared = segmentX * segmentX + segmentY * segmentY
        if (lengthSquared < 0.0001)
            return distanceSquared(px, py, ax, ay)
        const projection = clamp(((px - ax) * segmentX + (py - ay) * segmentY)
                                 / lengthSquared, 0, 1)
        return distanceSquared(px, py,
                               ax + segmentX * projection,
                               ay + segmentY * projection)
    }

    function crossProduct(ax, ay, bx, by, cx, cy) {
        return (bx - ax) * (cy - ay) - (by - ay) * (cx - ax)
    }

    function pointOnPlanarSegment(px, py, ax, ay, bx, by) {
        const epsilon = 0.0001
        return px >= Math.min(ax, bx) - epsilon && px <= Math.max(ax, bx) + epsilon
            && py >= Math.min(ay, by) - epsilon && py <= Math.max(ay, by) + epsilon
    }

    // Shortest distance between two continuous centreline segments. This is
    // what prevents a fast projected head from tunnelling between rollout
    // samples or through a coalesced body capsule.
    function worldSegmentsDistanceSquared(ax, ay, bx, by, cx, cy, dx, dy) {
        const localBx = ax + axisDelta(ax, bx, worldWidth)
        const localBy = ay + axisDelta(ay, by, worldHeight)
        const localCx = ax + axisDelta(ax, cx, worldWidth)
        const localCy = ay + axisDelta(ay, cy, worldHeight)
        const localDx = localCx + axisDelta(cx, dx, worldWidth)
        const localDy = localCy + axisDelta(cy, dy, worldHeight)
        const firstCross = crossProduct(ax, ay, localBx, localBy, localCx, localCy)
        const secondCross = crossProduct(ax, ay, localBx, localBy, localDx, localDy)
        const thirdCross = crossProduct(localCx, localCy, localDx, localDy, ax, ay)
        const fourthCross = crossProduct(localCx, localCy, localDx, localDy,
                                         localBx, localBy)
        const properIntersection = firstCross * secondCross < 0
                                   && thirdCross * fourthCross < 0
        const epsilon = 0.0001
        const touching = (Math.abs(firstCross) < epsilon
                          && pointOnPlanarSegment(localCx, localCy,
                                                  ax, ay, localBx, localBy))
            || (Math.abs(secondCross) < epsilon
                && pointOnPlanarSegment(localDx, localDy,
                                        ax, ay, localBx, localBy))
            || (Math.abs(thirdCross) < epsilon
                && pointOnPlanarSegment(ax, ay,
                                        localCx, localCy, localDx, localDy))
            || (Math.abs(fourthCross) < epsilon
                && pointOnPlanarSegment(localBx, localBy,
                                        localCx, localCy, localDx, localDy))
        if (properIntersection || touching)
            return 0
        return Math.min(
            planarPointSegmentDistanceSquared(ax, ay,
                                               localCx, localCy, localDx, localDy),
            planarPointSegmentDistanceSquared(localBx, localBy,
                                               localCx, localCy, localDx, localDy),
            planarPointSegmentDistanceSquared(localCx, localCy,
                                               ax, ay, localBx, localBy),
            planarPointSegmentDistanceSquared(localDx, localDy,
                                               ax, ay, localBx, localBy))
    }

    function baseRadius() {
        if (!context)
            return 8
        const scaled = Math.min(worldWidth, worldHeight) * 0.0075
                       * context.animationScale / 100
        return clamp(scaled, 4.5, 18)
    }

    // Limit length by painted screen area rather than an arbitrary segment
    // count. One champion may occupy about 7.5% of the arena, while the whole
    // ecosystem shares a larger 22% budget. Absolute guards keep pathological
    // display geometries from creating unreasonable CPU/GPU loads.
    function maximumSnakeSegments(snake) {
        const paintedAreaPerSegment = Math.max(1, snake.radius * snake.radius * 2.35)
        const areaLimit = Math.floor(worldWidth * worldHeight * 0.075
                                     / paintedAreaPerSegment)
        return Math.round(clamp(areaLimit, 400, 1600))
    }

    function maximumWorldSegments() {
        // Reserve for the thickest possible randomized snake at its 25%
        // thickness ceiling so the ecosystem budget remains a true upper bound.
        const radius = baseRadius() * 1.14 * 1.25
        const paintedAreaPerSegment = Math.max(1, radius * radius * 2.35)
        const areaLimit = Math.floor(worldWidth * worldHeight * 0.22
                                     / paintedAreaPerSegment)
        return Math.round(clamp(areaLimit, desiredSnakeCount * 80, 6000))
    }

    function totalLiveSegments() {
        let total = 0
        for (let index = 0; index < snakes.length; ++index) {
            if (snakes[index].alive)
                total += snakes[index].segments.length
        }
        return total
    }

    // Early growth remains quick and satisfying. Past the old 120-segment
    // boundary, each new segment gradually costs more food, producing a long
    // late game instead of explosive screen-filling growth.
    function segmentGrowthCost(snake) {
        const lateSegments = Math.max(0, snake.segments.length - 120)
        return 1 + Math.pow(lateSegments / 260, 0.85)
    }

    function spawnPosition(radius, length) {
        const margin = Math.max(radius * 5, 18)
        const clearance = Math.min(Math.min(worldWidth, worldHeight) * 0.22,
                                   Math.max(170, radius * 20))
        let best = { x: worldWidth / 2, y: worldHeight / 2,
                     angle: random() * Math.PI * 2, distance: -1 }
        for (let attempt = 0; attempt < 48; ++attempt) {
            const x = margin + random() * Math.max(1, worldWidth - margin * 2)
            const y = margin + random() * Math.max(1, worldHeight - margin * 2)
            const angle = random() * Math.PI * 2
            let minimumDistanceSquared = Number.MAX_VALUE
            // Check both the proposed head and the line occupied by its body,
            // so a respawn cannot materialize beside or through a living snake.
            const samples = Math.min(7, Math.max(2, Math.ceil(length / 4)))
            for (let sample = 0; sample < samples; ++sample) {
                const bodyDistance = radius * 1.18 * sample * (length - 1)
                                     / Math.max(1, samples - 1)
                const sampleX = x - Math.cos(angle) * bodyDistance
                const sampleY = y - Math.sin(angle) * bodyDistance
                if (deadlyWalls && (sampleX < margin || sampleX > worldWidth - margin
                        || sampleY < margin || sampleY > worldHeight - margin)) {
                    minimumDistanceSquared = 0
                    break
                }
                for (let snakeIndex = 0; snakeIndex < snakes.length; ++snakeIndex) {
                    const other = snakes[snakeIndex]
                    if (!other.alive)
                        continue
                    for (let segmentIndex = 0; segmentIndex < other.segments.length;
                            segmentIndex += 2) {
                        const segment = other.segments[segmentIndex]
                        minimumDistanceSquared = Math.min(minimumDistanceSquared,
                            worldDistanceSquared(sampleX, sampleY, segment.x, segment.y))
                    }
                }
            }
            if (minimumDistanceSquared > best.distance) {
                best = { x: x, y: y, angle: angle,
                         distance: minimumDistanceSquared }
            }
            if (minimumDistanceSquared >= clearance * clearance)
                break
        }
        return best
    }

    function makeSnake(index) {
        const radius = baseRadius() * (0.86 + random() * 0.28)
        const length = 14 + Math.floor(random() * 12) + (index === 0 ? 7 : 0)
        const spawn = spawnPosition(radius, length)
        const x = spawn.x
        const y = spawn.y
        const angle = spawn.angle
        const spacing = radius * 1.18
        const segments = []
        for (let segment = 0; segment < length; ++segment) {
            segments.push({
                x: x - Math.cos(angle) * spacing * segment,
                y: y - Math.sin(angle) * spacing * segment,
                previousX: x - Math.cos(angle) * spacing * segment,
                previousY: y - Math.sin(angle) * spacing * segment
            })
        }
        const snake = {
            alive: true,
            respawn: 0,
            segments: segments,
            angle: angle,
            baseRadius: radius,
            radius: radius,
            birthLength: length,
            colorIndex: index % palette.length,
            speedBias: 0.86 + random() * 0.28,
            turnBias: (random() - 0.5) * 0.35,
            wanderPhase: random() * Math.PI * 2,
            aggression: random(),
            growth: 0,
            growthStretch: 0,
            growthBlocked: false,
            rush: 0,
            feastId: 0,
            feastTargetIndex: -1,
            feastDirection: 1,
            foodTargetId: 0,
            foodTargetUntil: 0,
            foodPlanUntil: 0,
            plannedGoalX: x,
            plannedGoalY: y,
            foodPathIds: [],
            foodPathUntil: 0,
            desiredAngle: angle,
            brainCooldown: random() * 0.06,
            brainPlanning: false,
            nextSafetyCheck: 0,
            safetyActiveUntil: 0,
            safetyDesiredAngle: angle,
            avoidanceSide: 0,
            avoidanceCommitUntil: 0,
            score: length
        }
        rebuildSnakeTrail(snake)
        return snake
    }

    // Keep an arc-length history of the route travelled by the head. Body
    // points are sampled from this history rather than pulled directly toward
    // their neighbours; the latter makes the whole tail slide sideways as a
    // turn propagates through the constraint chain.
    function rebuildSnakeTrail(snake) {
        const segments = snake.segments
        if (!segments || segments.length === 0) {
            snake.trailPoints = []
            snake.trailStart = 0
            return
        }

        const headToTail = [{ x: segments[0].x, y: segments[0].y }]
        for (let index = 1; index < segments.length; ++index) {
            const previous = segments[index - 1]
            const point = headToTail[index - 1]
            headToTail.push({
                x: point.x + axisDelta(previous.x, segments[index].x, worldWidth),
                y: point.y + axisDelta(previous.y, segments[index].y, worldHeight)
            })
        }

        const tail = headToTail[headToTail.length - 1]
        const beforeTail = headToTail.length > 1
            ? headToTail[headToTail.length - 2] : null
        let tailDirectionX = beforeTail ? tail.x - beforeTail.x : -Math.cos(snake.angle)
        let tailDirectionY = beforeTail ? tail.y - beforeTail.y : -Math.sin(snake.angle)
        const directionLength = Math.sqrt(tailDirectionX * tailDirectionX
                                          + tailDirectionY * tailDirectionY)
        if (directionLength > 0.001) {
            tailDirectionX /= directionLength
            tailDirectionY /= directionLength
        } else {
            tailDirectionX = -Math.cos(snake.angle)
            tailDirectionY = -Math.sin(snake.angle)
        }

        // A short reserve behind the tail lets forward growth stretch the
        // neck without ever running beyond the recorded route.
        const spacing = snake.radius * 1.18
        for (let reserve = 1; reserve <= 4; ++reserve) {
            headToTail.push({
                x: tail.x + tailDirectionX * spacing * reserve,
                y: tail.y + tailDirectionY * spacing * reserve
            })
        }
        headToTail.reverse()
        let distance = 0
        for (let index = 0; index < headToTail.length; ++index) {
            if (index > 0) {
                const dx = headToTail[index].x - headToTail[index - 1].x
                const dy = headToTail[index].y - headToTail[index - 1].y
                distance += Math.sqrt(dx * dx + dy * dy)
            }
            headToTail[index].distance = distance
        }
        snake.trailPoints = headToTail
        snake.trailStart = 0
    }

    function ensureSnakeTrail(snake) {
        const points = snake.trailPoints
        if (!points || points.length < 2) {
            rebuildSnakeTrail(snake)
            return
        }
        const latest = points[points.length - 1]
        const head = snake.segments[0]
        const latestX = deadlyWalls ? latest.x : wrapCoordinate(latest.x, worldWidth)
        const latestY = deadlyWalls ? latest.y : wrapCoordinate(latest.y, worldHeight)
        if (worldDistanceSquared(latestX, latestY, head.x, head.y) > 0.01)
            rebuildSnakeTrail(snake)
    }

    function appendHeadTrailPoint(snake) {
        const points = snake.trailPoints
        const latest = points[points.length - 1]
        const head = snake.segments[0]
        const latestWrappedX = deadlyWalls ? latest.x : wrapCoordinate(latest.x, worldWidth)
        const latestWrappedY = deadlyWalls ? latest.y : wrapCoordinate(latest.y, worldHeight)
        const x = latest.x + axisDelta(latestWrappedX, head.x, worldWidth)
        const y = latest.y + axisDelta(latestWrappedY, head.y, worldHeight)
        const dx = x - latest.x
        const dy = y - latest.y
        const travelled = Math.sqrt(dx * dx + dy * dy)
        if (travelled > 0.0001)
            points.push({ x: x, y: y, distance: latest.distance + travelled })
    }

    function placeSegmentsOnTrail(snake) {
        const points = snake.trailPoints
        if (!points || points.length < 2 || snake.segments.length < 2)
            return
        const newest = points[points.length - 1]
        const spacing = snake.radius * 1.18
        const stretch = snake.growthStretch || 0
        let cursor = points.length - 2
        const start = snake.trailStart || 0

        for (let segmentIndex = 1; segmentIndex < snake.segments.length; ++segmentIndex) {
            const targetDistance = newest.distance
                - spacing * (segmentIndex + stretch)
            while (cursor > start && points[cursor].distance > targetDistance)
                --cursor
            const older = points[cursor]
            const newer = points[Math.min(cursor + 1, points.length - 1)]
            const span = Math.max(0.0001, newer.distance - older.distance)
            const amount = clamp((targetDistance - older.distance) / span, 0, 1)
            const x = older.x + (newer.x - older.x) * amount
            const y = older.y + (newer.y - older.y) * amount
            const segment = snake.segments[segmentIndex]
            segment.x = deadlyWalls ? x : wrapCoordinate(x, worldWidth)
            segment.y = deadlyWalls ? y : wrapCoordinate(y, worldHeight)
        }

        const keepAfter = newest.distance
            - spacing * (snake.segments.length + 4)
        while (snake.trailStart + 1 < points.length
               && points[snake.trailStart + 1].distance < keepAfter) {
            ++snake.trailStart
        }
        if (snake.trailStart > 512 && snake.trailStart > points.length / 2) {
            snake.trailPoints = points.slice(snake.trailStart)
            snake.trailStart = 0
        }
    }

    function addFood(x, y, value, colorIndex, vx, vy, life, feastId, trailIndex,
                     feastLength) {
        food.push({
            id: nextFoodId++,
            x: deadlyWalls ? clamp(x, 3, Math.max(3, worldWidth - 3))
                           : wrapCoordinate(x, worldWidth),
            y: deadlyWalls ? clamp(y, 3, Math.max(3, worldHeight - 3))
                           : wrapCoordinate(y, worldHeight),
            value: value,
            colorIndex: colorIndex,
            size: baseRadius() * (0.23 + Math.min(1.4, value) * 0.13),
            vx: vx,
            vy: vy,
            life: life,
            phase: random() * Math.PI * 2,
            feastId: feastId === undefined ? 0 : feastId,
            trailIndex: trailIndex === undefined ? -1 : trailIndex,
            feastLength: feastLength === undefined ? 0 : feastLength,
            clusterValue: value,
            claimedBy: -1,
            claimedUntil: 0,
            attraction: 0,
            attractionX: x,
            attractionY: y,
            vacuumOwner: -1,
            vacuumOriginalLife: life
        })
    }

    function addAmbientFood() {
        const margin = Math.max(12, baseRadius() * 2)
        addFood(margin + random() * Math.max(1, worldWidth - margin * 2),
                margin + random() * Math.max(1, worldHeight - margin * 2),
                0.48 + random() * 0.3, Math.floor(random() * palette.length),
                0, 0, 34 + random() * 12)
    }

    function requestFrame() {
        if (nativeRenderer) {
            nativeRenderer.syncFrame(snakes, food, palette, simulationTime,
                                     renderAlpha, worldWidth, worldHeight,
                                     drawOffsetX, drawOffsetY, deadlyWalls)
        } else {
            snakeCanvas.requestPaint()
        }
    }

    function simulationSnapshot(): string {
        return JSON.stringify({
            version: 1,
            worldWidth: worldWidth,
            worldHeight: worldHeight,
            snakes: snakes,
            food: food,
            accumulator: accumulator,
            renderAlpha: renderAlpha,
            simulationTime: simulationTime,
            randomState: randomState,
            nextFeastId: nextFeastId,
            nextFoodId: nextFoodId,
            foodAnalysisCooldown: foodAnalysisCooldown,
            growthSlots: growthSlots,
            deathCount: deathCount,
            brainCursor: brainCursor
        })
    }

    function cancelActiveBrainPlan() {
        if (activeBrainPlan && activeBrainPlan.snake)
            activeBrainPlan.snake.brainPlanning = false
        activeBrainPlan = null
    }

    function resizeSimulationWorld(sourceWidth, sourceHeight) {
        if (sourceWidth <= 0 || sourceHeight <= 0 || worldWidth <= 0 || worldHeight <= 0) {
            initializeWorld()
            return
        }
        cancelActiveBrainPlan()
        const scaleX = worldWidth / sourceWidth
        const scaleY = worldHeight / sourceHeight
        for (let snakeIndex = 0; snakeIndex < snakes.length; ++snakeIndex) {
            const snake = snakes[snakeIndex]
            for (let segmentIndex = 0; segmentIndex < snake.segments.length; ++segmentIndex) {
                const segment = snake.segments[segmentIndex]
                segment.x *= scaleX
                segment.y *= scaleY
                segment.previousX *= scaleX
                segment.previousY *= scaleY
            }
            if (snake.trailPoints) {
                let trailDistance = 0
                for (let trailIndex = 0; trailIndex < snake.trailPoints.length;
                        ++trailIndex) {
                    const point = snake.trailPoints[trailIndex]
                    point.x *= scaleX
                    point.y *= scaleY
                    if (trailIndex > 0) {
                        const previousPoint = snake.trailPoints[trailIndex - 1]
                        const dx = point.x - previousPoint.x
                        const dy = point.y - previousPoint.y
                        trailDistance += Math.sqrt(dx * dx + dy * dy)
                    }
                    point.distance = trailDistance
                }
            }
            snake.plannedGoalX *= scaleX
            snake.plannedGoalY *= scaleY
        }
        for (let foodIndex = 0; foodIndex < food.length; ++foodIndex) {
            const particle = food[foodIndex]
            particle.x *= scaleX
            particle.y *= scaleY
            particle.vx *= scaleX
            particle.vy *= scaleY
            particle.attractionX *= scaleX
            particle.attractionY *= scaleY
        }
        initializedWidth = worldWidth
        initializedHeight = worldHeight
        growthSlots = Math.max(0, maximumWorldSegments() - totalLiveSegments())
        requestFrame()
    }

    function synchronizeWorldGeometry() {
        if (snakes.length === 0) {
            initializeWorld()
        } else if (Math.abs(worldWidth - initializedWidth) > 1
                   || Math.abs(worldHeight - initializedHeight) > 1) {
            resizeSimulationWorld(initializedWidth, initializedHeight)
        }
    }

    function restoreSimulationSnapshot(snapshot: string): bool {
        let state
        try {
            state = JSON.parse(snapshot)
        } catch (error) {
            return false
        }
        if (!state || state.version !== 1 || !Array.isArray(state.snakes)
                || !Array.isArray(state.food)) {
            return false
        }

        initializeTimer.stop()
        snakes = state.snakes
        activeBrainPlan = null
        for (let snakeIndex = 0; snakeIndex < snakes.length; ++snakeIndex) {
            snakes[snakeIndex].brainPlanning = false
            if (!snakes[snakeIndex].trailPoints)
                rebuildSnakeTrail(snakes[snakeIndex])
        }
        food = state.food
        accumulator = state.accumulator
        renderAlpha = state.renderAlpha
        simulationTime = state.simulationTime
        randomState = state.randomState
        nextFeastId = state.nextFeastId
        nextFoodId = state.nextFoodId
        foodAnalysisCooldown = state.foodAnalysisCooldown
        growthSlots = state.growthSlots
        deathCount = state.deathCount === undefined ? 0 : state.deathCount
        brainCursor = state.brainCursor === undefined ? 0 : state.brainCursor
        initializedWidth = state.worldWidth
        initializedHeight = state.worldHeight
        resizeSimulationWorld(state.worldWidth, state.worldHeight)
        return true
    }

    function initializeWorld() {
        if (!context || worldWidth < 80 || worldHeight < 80)
            return
        randomState = (Math.abs(seed) + 1) * 2654435761 % 4294967296
        if (randomState < 1)
            randomState = 1
        accumulator = 0
        simulationTime = 0
        nextFeastId = 1
        nextFoodId = 1
        foodAnalysisCooldown = 0
        deathCount = 0
        wallDeathCount = 0
        headDeathCount = 0
        bodyDeathCount = 0
        selfDeathCount = 0
        brainCursor = 0
        activeBrainPlan = null
        lastBrainWorkUnits = 0
        safetyCells = ({})
        safetyCellColumns = 0
        safetyCellRows = 0
        safetyCellSnakeCount = 0
        snakes = []
        food = []
        for (let i = 0; i < desiredSnakeCount; ++i)
            snakes.push(makeSnake(i))
        growthSlots = Math.max(0, maximumWorldSegments() - totalLiveSegments())
        for (let particle = 0; particle < desiredFoodCount; ++particle)
            addAmbientFood()
        initializedWidth = worldWidth
        initializedHeight = worldHeight
        requestFrame()
    }

    function analyzeFoodClusters() {
        const range = Math.max(54, baseRadius() * 10)
        const cellSize = range
        const cellColumns = Math.max(1, Math.ceil(worldWidth / cellSize))
        const cellRows = Math.max(1, Math.ceil(worldHeight / cellSize))
        const cells = {}
        for (let index = 0; index < food.length; ++index) {
            const particle = food[index]
            particle.clusterValue = particle.value
            const key = Math.floor(particle.x / cellSize) + ":"
                        + Math.floor(particle.y / cellSize)
            if (!cells[key])
                cells[key] = []
            cells[key].push(index)
        }
        for (let index = 0; index < food.length; ++index) {
            const particle = food[index]
            const cellX = Math.floor(particle.x / cellSize)
            const cellY = Math.floor(particle.y / cellSize)
            let clusterValue = particle.value
            const visitedCells = {}
            for (let offsetX = -1; offsetX <= 1; ++offsetX) {
                for (let offsetY = -1; offsetY <= 1; ++offsetY) {
                    let nearbyX = cellX + offsetX
                    let nearbyY = cellY + offsetY
                    if (!deadlyWalls) {
                        nearbyX = ((nearbyX % cellColumns) + cellColumns) % cellColumns
                        nearbyY = ((nearbyY % cellRows) + cellRows) % cellRows
                    } else if (nearbyX < 0 || nearbyX >= cellColumns
                               || nearbyY < 0 || nearbyY >= cellRows) {
                        continue
                    }
                    const key = nearbyX + ":" + nearbyY
                    if (visitedCells[key])
                        continue
                    visitedCells[key] = true
                    const nearby = cells[key] || []
                    for (let nearbyIndex = 0; nearbyIndex < nearby.length; ++nearbyIndex) {
                        const otherIndex = nearby[nearbyIndex]
                        if (otherIndex === index)
                            continue
                        const other = food[otherIndex]
                        const distance = Math.sqrt(worldDistanceSquared(
                            particle.x, particle.y, other.x, other.y))
                        if (distance < range)
                            clusterValue += other.value * (1 - distance / range)
                    }
                }
            }
            particle.clusterValue = clusterValue
        }
    }

    function foodCaptureRadius(snake, particle) {
        return snake.radius * 3 + particle.size
    }

    function deathFoodMultiplier(particle) {
        if (!particle.feastId || particle.feastId <= 0)
            return 1
        return 1 + intelligence * clamp(particle.feastLength / 90, 0.5, 4)
    }

    // Estimate the food collected without needing a pixel-perfect hit. Every
    // particle inside the snake's actual vacuum corridor contributes to the
    // route, so a line through a clump beats merely aiming at its nearest edge.
    function routeHarvestValue(snake, target) {
        const head = snake.segments[0]
        const routeX = axisDelta(head.x, target.x, worldWidth)
        const routeY = axisDelta(head.y, target.y, worldHeight)
        const routeLength = Math.max(0.001, Math.sqrt(routeX * routeX + routeY * routeY))
        const directionX = routeX / routeLength
        const directionY = routeY / routeLength
        let harvest = 0
        for (let index = 0; index < food.length; ++index) {
            const particle = food[index]
            const particleX = axisDelta(head.x, particle.x, worldWidth)
            const particleY = axisDelta(head.y, particle.y, worldHeight)
            const projection = particleX * directionX + particleY * directionY
            const captureRadius = foodCaptureRadius(snake, particle)
            if (projection < -captureRadius * 0.25
                    || projection > routeLength + captureRadius * 1.5)
                continue
            const lateralDistance = Math.abs(particleX * directionY
                                             - particleY * directionX)
            if (lateralDistance > captureRadius)
                continue
            const corridorWeight = 0.3 + 0.7
                * (1 - lateralDistance / Math.max(1, captureRadius))
            harvest += particle.value * corridorWeight
                       * deathFoodMultiplier(particle)
        }
        return harvest
    }

    function insertRouteCandidate(candidates, particle, score, limit) {
        let position = 0
        while (position < candidates.length && candidates[position].score <= score)
            ++position
        if (position >= limit)
            return
        candidates.splice(position, 0, { particle: particle, score: score })
        if (candidates.length > limit)
            candidates.pop()
    }

    function snakeTurnRate(snake) {
        return 2.05 + intelligence * 1.8
               + 20 / Math.max(8, snake.segments.length)
    }

    function turnReachabilityPenaltyFrom(snake, fromX, fromY, heading, particle) {
        const dx = axisDelta(fromX, particle.x, worldWidth)
        const dy = axisDelta(fromY, particle.y, worldHeight)
        const distance = Math.max(0.001, Math.sqrt(dx * dx + dy * dy))
        const difference = Math.abs(normalizeAngle(Math.atan2(dy, dx) - heading))
        const turnRadius = snakeSpeed(snake) / Math.max(0.1, snakeTurnRate(snake))
        const captureRadius = foodCaptureRadius(snake, particle)
        const requiredArc = difference * turnRadius
        const shortfall = Math.max(0, requiredArc - distance - captureRadius)
        let penalty = 1 + shortfall / Math.max(captureRadius, turnRadius * 0.25) * 4
        if (difference > Math.PI * 0.72 && distance < turnRadius * 1.7)
            penalty += 5
        return penalty
    }

    function turnReachabilityPenalty(snake, particle) {
        const head = snake.segments[0]
        return turnReachabilityPenaltyFrom(snake, head.x, head.y,
                                           snake.angle, particle)
    }

    function foodApproachGoal(snake, particle) {
        const penalty = turnReachabilityPenalty(snake, particle)
        if (penalty <= 1.15)
            return { x: particle.x, y: particle.y, direct: true }
        const head = snake.segments[0]
        const dx = axisDelta(head.x, particle.x, worldWidth)
        const dy = axisDelta(head.y, particle.y, worldHeight)
        const difference = normalizeAngle(Math.atan2(dy, dx) - snake.angle)
        let direction = difference >= 0 ? 1 : -1
        if (Math.abs(Math.abs(difference) - Math.PI) < 0.08)
            direction = snake.turnBias >= 0 ? 1 : -1
        const forwardX = Math.cos(snake.angle)
        const forwardY = Math.sin(snake.angle)
        const sideX = -forwardY * direction
        const sideY = forwardX * direction
        const turnRadius = snakeSpeed(snake) / Math.max(0.1, snakeTurnRate(snake))
        const forwardDistance = Math.max(turnRadius * 1.2,
                                         foodCaptureRadius(snake, particle) * 1.5)
        let goalX = head.x + forwardX * forwardDistance + sideX * turnRadius * 1.15
        let goalY = head.y + forwardY * forwardDistance + sideY * turnRadius * 1.15
        if (deadlyWalls) {
            const margin = snake.radius * 4
            goalX = clamp(goalX, margin, worldWidth - margin)
            goalY = clamp(goalY, margin, worldHeight - margin)
        } else {
            goalX = wrapCoordinate(goalX, worldWidth)
            goalY = wrapCoordinate(goalY, worldHeight)
        }
        return { x: goalX, y: goalY, direct: false }
    }

    function foodById(id) {
        for (let index = 0; index < food.length; ++index) {
            if (food[index].id === id)
                return food[index]
        }
        return null
    }

    function nextFoodPathParticle(snake) {
        while (snake.foodPathIds && snake.foodPathIds.length > 0) {
            const particle = foodById(snake.foodPathIds[0])
            if (!particle) {
                snake.foodPathIds.shift()
                continue
            }
            // Once the vacuum has secured a waypoint, look through it to the
            // next particle. Steering at the centre of every speck causes
            // needless zig-zags and makes a fast snake double back after food
            // that is already travelling toward its mouth.
            if (snake.foodPathIds.length > 1
                    && worldDistanceSquared(snake.segments[0].x,
                                            snake.segments[0].y,
                                            particle.x, particle.y)
                       < Math.pow(foodCaptureRadius(snake, particle) * 0.9, 2)) {
                snake.foodPathIds.shift()
                continue
            }
            // If a missed waypoint has moved inside the minimum turning circle,
            // advance to the next leg rather than orbiting it forever.
            if (snake.foodPathIds.length > 1
                    && turnReachabilityPenalty(snake, particle) > 4.5) {
                snake.foodPathIds.shift()
                continue
            }
            return particle
        }
        return null
    }

    // Once a valuable region is selected, trace an explicit polyline through
    // it: closest reachable particle from the head, then the closest profitable
    // particle from that waypoint, and so on. Cluster value only biases close
    // choices; it cannot make the route jump randomly across the region.
    function buildFoodPath(snake, anchor) {
        const path = []
        const selected = {}
        const regionRadius = Math.max(100, baseRadius() * 18)
        const regionRadiusSquared = regionRadius * regionRadius
        let fromX = snake.segments[0].x
        let fromY = snake.segments[0].y
        let heading = snake.angle
        for (let step = 0; step < 12; ++step) {
            let best = null
            let bestScore = Number.MAX_VALUE
            for (let index = 0; index < food.length; ++index) {
                const particle = food[index]
                if (selected[particle.id])
                    continue
                const sameDeath = anchor.feastId > 0
                    && particle.feastId === anchor.feastId
                const sameRegion = sameDeath || (anchor.feastId <= 0
                    && worldDistanceSquared(anchor.x, anchor.y,
                                            particle.x, particle.y)
                       <= regionRadiusSquared)
                if (!sameRegion)
                    continue
                const distance = Math.sqrt(worldDistanceSquared(
                    fromX, fromY, particle.x, particle.y))
                const cluster = particle.clusterValue === undefined
                    ? particle.value : particle.clusterValue
                const value = 0.7 + particle.value
                    + intelligence * Math.min(12, cluster) * 0.22
                    + (sameDeath ? intelligence * 2.5 : 0)
                const reachability = turnReachabilityPenaltyFrom(
                    snake, fromX, fromY, heading, particle)
                const score = distance / value * reachability
                if (score < bestScore) {
                    bestScore = score
                    best = particle
                }
            }
            if (!best)
                break
            path.push(best.id)
            selected[best.id] = true
            const dx = axisDelta(fromX, best.x, worldWidth)
            const dy = axisDelta(fromY, best.y, worldHeight)
            if (Math.abs(dx) + Math.abs(dy) > 0.001)
                heading = Math.atan2(dy, dx)
            fromX = best.x
            fromY = best.y
        }
        if (path.length === 0)
            path.push(anchor.id)
        return path
    }

    function chooseGoal(snake, snakeIndex) {
        const head = snake.segments[0]
        let goalX = worldWidth / 2
        let goalY = worldHeight / 2
        let bestScore = Number.MAX_VALUE
        let bestHarvest = 0
        let chosen = null
        let choseNewAnchor = false

        // A snake already sweeping a death trail looks several particles ahead.
        // The vacuum then collects the intervening line instead of making the
        // head stop and zig-zag at every individual particle.
        if (snake.feastId > 0) {
            const lookAhead = Math.round(6 + intelligence * 14)
            for (let index = 0; index < food.length; ++index) {
                const particle = food[index]
                if (particle.feastId !== snake.feastId)
                    continue
                const progress = snake.feastTargetIndex < 0 ? 0
                    : (particle.trailIndex - snake.feastTargetIndex)
                      * snake.feastDirection
                const trailPenalty = progress >= 0 && progress <= lookAhead
                    ? lookAhead - progress
                    : lookAhead + Math.abs(progress) + 4
                const claimPenalty = particle.claimedBy !== snakeIndex
                    && particle.claimedUntil > simulationTime ? 1 + intelligence * 3 : 1
                const score = (trailPenalty * worldWidth * worldHeight
                    + worldDistanceSquared(head.x, head.y, particle.x, particle.y))
                    * claimPenalty
                if (score < bestScore) {
                    bestScore = score
                    chosen = particle
                }
            }
            if (!chosen) {
                snake.feastId = 0
                snake.feastTargetIndex = -1
                bestScore = Number.MAX_VALUE
            }
        }

        if (!chosen && snake.foodPathUntil > simulationTime) {
            chosen = nextFoodPathParticle(snake)
            if (chosen)
                bestHarvest = routeHarvestValue(snake, chosen)
        }

        if (!chosen) {
            // First reduce the potentially large food field to a small set of
            // promising endpoints, then do capture-corridor scoring only for
            // those endpoints. This keeps maximum-density simulation cheap.
            const candidates = []
            const candidateLimit = 12
            for (let index = 0; index < food.length; ++index) {
                const particle = food[index]
                const cluster = particle.clusterValue === undefined
                    ? particle.value : particle.clusterValue
                const deathMass = particle.feastId > 0
                    ? Math.sqrt(Math.max(1, particle.feastLength)) * intelligence * 1.8 : 0
                const persistence = particle.id === snake.foodTargetId
                    && snake.foodTargetUntil > simulationTime ? 0.55 : 1
                const claimPenalty = particle.claimedBy !== snakeIndex
                    && particle.claimedUntil > simulationTime ? 1 + intelligence * 3 : 1
                const reachability = Math.min(8, turnReachabilityPenalty(snake, particle))
                const coarseScore = Math.pow(worldDistanceSquared(
                    head.x, head.y, particle.x, particle.y), 0.68)
                    / (0.5 + particle.value + cluster * (1 + intelligence * 2.4)
                       + deathMass) * persistence * claimPenalty * reachability
                insertRouteCandidate(candidates, particle, coarseScore, candidateLimit)
            }

            for (let index = 0; index < candidates.length; ++index) {
                const particle = candidates[index].particle
                const cluster = particle.clusterValue === undefined
                    ? particle.value : particle.clusterValue
                const harvest = routeHarvestValue(snake, particle)
                const deathMass = particle.feastId > 0
                    ? Math.sqrt(Math.max(1, particle.feastLength)) * intelligence * 2.2 : 0
                const persistence = particle.id === snake.foodTargetId
                    && snake.foodTargetUntil > simulationTime ? 0.62 : 1
                const claimPenalty = particle.claimedBy !== snakeIndex
                    && particle.claimedUntil > simulationTime ? 1 + intelligence * 3 : 1
                const reachability = Math.min(8, turnReachabilityPenalty(snake, particle))
                const routeValue = 0.5 + particle.value + cluster * 1.2
                    + harvest * (1.4 + intelligence * 4.8) + deathMass
                const score = Math.pow(worldDistanceSquared(
                    head.x, head.y, particle.x, particle.y), 0.68)
                    / routeValue * persistence * claimPenalty * reachability
                if (score < bestScore) {
                    bestScore = score
                    bestHarvest = harvest
                    chosen = particle
                    choseNewAnchor = true
                }
            }
        }

        if (chosen && choseNewAnchor) {
            const anchor = chosen
            snake.foodPathIds = buildFoodPath(snake, anchor)
            snake.foodPathUntil = simulationTime + 0.9
            chosen = nextFoodPathParticle(snake) || anchor
            bestHarvest = routeHarvestValue(snake, chosen)
        }

        if (chosen) {
            const approach = foodApproachGoal(snake, chosen)
            goalX = approach.x
            goalY = approach.y
            const cluster = chosen.clusterValue === undefined
                ? chosen.value : chosen.clusterValue
            if (bestHarvest <= 0)
                bestHarvest = routeHarvestValue(snake, chosen)
            chosen.claimedBy = snakeIndex
            chosen.claimedUntil = simulationTime + 0.34
            snake.foodTargetId = chosen.id
            snake.foodTargetUntil = simulationTime + 0.52
            snake.rush = intelligence * clamp((Math.max(cluster, bestHarvest) - 1.1)
                                               / 10, 0, 0.3)
            if (chosen.feastId > 0 && intelligence > 0.2) {
                snake.feastId = chosen.feastId
                if (snake.feastTargetIndex < 0) {
                    snake.feastDirection = chosen.trailIndex < chosen.feastLength / 2 ? 1 : -1
                    snake.feastTargetIndex = chosen.trailIndex
                }
                snake.rush = Math.max(snake.rush, intelligence * 0.28)
            }
        } else {
            snake.rush = 0
            snake.foodTargetId = 0
        }

        // Food always wins over optional hunting behavior. Rival interception
        // is only considered in the exceptional case of an empty food field.
        const hunting = !chosen
                        && snake.aggression > 0.78 - intelligence * 0.18
                        && Math.sin(simulationTime * 0.55 + snake.wanderPhase) > 0.2
        if (hunting) {
            let preyDistance = Number.MAX_VALUE
            for (let otherIndex = 0; otherIndex < snakes.length; ++otherIndex) {
                if (otherIndex === snakeIndex)
                    continue
                const other = snakes[otherIndex]
                if (!other.alive || other.segments.length + 3 >= snake.segments.length)
                    continue
                const otherHead = other.segments[0]
                const distance = worldDistanceSquared(head.x, head.y, otherHead.x, otherHead.y)
                if (distance < preyDistance) {
                    preyDistance = distance
                    const lead = (35 + other.radius * 5) * (0.55 + intelligence * 1.15)
                    goalX = otherHead.x + Math.cos(other.angle) * lead
                    goalY = otherHead.y + Math.sin(other.angle) * lead
                }
            }
        }
        return { x: goalX, y: goalY, harvest: bestHarvest }
    }

    function snakeSpeed(snake) {
        const speedControl = context ? context.animationSpeed : 100
        const lengthPenalty = 1 + Math.max(0, snake.segments.length - 24) * 0.004
        const growthBoost = !snake.growthBlocked
            && snake.growth >= segmentGrowthCost(snake) ? 0.2 : 0
        return (52 + speedControl * 0.66) * snake.speedBias
               * (1 + (snake.rush || 0) + growthBoost) / lengthPenalty
    }

    function segmentMotion(segment, maximumSpeed) {
        const previousX = segment.previousX === undefined ? segment.x : segment.previousX
        const previousY = segment.previousY === undefined ? segment.y : segment.previousY
        let velocityX = axisDelta(previousX, segment.x, worldWidth) * 60
        let velocityY = axisDelta(previousY, segment.y, worldHeight) * 60
        const velocity = Math.sqrt(velocityX * velocityX + velocityY * velocityY)
        if (velocity > maximumSpeed && velocity > 0.001) {
            velocityX *= maximumSpeed / velocity
            velocityY *= maximumSpeed / velocity
        }
        return { x: velocityX, y: velocityY }
    }

    // Represent bodies as connected capsules, not disconnected sample points.
    // A long snake may use fewer capsules, but every gap between sampled
    // segments is still solid geometry. Self capsules also carry the estimated
    // time at which the tail will have vacated them; this lets a clever snake
    // cross behind its own moving tail without treating the whole coil as a
    // permanent wall.
    function collectHazards(snake, snakeIndex, horizon) {
        const head = snake.segments[0]
        const range = horizon + snake.radius * 8
        const rangeSquared = range * range
        const hazards = []
        const ownSpeed = Math.max(1, snakeSpeed(snake))
        const ownSpacing = snake.radius * 1.18
        for (let otherIndex = 0; otherIndex < snakes.length; ++otherIndex) {
            const other = snakes[otherIndex]
            if (!other.alive || other.segments.length === 0)
                continue
            const sameSnake = otherIndex === snakeIndex
            if (sameSnake && !selfCollisions)
                continue
            const otherSpeed = Math.max(1, snakeSpeed(other))
            const maximumMotion = otherSpeed * 1.35

            if (!sameSnake) {
                const otherHead = other.segments[0]
                if (worldDistanceSquared(head.x, head.y,
                                         otherHead.x, otherHead.y) <= rangeSquared) {
                    const motion = segmentMotion(otherHead, maximumMotion)
                    // Desired heading is more stable than a one-frame body
                    // velocity when a rival has only just begun to turn.
                    const desiredHeading = other.desiredAngle === undefined
                        ? other.angle : other.desiredAngle
                    const predictedX = (Math.cos(other.angle) * 0.7
                                        + Math.cos(desiredHeading) * 0.3) * otherSpeed
                    const predictedY = (Math.sin(other.angle) * 0.7
                                        + Math.sin(desiredHeading) * 0.3) * otherSpeed
                    hazards.push({
                        x: otherHead.x, y: otherHead.y,
                        x2: otherHead.x, y2: otherHead.y,
                        vx: motion.x * 0.15 + predictedX * 0.85,
                        vy: motion.y * 0.15 + predictedY * 0.85,
                        vx2: motion.x * 0.15 + predictedX * 0.85,
                        vy2: motion.y * 0.15 + predictedY * 0.85,
                        radius: other.radius,
                        self: false,
                        head: true,
                        halfLength: 0,
                        halfStretchSpeed: 0,
                        releaseTime: Number.MAX_VALUE
                    })
                }
            }

            const firstSegment = sameSnake ? Math.min(10, other.segments.length) : 3
            const targetCapsules = sameSnake
                ? Math.max(Math.ceil(other.segments.length / 4),
                           Math.round(65 + intelligence * 25))
                : Math.max(Math.ceil(other.segments.length / 6),
                           Math.round(32 + intelligence * 13))
            // Capsules remain continuous even when a few original points are
            // coalesced, avoiding redundant geometry without reopening gaps.
            const stride = Math.max(2, Math.ceil(
                Math.max(1, other.segments.length - firstSegment) / targetCapsules))
            for (let segmentIndex = firstSegment;
                    segmentIndex < other.segments.length; segmentIndex += stride) {
                const endpointIndex = Math.min(other.segments.length - 1,
                                               segmentIndex + stride)
                const segment = other.segments[segmentIndex]
                const endpoint = other.segments[endpointIndex]
                if (worldSegmentDistanceSquared(head.x, head.y,
                                                segment.x, segment.y,
                                                endpoint.x, endpoint.y) > rangeSquared)
                    continue
                const motion = sameSnake ? { x: 0, y: 0 }
                                         : segmentMotion(segment, maximumMotion)
                const endpointMotion = sameSnake ? { x: 0, y: 0 }
                    : segmentMotion(endpoint, maximumMotion)
                const spanX = axisDelta(segment.x, endpoint.x, worldWidth)
                const spanY = axisDelta(segment.y, endpoint.y, worldHeight)
                const stretchX = endpointMotion.x - motion.x
                const stretchY = endpointMotion.y - motion.y
                hazards.push({
                    x: segment.x, y: segment.y,
                    x2: endpoint.x, y2: endpoint.y,
                    vx: motion.x, vy: motion.y,
                    vx2: endpointMotion.x, vy2: endpointMotion.y,
                    radius: other.radius * (1 + Math.min(5, stride - 1) * 0.08),
                    self: sameSnake,
                    head: false,
                    halfLength: Math.sqrt(spanX * spanX + spanY * spanY) * 0.5,
                    halfStretchSpeed: Math.sqrt(stretchX * stretchX
                                                + stretchY * stretchY) * 0.5,
                    releaseTime: sameSnake
                        ? Math.max(0, (other.segments.length - 1 - segmentIndex)
                                   * ownSpacing / ownSpeed)
                        : Number.MAX_VALUE
                })
            }
        }
        return hazards
    }

    // Roll out an actual future trajectory. The simulated head is limited by
    // the same turn rate and speed as the live snake. Escape candidates first
    // hold a turn, then recover toward food, giving the planner routes around a
    // body instead of only straight rays through it.
    function projectTrajectory(snake, targetAngle, recoveryFraction,
                               goalX, goalY, horizon, samples) {
        const points = []
        const speed = Math.max(1, snakeSpeed(snake))
        const duration = horizon / speed
        const stepTime = duration / samples
        const maxTurn = snakeTurnRate(snake) * stepTime
        let projectedX = snake.segments[0].x
        let projectedY = snake.segments[0].y
        let projectedAngle = snake.angle
        points.push({ x: projectedX, y: projectedY,
                      angle: projectedAngle, time: 0 })
        for (let sample = 1; sample <= samples; ++sample) {
            const progress = sample / samples
            let desired = targetAngle
            if (progress >= recoveryFraction) {
                desired = Math.atan2(axisDelta(projectedY, goalY, worldHeight),
                                     axisDelta(projectedX, goalX, worldWidth))
            }
            const difference = normalizeAngle(desired - projectedAngle)
            projectedAngle = normalizeAngle(projectedAngle
                + clamp(difference, -maxTurn, maxTurn))
            projectedX += Math.cos(projectedAngle) * speed * stepTime
            projectedY += Math.sin(projectedAngle) * speed * stepTime
            if (!deadlyWalls) {
                projectedX = wrapCoordinate(projectedX, worldWidth)
                projectedY = wrapCoordinate(projectedY, worldHeight)
            }
            points.push({ x: projectedX, y: projectedY,
                          angle: projectedAngle, time: sample * stepTime })
        }
        return points
    }

    // Score what the whole curved vacuum corridor will collect. Each particle
    // is counted once at its earliest intercept, allowing the snake to weave
    // through a clump instead of fixating on the centre of one particle.
    function trajectoryHarvestValue(snake, snakeIndex, points) {
        let harvest = 0
        for (let foodIndex = 0; foodIndex < food.length; ++foodIndex) {
            const particle = food[foodIndex]
            const captureRadius = foodCaptureRadius(snake, particle)
            const captureSquared = captureRadius * captureRadius
            for (let pointIndex = 1; pointIndex < points.length; ++pointIndex) {
                const previous = points[pointIndex - 1]
                const point = points[pointIndex]
                const distanceSquared = worldSegmentDistanceSquared(
                    particle.x, particle.y, previous.x, previous.y, point.x, point.y)
                if (distanceSquared > captureSquared)
                    continue
                const distance = Math.sqrt(distanceSquared)
                const corridorWeight = 0.35 + 0.65
                    * (1 - distance / Math.max(1, captureRadius))
                const claimWeight = particle.claimedBy !== snakeIndex
                    && particle.claimedUntil > simulationTime ? 0.3 : 1
                const timeWeight = 1 / (1 + point.time * 0.12)
                harvest += particle.value * deathFoodMultiplier(particle)
                           * corridorWeight * claimWeight * timeWeight
                break
            }
        }
        return harvest
    }

    function evaluateTrajectory(snake, snakeIndex, points, hazards, goalX, goalY) {
        let risk = 0
        let collides = false
        let collisionTime = Number.MAX_VALUE
        let minimumClearance = Number.MAX_VALUE
        for (let pointIndex = 1; pointIndex < points.length; ++pointIndex) {
            const previousPoint = points[pointIndex - 1]
            const point = points[pointIndex]
            const urgency = points.length - pointIndex
            const routeX = axisDelta(previousPoint.x, point.x, worldWidth)
            const routeY = axisDelta(previousPoint.y, point.y, worldHeight)
            const routeMidX = previousPoint.x + routeX * 0.5
            const routeMidY = previousPoint.y + routeY * 0.5
            const routeHalfLength = Math.sqrt(routeX * routeX + routeY * routeY) * 0.5
            if (deadlyWalls) {
                const wallDistance = Math.min(point.x, point.y,
                                              worldWidth - point.x,
                                              worldHeight - point.y)
                const collisionDistance = snake.radius * 1.05
                const softDistance = collisionDistance
                    + snake.radius * (2.2 + intelligence * 2.4)
                minimumClearance = Math.min(minimumClearance,
                                            wallDistance - collisionDistance)
                if (wallDistance <= collisionDistance) {
                    collides = true
                    collisionTime = Math.min(collisionTime, point.time)
                    risk += 100000 * (1 + urgency)
                } else if (wallDistance < softDistance) {
                    const proximity = (softDistance - wallDistance)
                                      / Math.max(1, softDistance - collisionDistance)
                    risk += proximity * proximity * (1 + urgency)
                            * (2 + intelligence * 5)
                }
            }

            for (let hazardIndex = 0; hazardIndex < hazards.length; ++hazardIndex) {
                const hazard = hazards[hazardIndex]
                if (hazard.self && previousPoint.time >= hazard.releaseTime)
                    continue
                const predictionTime = hazard.self
                    ? Math.min((previousPoint.time + point.time) * 0.5,
                               hazard.releaseTime)
                    : (previousPoint.time + point.time) * 0.5
                let hazardX = hazard.x + hazard.vx * predictionTime
                let hazardY = hazard.y + hazard.vy * predictionTime
                let hazardX2 = hazard.x2 + hazard.vx2 * predictionTime
                let hazardY2 = hazard.y2 + hazard.vy2 * predictionTime
                if (!deadlyWalls) {
                    hazardX = wrapCoordinate(hazardX, worldWidth)
                    hazardY = wrapCoordinate(hazardY, worldHeight)
                    hazardX2 = wrapCoordinate(hazardX2, worldWidth)
                    hazardY2 = wrapCoordinate(hazardY2, worldHeight)
                }
                const collisionDistance = hazard.self
                    ? (snake.radius + hazard.radius) * 0.74
                    : (snake.radius + hazard.radius) * (hazard.head ? 0.88 : 0.78)
                // Rival-head prediction becomes less certain farther out, so
                // its safety envelope grows slightly with time.
                const uncertainty = hazard.head
                    ? snake.radius * intelligence * Math.min(1.8, point.time * 0.35) : 0
                const softDistance = collisionDistance + uncertainty
                    + snake.radius * (1.35 + intelligence * (hazard.self ? 2.8 : 2.1))
                // Cheap bounding-circle rejection avoids running the exact
                // segment/segment calculation for distant capsule pairs.
                const hazardSpanX = axisDelta(hazardX, hazardX2, worldWidth)
                const hazardSpanY = axisDelta(hazardY, hazardY2, worldHeight)
                const hazardMidX = hazardX + hazardSpanX * 0.5
                const hazardMidY = hazardY + hazardSpanY * 0.5
                const boundingDistance = routeHalfLength + hazard.halfLength
                    + hazard.halfStretchSpeed * predictionTime + softDistance
                if (worldDistanceSquared(routeMidX, routeMidY,
                                         hazardMidX, hazardMidY)
                        > boundingDistance * boundingDistance)
                    continue
                const distance = Math.sqrt(worldSegmentsDistanceSquared(
                    previousPoint.x, previousPoint.y, point.x, point.y,
                    hazardX, hazardY, hazardX2, hazardY2))
                minimumClearance = Math.min(minimumClearance,
                                            distance - collisionDistance)
                if (distance <= collisionDistance) {
                    collides = true
                    collisionTime = Math.min(collisionTime, point.time)
                    risk += 100000 * (1 + urgency)
                            * (hazard.self ? 1.5 : 1)
                } else if (distance < softDistance) {
                    const proximity = (softDistance - distance)
                                      / Math.max(1, softDistance - collisionDistance)
                    risk += proximity * proximity * (1 + urgency)
                            * (2 + intelligence * 6)
                            * (hazard.self ? 1.6 : 1)
                }
            }
        }
        const head = snake.segments[0]
        const finalPoint = points[points.length - 1]
        const initialDistance = Math.sqrt(worldDistanceSquared(
            head.x, head.y, goalX, goalY))
        const finalDistance = Math.sqrt(worldDistanceSquared(
            finalPoint.x, finalPoint.y, goalX, goalY))
        return {
            risk: risk,
            collides: collides,
            collisionTime: collisionTime,
            minimumClearance: minimumClearance,
            progress: initialDistance - finalDistance,
            finalDistance: finalDistance,
            harvest: 0,
            points: points
        }
    }

    // Compatibility helper used by tests and diagnostics. Unlike the former
    // six-ray approximation this follows the snake's real turning circle and
    // checks continuous body capsules.
    function headingRisk(snake, snakeIndex, angle, horizon, suppliedHazards) {
        const hazards = suppliedHazards || collectHazards(snake, snakeIndex, horizon)
        const head = snake.segments[0]
        const points = projectTrajectory(snake, angle, 2,
                                         head.x + Math.cos(angle) * horizon,
                                         head.y + Math.sin(angle) * horizon,
                                         horizon, Math.round(7 + intelligence))
        return evaluateTrajectory(snake, snakeIndex, points, hazards,
                                  points[points.length - 1].x,
                                  points[points.length - 1].y).risk
    }

    function addTrajectoryCandidate(candidates, angle, recoveryFraction) {
        const normalized = normalizeAngle(angle)
        for (let index = 0; index < candidates.length; ++index) {
            if (Math.abs(normalizeAngle(candidates[index].angle - normalized)) < 0.025
                    && Math.abs(candidates[index].recovery - recoveryFraction) < 0.04)
                return
        }
        candidates.push({ angle: normalized, recovery: recoveryFraction })
    }

    function planningSnakeSnapshot(snake) {
        // Rollouts only need the head plus the body length used by speed and
        // turn-rate calculations. Keeping a sparse array avoids copying a
        // champion's entire body for every plan.
        const segments = new Array(snake.segments.length)
        segments[0] = {
            x: snake.segments[0].x,
            y: snake.segments[0].y
        }
        return {
            segments: segments,
            angle: snake.angle,
            desiredAngle: snake.desiredAngle,
            radius: snake.radius,
            speedBias: snake.speedBias,
            rush: snake.rush,
            growthBlocked: snake.growthBlocked,
            growth: snake.growth,
            avoidanceSide: snake.avoidanceSide,
            avoidanceCommitUntil: snake.avoidanceCommitUntil
        }
    }

    // Constructing the hazard snapshot is one bounded planner work unit. The
    // candidate rollouts and food-corridor integrations are advanced separately
    // so no frame has to solve an entire receding-horizon search at once.
    function createSteeringPlan(snake, snakeIndex) {
        const head = snake.segments[0]
        let goal
        if (snake.foodPlanUntil <= simulationTime) {
            goal = chooseGoal(snake, snakeIndex)
            snake.plannedGoalX = goal.x
            snake.plannedGoalY = goal.y
            snake.foodPlanUntil = simulationTime + 0.12 + (snakeIndex % 3) * 0.008
        } else {
            goal = { x: snake.plannedGoalX, y: snake.plannedGoalY }
        }
        const goalAngle = Math.atan2(axisDelta(head.y, goal.y, worldHeight),
                                     axisDelta(head.x, goal.x, worldWidth))
        const horizon = Math.max(120 + intelligence * 280,
                                 snake.radius * (14 + intelligence * 24))
        const samples = Math.round(7 + intelligence)
        const hazards = collectHazards(snake, snakeIndex, horizon)
        const planningSnake = planningSnakeSnapshot(snake)
        const candidates = []
        addTrajectoryCandidate(candidates, goalAngle, 0)
        addTrajectoryCandidate(candidates, snake.angle, 0.24)
        const goalSweep = 0.22 + intelligence * 0.18
        addTrajectoryCandidate(candidates, goalAngle - goalSweep, 0.3)
        addTrajectoryCandidate(candidates, goalAngle + goalSweep, 0.3)
        const escapeAngles = intelligence >= 0.85
            ? [0.46, 0.88, 1.38, 2.02]
            : (intelligence >= 0.45 ? [0.48, 0.9, 1.35] : [0.62, 1.2])
        for (let index = 0; index < escapeAngles.length; ++index) {
            const offset = escapeAngles[index]
            const recovery = clamp(0.28 + index * 0.13, 0.28, 0.86)
            addTrajectoryCandidate(candidates, snake.angle - offset, recovery)
            addTrajectoryCandidate(candidates, snake.angle + offset, recovery)
        }

        return {
            snake: snake,
            planningSnake: planningSnake,
            snakeIndex: snakeIndex,
            goal: goal,
            goalAngle: goalAngle,
            horizon: horizon,
            samples: samples,
            hazards: hazards,
            candidates: candidates,
            candidateIndex: 0,
            evaluated: [],
            hasSafeRoute: false,
            harvestCandidates: [],
            harvestIndex: 0,
            harvestLimit: 0,
            stage: "evaluate",
            complete: false,
            selectedAngle: goalAngle,
            commitActive: snake.avoidanceCommitUntil > simulationTime
        }
    }

    function evaluateNextPlanCandidate(plan) {
        const candidate = plan.candidates[plan.candidateIndex]
        const points = projectTrajectory(plan.planningSnake, candidate.angle,
                                             candidate.recovery,
                                             plan.goal.x, plan.goal.y,
                                             plan.horizon, plan.samples)
        const result = evaluateTrajectory(plan.planningSnake, plan.snakeIndex, points,
                                          plan.hazards,
                                          plan.goal.x, plan.goal.y)
        result.candidate = candidate
        plan.evaluated.push(result)
        if (!result.collides)
            plan.hasSafeRoute = true

        // Safety must not wait for food scoring and every distant escape
        // rollout. Publish a viable direction as soon as it is known. The
        // completed plan may refine this a few frames later, but the live snake
        // has already begun the turn instead of continuing along a doomed,
        // stale heading.
        if (plan.candidateIndex === 0) {
            plan.directCollides = result.collides
            if (!result.collides) {
                plan.snake.desiredAngle = candidate.angle
                plan.provisionalPublished = true
            }
        } else if (plan.directCollides && !result.collides
                   && !plan.provisionalPublished) {
            plan.snake.desiredAngle = candidate.angle
            plan.provisionalPublished = true
        }
        ++plan.candidateIndex
    }

    function preparePlanHarvest(plan) {
        const snake = plan.planningSnake
        const commitActive = plan.commitActive
        for (let index = 0; index < plan.evaluated.length; ++index) {
            const result = plan.evaluated[index]
            const candidateTurn = normalizeAngle(result.candidate.angle
                                                 - snake.angle)
            const side = candidateTurn < -0.04 ? -1 : (candidateTurn > 0.04 ? 1 : 0)
            const commitmentCost = commitActive && side !== 0
                && side !== snake.avoidanceSide ? intelligence * 9 : 0
            const collisionCost = result.collides
                ? 1000000 / Math.max(0.05, result.collisionTime) : 0
            result.baseScore = collisionCost
                + result.risk * (1.5 + intelligence * 8.5)
                + result.finalDistance / Math.max(1, plan.horizon)
                  * (1.4 + intelligence * 2.4)
                - result.progress / Math.max(1, plan.horizon)
                  * (1.1 + intelligence * 2.8)
                + Math.abs(candidateTurn) * (0.12 + intelligence * 0.08)
                + Math.abs(normalizeAngle(result.candidate.angle
                                          - snake.desiredAngle))
                  * (0.08 + intelligence * 0.24)
                + commitmentCost
        }

        // Collision geometry is evaluated for every route, but the food
        // corridor integral is needed only for the most promising safe routes.
        // This retains deliberate weaving without multiplying dense-food work
        // by every emergency escape trajectory.
        plan.harvestCandidates = plan.evaluated.filter(function(result) {
            return !plan.hasSafeRoute || !result.collides
        }).sort(function(left, right) {
            return left.baseScore - right.baseScore
        })
        plan.harvestLimit = Math.min(plan.harvestCandidates.length,
                                     Math.round(2 + intelligence))
        plan.harvestIndex = 0
        plan.stage = plan.harvestLimit > 0 ? "harvest" : "finish"
    }

    function evaluateNextPlanHarvest(plan) {
        const result = plan.harvestCandidates[plan.harvestIndex]
        result.harvest = trajectoryHarvestValue(plan.planningSnake, plan.snakeIndex,
                                                result.points)
        ++plan.harvestIndex
        if (plan.harvestIndex >= plan.harvestLimit)
            plan.stage = "finish"
    }

    function finishSteeringPlan(plan) {
        const snake = plan.snake
        let best = null
        let bestScore = Number.MAX_VALUE
        for (let index = 0; index < plan.evaluated.length; ++index) {
            const result = plan.evaluated[index]
            if (plan.hasSafeRoute && result.collides)
                continue
            const score = result.baseScore
                - result.harvest * (0.55 + intelligence * 3.1)
            if (score < bestScore) {
                bestScore = score
                best = result
            }
        }

        if (!best) {
            plan.selectedAngle = plan.goalAngle
            plan.complete = true
            return
        }
        const direct = plan.evaluated.length > 0 ? plan.evaluated[0] : best
        const selectedTurn = normalizeAngle(best.candidate.angle - snake.angle)
        if (Math.abs(selectedTurn) > 0.22
                && (direct.collides || direct.risk > 18)) {
            snake.avoidanceSide = selectedTurn < 0 ? -1 : 1
            snake.avoidanceCommitUntil = simulationTime + 0.42 + intelligence * 0.48
        } else if (simulationTime >= snake.avoidanceCommitUntil
                   && best.risk < 2) {
            snake.avoidanceSide = 0
        }
        snake.lastPlanRisk = best.risk
        snake.lastPlanCollision = best.collides
        snake.lastPlanHarvest = best.harvest
        plan.selectedAngle = best.candidate.angle
        plan.complete = true
    }

    // Advance exactly one expensive unit: one trajectory collision rollout or
    // one food-corridor integration. Bookkeeping at stage boundaries is cheap
    // and remains attached to the unit that completed the preceding stage.
    function advanceSteeringPlan(plan) {
        if (!plan || plan.complete)
            return true
        if (plan.stage === "evaluate") {
            evaluateNextPlanCandidate(plan)
            if (plan.candidateIndex >= plan.candidates.length)
                preparePlanHarvest(plan)
        } else if (plan.stage === "harvest") {
            evaluateNextPlanHarvest(plan)
        }
        if (plan.stage === "finish")
            finishSteeringPlan(plan)
        return plan.complete
    }

    // Synchronous compatibility entry point for focused tests and diagnostics.
    // Runtime scheduling uses the incremental functions below.
    function planSteering(snake, snakeIndex) {
        const plan = createSteeringPlan(snake, snakeIndex)
        while (!advanceSteeringPlan(plan)) {
            // Intentionally empty: tests require the completed decision.
        }
        return plan.selectedAngle
    }

    function nextSnakeNeedingPlan() {
        for (let attempt = 0; attempt < snakes.length; ++attempt) {
            const index = (brainCursor + attempt) % snakes.length
            const snake = snakes[index]
            if (!snake.alive || snake.brainPlanning || snake.brainCooldown > 0)
                continue
            return index
        }
        return -1
    }

    // The frame budget is expressed in deterministic work units instead of
    // wall-clock milliseconds, making behavior stable across machines and
    // tests. A unit is the smallest expensive planner operation; three units per
    // 60 Hz simulation tick leave the GUI thread ample time to present frames.
    function updateSnakeBrains(seconds) {
        lastBrainWorkUnits = 0
        for (let index = 0; index < snakes.length; ++index) {
            if (snakes[index].alive && !snakes[index].brainPlanning)
                snakes[index].brainCooldown -= seconds
        }

        while (lastBrainWorkUnits < maximumBrainWorkUnits) {
            if (activeBrainPlan
                    && (activeBrainPlan.snakeIndex >= snakes.length
                        || snakes[activeBrainPlan.snakeIndex] !== activeBrainPlan.snake
                        || !activeBrainPlan.snake.alive)) {
                activeBrainPlan.snake.brainPlanning = false
                activeBrainPlan = null
            }

            if (!activeBrainPlan) {
                const index = nextSnakeNeedingPlan()
                if (index < 0)
                    return
                const snake = snakes[index]
                snake.brainPlanning = true
                activeBrainPlan = createSteeringPlan(snake, index)
                brainCursor = (index + 1) % snakes.length
                ++lastBrainWorkUnits
                continue
            }

            const plan = activeBrainPlan
            const complete = advanceSteeringPlan(plan)
            ++lastBrainWorkUnits
            if (!complete)
                continue

            const snake = plan.snake
            snake.desiredAngle = plan.selectedAngle
            snake.brainPlanning = false
            // A predictive route remains useful for several tenths of a
            // second. Replan rapidly only while the selected route is
            // genuinely hazardous; open-space snakes spend that time moving
            // rather than repeatedly proving the same path safe.
            const urgent = snake.lastPlanCollision
            snake.brainCooldown = urgent ? 0.20
                : 0.90 - intelligence * 0.10
                  + (plan.snakeIndex % 3) * 0.008
            activeBrainPlan = null
        }
    }

    function steerSnake(snake, seconds) {
        const difference = normalizeAngle(snake.desiredAngle - snake.angle)
        const maxTurn = snakeTurnRate(snake) * seconds
        snake.angle = normalizeAngle(snake.angle + clamp(difference, -maxTurn, maxTurn))
    }

    // The predictive planner deliberately spreads its expensive work across
    // frames. Walls are static and cheap to reason about, so enforce them with
    // a per-frame safety controller as well. Its activation distance includes
    // the snake's real minimum turning radius; even a head pointed directly at
    // an edge begins its U-turn early enough to remain inside the arena.
    function applyWallSafety(snake) {
        if (!deadlyWalls || snake.segments.length === 0)
            return false
        const head = snake.segments[0]
        const turnRadius = snakeSpeed(snake) / Math.max(0.1, snakeTurnRate(snake))
        const safetyDistance = snake.radius * 2.6 + turnRadius * 1.35
        let inwardX = 0
        let inwardY = 0
        let closest = Number.MAX_VALUE

        function pressure(distance) {
            if (distance >= safetyDistance)
                return 0
            const amount = 1 - Math.max(0, distance) / safetyDistance
            return amount * amount
        }

        const leftPressure = pressure(head.x)
        const rightPressure = pressure(worldWidth - head.x)
        const topPressure = pressure(head.y)
        const bottomPressure = pressure(worldHeight - head.y)
        inwardX += leftPressure - rightPressure
        inwardY += topPressure - bottomPressure
        closest = Math.min(head.x, worldWidth - head.x,
                           head.y, worldHeight - head.y)

        const inwardLength = Math.sqrt(inwardX * inwardX + inwardY * inwardY)
        if (inwardLength < 0.0001)
            return false
        inwardX /= inwardLength
        inwardY /= inwardLength
        const forwardInward = Math.cos(snake.angle) * inwardX
                              + Math.sin(snake.angle) * inwardY
        const criticalDistance = snake.radius * 1.4 + turnRadius
        if (forwardInward < 0.72 || closest < criticalDistance) {
            snake.desiredAngle = Math.atan2(inwardY, inwardX)
            snake.brainCooldown = Math.min(snake.brainCooldown, 0.08)
            return true
        }
        return false
    }

    // The full planner predicts moving capsules far into the future, but a
    // dense arena can queue several plans. This bounded near-field guard scans
    // only coalesced body capsules along the head's immediate travel corridor.
    // It never changes speed; it commits to a turn away from the closest
    // imminent body while the predictive planner prepares the longer route.
    function applyCollisionSafety(snake, snakeIndex) {
        if (intelligence < 0.2 || snake.segments.length === 0)
            return false
        const enforcing = snake.safetyActiveUntil > simulationTime
        if (enforcing)
            snake.desiredAngle = snake.safetyDesiredAngle
        if (snake.nextSafetyCheck > simulationTime)
            return enforcing
        snake.nextSafetyCheck = simulationTime + 0.052
            + (snakeIndex % 3) * 0.004
        const head = snake.segments[0]
        const speed = snakeSpeed(snake)
        const turnRadius = speed / Math.max(0.1, snakeTurnRate(snake))
        const lookAhead = Math.min(190, speed * (0.38 + intelligence * 0.28)
                                   + turnRadius * 1.2)
        const futureX = head.x + Math.cos(snake.angle) * lookAhead
        const futureY = head.y + Math.sin(snake.angle) * lookAhead
        let bestDistanceSquared = Number.MAX_VALUE
        let threatX = 0
        let threatY = 0
        let threatIsSelf = false

        for (let otherIndex = 0; otherIndex < snakes.length; ++otherIndex) {
            const other = snakes[otherIndex]
            if (!other.alive || other.segments.length === 0)
                continue
            const sameSnake = otherIndex === snakeIndex
            if (sameSnake && !selfCollisions)
                continue

            if (!sameSnake) {
                const otherHead = other.segments[0]
                const otherTravel = snakeSpeed(other) * lookAhead / Math.max(1, speed)
                const otherFutureX = otherHead.x + Math.cos(other.angle) * otherTravel
                const otherFutureY = otherHead.y + Math.sin(other.angle) * otherTravel
                const headClearance = (snake.radius + other.radius)
                    * (1.35 + intelligence * 0.45)
                const crossingDistance = worldSegmentsDistanceSquared(
                    head.x, head.y, futureX, futureY,
                    otherHead.x, otherHead.y, otherFutureX, otherFutureY)
                if (crossingDistance < headClearance * headClearance
                        && crossingDistance < bestDistanceSquared) {
                    bestDistanceSquared = crossingDistance
                    threatX = axisDelta(head.x, otherHead.x, worldWidth)
                    threatY = axisDelta(head.y, otherHead.y, worldHeight)
                    threatIsSelf = false
                }
            }

        }

        // Query last frame's collision grid along the projected head route.
        // Body points are closer together than the safety clearance, so point
        // queries preserve a continuous guard while avoiding snakes x bodies
        // scans. One-frame-old positions are deliberately covered by the
        // generous near-field margin.
        const validGrid = safetyCellColumns > 0 && safetyCellRows > 0
            && safetyCellSnakeCount > 0
        const visitedCells = {}
        const visitedOccupants = {}
        const routeSamples = validGrid ? Math.max(1, Math.ceil(lookAhead
            / Math.max(1, Math.min(safetyCellWidth, safetyCellHeight)))) : 0
        for (let routeSample = 0; routeSample <= routeSamples; ++routeSample) {
            const amount = routeSample / Math.max(1, routeSamples)
            const sampleX = head.x + (futureX - head.x) * amount
            const sampleY = head.y + (futureY - head.y) * amount
            const centerX = collisionCell(sampleX, safetyCellWidth,
                                          safetyCellColumns, worldWidth)
            const centerY = collisionCell(sampleY, safetyCellHeight,
                                          safetyCellRows, worldHeight)
            for (let offsetX = -1; offsetX <= 1; ++offsetX) {
                for (let offsetY = -1; offsetY <= 1; ++offsetY) {
                    let cellX = centerX + offsetX
                    let cellY = centerY + offsetY
                    if (!deadlyWalls) {
                        cellX = ((cellX % safetyCellColumns)
                                 + safetyCellColumns) % safetyCellColumns
                        cellY = ((cellY % safetyCellRows)
                                 + safetyCellRows) % safetyCellRows
                    } else if (cellX < 0 || cellX >= safetyCellColumns
                               || cellY < 0 || cellY >= safetyCellRows) {
                        continue
                    }
                    const cellKey = cellY * safetyCellColumns + cellX
                    if (visitedCells[cellKey])
                        continue
                    visitedCells[cellKey] = true
                    const occupants = safetyCells[cellKey] || []
                    for (let occupantIndex = 0; occupantIndex < occupants.length;
                            ++occupantIndex) {
                        const encoded = occupants[occupantIndex]
                        if (visitedOccupants[encoded])
                            continue
                        visitedOccupants[encoded] = true
                        const otherIndex = encoded % safetyCellSnakeCount
                        const segmentIndex = Math.floor(encoded / safetyCellSnakeCount)
                        if (otherIndex >= snakes.length)
                            continue
                        const other = snakes[otherIndex]
                        if (!other.alive || segmentIndex >= other.segments.length)
                            continue
                        const sameSnake = otherIndex === snakeIndex
                        if ((sameSnake && !selfCollisions)
                                || (sameSnake && segmentIndex < 10))
                            continue
                        const segment = other.segments[segmentIndex]
                        const bodyClearance = sameSnake
                            ? snake.radius * (2.0 + intelligence * 1.1)
                            : (snake.radius + other.radius)
                              * (1.18 + intelligence * 0.42)
                        const routeDistance = worldSegmentDistanceSquared(
                            segment.x, segment.y,
                            head.x, head.y, futureX, futureY)
                        if (routeDistance >= bodyClearance * bodyClearance
                                || routeDistance >= bestDistanceSquared)
                            continue
                        bestDistanceSquared = routeDistance
                        threatX = axisDelta(head.x, segment.x, worldWidth)
                        threatY = axisDelta(head.y, segment.y, worldHeight)
                        threatIsSelf = sameSnake
                    }
                }
            }
        }

        if (bestDistanceSquared === Number.MAX_VALUE)
            return enforcing
        const cross = Math.cos(snake.angle) * threatY
                      - Math.sin(snake.angle) * threatX
        let turnSide
        if (!threatIsSelf && snake.avoidanceCommitUntil > simulationTime
                && snake.avoidanceSide !== 0) {
            turnSide = snake.avoidanceSide
        } else {
            turnSide = Math.abs(cross) < snake.radius * 0.35
                ? 1 : (cross > 0 ? -1 : 1)
        }
        snake.avoidanceSide = turnSide
        snake.avoidanceCommitUntil = Math.max(snake.avoidanceCommitUntil,
                                               simulationTime + 0.34
                                               + intelligence * 0.34)
        if (threatIsSelf) {
            // When circling food the tail is usually beside or slightly behind
            // the head. Steering merely left/right can tighten that same loop.
            // Blend the current heading with a radial vector away from the
            // coil, producing an outward escape line instead.
            const threatLength = Math.max(0.001,
                Math.sqrt(threatX * threatX + threatY * threatY))
            const toward = (Math.cos(snake.angle) * threatX
                            + Math.sin(snake.angle) * threatY) / threatLength
            if (toward > 0.2) {
                snake.safetyDesiredAngle = normalizeAngle(snake.angle
                    + turnSide * (0.9 + intelligence * 0.42))
            } else {
                const escapeX = Math.cos(snake.angle) * 0.58
                                - threatX / threatLength * 1.15
                const escapeY = Math.sin(snake.angle) * 0.58
                                - threatY / threatLength * 1.15
                snake.safetyDesiredAngle = Math.atan2(escapeY, escapeX)
            }
            snake.safetyActiveUntil = simulationTime + 0.28
            snake.foodPlanUntil = 0
        } else {
            snake.safetyDesiredAngle = normalizeAngle(snake.angle + turnSide
                * (0.72 + intelligence * 0.42))
            snake.safetyActiveUntil = simulationTime + 0.12
        }
        snake.desiredAngle = snake.safetyDesiredAngle
        snake.brainCooldown = Math.min(snake.brainCooldown, 0.06)
        return true
    }

    function updateSnakeRadius(snake) {
        const gainedSegments = Math.max(0, snake.segments.length - snake.birthLength)
        snake.radius = snake.baseRadius * (1 + Math.min(0.25, gainedSegments * 0.0025))
    }

    function appendGrowthSegment(snake) {
        const segments = snake.segments
        const tail = segments[segments.length - 1]
        const previous = segments.length > 1 ? segments[segments.length - 2] : null
        let directionX = previous ? axisDelta(previous.x, tail.x, worldWidth)
                                  : -Math.cos(snake.angle)
        let directionY = previous ? axisDelta(previous.y, tail.y, worldHeight)
                                  : -Math.sin(snake.angle)
        const directionLength = Math.sqrt(directionX * directionX + directionY * directionY)
        if (directionLength < 0.001) {
            directionX = -Math.cos(snake.angle)
            directionY = -Math.sin(snake.angle)
        } else {
            directionX /= directionLength
            directionY /= directionLength
        }
        const spacing = snake.radius * 1.18
        const newX = tail.x + directionX * spacing
        const newY = tail.y + directionY * spacing
        segments.push({
            x: deadlyWalls ? newX : wrapCoordinate(newX, worldWidth),
            y: deadlyWalls ? newY : wrapCoordinate(newY, worldHeight),
            previousX: deadlyWalls ? newX : wrapCoordinate(newX, worldWidth),
            previousY: deadlyWalls ? newY : wrapCoordinate(newY, worldHeight)
        })
        // This helper is also used to construct long snakes in tests. Rebuild
        // the history once movement resumes instead of doing quadratic work
        // for a batch of appended segments.
        snake.trailPoints = []
        snake.trailStart = 0
        updateSnakeRadius(snake)
    }

    function moveSnake(snake, snakeIndex, seconds) {
        const growthCost = segmentGrowthCost(snake)
        const growthAllowed = snake.segments.length < maximumSnakeSegments(snake)
                              && growthSlots > 0
        snake.growthBlocked = !growthAllowed
        for (let segmentIndex = 0; segmentIndex < snake.segments.length; ++segmentIndex) {
            snake.segments[segmentIndex].previousX = snake.segments[segmentIndex].x
            snake.segments[segmentIndex].previousY = snake.segments[segmentIndex].y
        }
        // Check for restored or externally repositioned segments before the
        // head advances; after movement a head/trail mismatch is intentional.
        ensureSnakeTrail(snake)
        applyCollisionSafety(snake, snakeIndex)
        applyWallSafety(snake)
        steerSnake(snake, seconds)
        const speed = snakeSpeed(snake)
        const head = snake.segments[0]
        head.x += Math.cos(snake.angle) * speed * seconds
        head.y += Math.sin(snake.angle) * speed * seconds
        if (!deadlyWalls) {
            head.x = wrapCoordinate(head.x, worldWidth)
            head.y = wrapCoordinate(head.y, worldHeight)
        }
        appendHeadTrailPoint(snake)

        const spacing = snake.radius * 1.18
        if (growthAllowed && snake.growth >= growthCost) {
            snake.growthStretch = Math.min(1, (snake.growthStretch || 0)
                + speed * seconds / Math.max(1, spacing) * 0.62)
        } else {
            snake.growthStretch = 0
        }
        placeSegmentsOnTrail(snake)

        if (growthAllowed && snake.growth >= growthCost && snake.growthStretch >= 1) {
            snake.segments.splice(1, 0, {
                x: head.x, y: head.y, previousX: head.x, previousY: head.y
            })
            snake.growth -= growthCost
            snake.growthStretch = 0
            growthSlots = Math.max(0, growthSlots - 1)
            updateSnakeRadius(snake)
            placeSegmentsOnTrail(snake)
        }
    }

    function consumeFoodParticle(eater, particle, index) {
        const storedGrowthLimit = segmentGrowthCost(eater) * 12
        eater.growth = Math.min(storedGrowthLimit,
                                eater.growth + particle.value)
        eater.score += particle.value
        if (particle.id === eater.foodTargetId)
            eater.foodPlanUntil = 0
        if (particle.feastId > 0 && eater.feastId === particle.feastId) {
            const nextTarget = particle.trailIndex + eater.feastDirection
            if (eater.feastTargetIndex < 0
                    || (nextTarget - eater.feastTargetIndex)
                       * eater.feastDirection > 0) {
                eater.feastTargetIndex = nextTarget
                eater.foodPlanUntil = 0
            }
        }
        food.splice(index, 1)
    }

    function feedSnakes(seconds, onlySnake) {
        for (let index = food.length - 1; index >= 0; --index) {
            const particle = food[index]
            let eater = null
            let eaterIndex = -1
            let closestDistance = Number.MAX_VALUE
            const lockedIndex = particle.vacuumOwner === undefined
                ? -1 : particle.vacuumOwner
            if (lockedIndex >= 0 && lockedIndex < snakes.length) {
                const lockedSnake = snakes[lockedIndex]
                if (lockedSnake.alive && (!onlySnake || lockedSnake === onlySnake)) {
                    eater = lockedSnake
                    eaterIndex = lockedIndex
                    closestDistance = Math.sqrt(worldDistanceSquared(
                        particle.x, particle.y,
                        eater.segments[0].x, eater.segments[0].y))
                } else {
                    particle.vacuumOwner = -1
                    particle.life = particle.vacuumOriginalLife === undefined
                        ? 20 : particle.vacuumOriginalLife
                }
            }
            if (!eater) {
                for (let snakeIndex = 0; snakeIndex < snakes.length; ++snakeIndex) {
                    const snake = snakes[snakeIndex]
                    if (!snake.alive || (onlySnake && snake !== onlySnake))
                        continue
                    const head = snake.segments[0]
                    const distance = Math.sqrt(worldDistanceSquared(
                        particle.x, particle.y, head.x, head.y))
                    const reach = foodCaptureRadius(snake, particle)
                    if (distance <= reach && distance < closestDistance) {
                        closestDistance = distance
                        eater = snake
                        eaterIndex = snakeIndex
                    }
                }
            }
            if (!eater)
                continue
            if (particle.vacuumOwner !== eaterIndex) {
                particle.vacuumOwner = eaterIndex
                particle.vacuumOriginalLife = particle.life
                particle.vx = 0
                particle.vy = 0
                particle.life = -1
            }
            const head = eater.segments[0]
            const eatingDistance = eater.radius * 1.16 + particle.size
            if (closestDistance <= eatingDistance) {
                consumeFoodParticle(eater, particle, index)
                continue
            }
            const pull = clamp(1 - (closestDistance - eatingDistance)
                               / Math.max(1, eater.radius * 1.84), 0.08, 1)
            const vacuumSpeed = Math.max(240 + pull * 420,
                                         snakeSpeed(eater) * 1.8)
            const travel = Math.min(closestDistance - eatingDistance,
                                    vacuumSpeed * seconds)
            if (travel >= closestDistance - eatingDistance - 0.001) {
                consumeFoodParticle(eater, particle, index)
                continue
            }
            const dx = axisDelta(particle.x, head.x, worldWidth)
            const dy = axisDelta(particle.y, head.y, worldHeight)
            particle.x += dx / closestDistance * travel
            particle.y += dy / closestDistance * travel
            if (!deadlyWalls) {
                particle.x = wrapCoordinate(particle.x, worldWidth)
                particle.y = wrapCoordinate(particle.y, worldHeight)
            }
            particle.attraction = pull
            particle.attractionX = head.x
            particle.attractionY = head.y
        }
    }

    function feedSnake(snake, seconds) {
        feedSnakes(seconds === undefined ? 1 / 60 : seconds, snake)
    }

    function collisionCell(value, cellSize, count, extent) {
        const coordinate = deadlyWalls ? clamp(value, 0, Math.max(0, extent - 0.001))
                                       : wrapCoordinate(value, extent)
        return clamp(Math.floor(coordinate / cellSize), 0, count - 1)
    }

    function markCollisions() {
        for (let index = 0; index < snakes.length; ++index) {
            const snake = snakes[index]
            snake.dying = false
            snake.deathReason = ""
            if (!snake.alive)
                continue
            const head = snake.segments[0]
            if (deadlyWalls
                    && (head.x < 0 || head.x > worldWidth
                        || head.y < 0 || head.y > worldHeight)) {
                snake.dying = true
                snake.deathReason = "wall"
            }
        }

        for (let left = 0; left < snakes.length; ++left) {
            const a = snakes[left]
            if (!a.alive)
                continue
            for (let right = left + 1; right < snakes.length; ++right) {
                const b = snakes[right]
                if (!b.alive)
                    continue
                const headDistance = (a.radius + b.radius) * 0.82
                const aHead = a.segments[0]
                const bHead = b.segments[0]
                const aPreviousX = aHead.previousX === undefined
                    ? aHead.x : aHead.previousX
                const aPreviousY = aHead.previousY === undefined
                    ? aHead.y : aHead.previousY
                const bPreviousX = bHead.previousX === undefined
                    ? bHead.x : bHead.previousX
                const bPreviousY = bHead.previousY === undefined
                    ? bHead.y : bHead.previousY
                const currentHeadDistance = worldDistanceSquared(
                    aHead.x, aHead.y, bHead.x, bHead.y)
                const headDistanceSquared = headDistance * headDistance
                let headsCollide = currentHeadDistance < headDistanceSquared
                if (!headsCollide) {
                    const combinedTravel = Math.sqrt(worldDistanceSquared(
                        aPreviousX, aPreviousY, aHead.x, aHead.y))
                        + Math.sqrt(worldDistanceSquared(
                            bPreviousX, bPreviousY, bHead.x, bHead.y))
                    const sweptReach = headDistance + combinedTravel
                    if (currentHeadDistance < sweptReach * sweptReach) {
                        headsCollide = worldSegmentsDistanceSquared(
                            aPreviousX, aPreviousY, aHead.x, aHead.y,
                            bPreviousX, bPreviousY, bHead.x, bHead.y)
                            < headDistanceSquared
                    }
                }
                if (headsCollide) {
                    const difference = a.segments.length - b.segments.length
                    if (Math.abs(difference) < 4) {
                        a.dying = true
                        b.dying = true
                        a.deathReason = "head"
                        b.deathReason = "head"
                    } else if (difference < 0) {
                        a.dying = true
                        a.deathReason = "head"
                    } else {
                        b.dying = true
                        b.deathReason = "head"
                    }
                }
            }
        }

        // Index body points once, then each head only checks its neighboring
        // cells. Segment spacing is smaller than every collision diameter, so
        // this preserves exact collision behavior while replacing the former
        // heads-times-all-segments scan in mature ecosystems.
        const targetCellSize = Math.max(24, baseRadius() * 6)
        const columns = Math.max(1, Math.ceil(worldWidth / targetCellSize))
        const rows = Math.max(1, Math.ceil(worldHeight / targetCellSize))
        // Derive uniform bin dimensions after rounding the bin count. Using
        // targetCellSize directly leaves a narrow final bin, which can put a
        // nearby point two cell indices away across a wraparound seam.
        const cellWidth = worldWidth / columns
        const cellHeight = worldHeight / rows
        const cells = {}
        const snakeCount = snakes.length
        for (let ownerIndex = 0; ownerIndex < snakeCount; ++ownerIndex) {
            const owner = snakes[ownerIndex]
            if (!owner.alive)
                continue
            for (let segmentIndex = 1; segmentIndex < owner.segments.length;
                    ++segmentIndex) {
                const segment = owner.segments[segmentIndex]
                const cellX = collisionCell(segment.x, cellWidth, columns, worldWidth)
                const cellY = collisionCell(segment.y, cellHeight, rows, worldHeight)
                const key = cellY * columns + cellX
                if (!cells[key])
                    cells[key] = []
                cells[key].push(segmentIndex * snakeCount + ownerIndex)
            }
        }
        safetyCells = cells
        safetyCellColumns = columns
        safetyCellRows = rows
        safetyCellWidth = cellWidth
        safetyCellHeight = cellHeight
        safetyCellSnakeCount = snakeCount

        for (let index = 0; index < snakeCount; ++index) {
            const snake = snakes[index]
            if (!snake.alive || snake.dying)
                continue
            const head = snake.segments[0]
            const headCellX = collisionCell(head.x, cellWidth, columns, worldWidth)
            const headCellY = collisionCell(head.y, cellHeight, rows, worldHeight)
            const visitedCells = []
            for (let offsetX = -1; offsetX <= 1 && !snake.dying; ++offsetX) {
                for (let offsetY = -1; offsetY <= 1 && !snake.dying; ++offsetY) {
                    let cellX = headCellX + offsetX
                    let cellY = headCellY + offsetY
                    if (!deadlyWalls) {
                        cellX = ((cellX % columns) + columns) % columns
                        cellY = ((cellY % rows) + rows) % rows
                    } else if (cellX < 0 || cellX >= columns
                               || cellY < 0 || cellY >= rows) {
                        continue
                    }
                    const key = cellY * columns + cellX
                    if (visitedCells.indexOf(key) >= 0)
                        continue
                    visitedCells.push(key)
                    const occupants = cells[key] || []
                    for (let occupantIndex = 0; occupantIndex < occupants.length;
                            ++occupantIndex) {
                        const encoded = occupants[occupantIndex]
                        const otherIndex = encoded % snakeCount
                        const segmentIndex = Math.floor(encoded / snakeCount)
                        const sameSnake = otherIndex === index
                        if ((sameSnake && !selfCollisions)
                                || (sameSnake && segmentIndex < 10))
                            continue
                        const other = snakes[otherIndex]
                        const collisionDistance = sameSnake
                            ? snake.radius * 1.48
                            : (snake.radius + other.radius) * 0.78
                        const segment = other.segments[segmentIndex]
                        const previousHeadX = head.previousX === undefined
                            ? head.x : head.previousX
                        const previousHeadY = head.previousY === undefined
                            ? head.y : head.previousY
                        const previousSegmentX = segment.previousX === undefined
                            ? segment.x : segment.previousX
                        const previousSegmentY = segment.previousY === undefined
                            ? segment.y : segment.previousY
                        const currentDistance = worldDistanceSquared(
                            head.x, head.y, segment.x, segment.y)
                        const collisionSquared = collisionDistance * collisionDistance
                        let collided = currentDistance < collisionSquared
                        if (!collided) {
                            const headTravel = Math.sqrt(worldDistanceSquared(
                                previousHeadX, previousHeadY, head.x, head.y))
                            const segmentTravel = Math.sqrt(worldDistanceSquared(
                                previousSegmentX, previousSegmentY,
                                segment.x, segment.y))
                            const sweptReach = collisionDistance
                                + headTravel + segmentTravel
                            if (currentDistance < sweptReach * sweptReach) {
                                collided = worldSegmentsDistanceSquared(
                                    previousHeadX, previousHeadY, head.x, head.y,
                                    previousSegmentX, previousSegmentY,
                                    segment.x, segment.y) < collisionSquared
                            }
                        }
                        if (collided) {
                            snake.dying = true
                            snake.deathReason = sameSnake ? "self" : "body"
                            break
                        }
                    }
                }
            }
        }
    }

    function deathParticleCount(snake) {
        if (!snake.segments || snake.segments.length === 0)
            return 0
        const thickness = snake.baseRadius > 0 ? snake.radius / snake.baseRadius : 1
        return Math.round(clamp(snake.segments.length * (0.78 + thickness * 0.22),
                                8, 720))
    }

    function explodeSnake(snake) {
        const segments = snake.segments
        const intendedCount = deathParticleCount(snake)
        const emittedCount = Math.min(intendedCount, Math.max(0, 900 - food.length))
        const foodValue = emittedCount > 0
            ? Math.max(0.65, segments.length / emittedCount) : 0
        const feastId = nextFeastId++
        for (let index = 0; index < emittedCount; ++index) {
            const position = emittedCount > 1
                ? index * (segments.length - 1) / (emittedCount - 1) : 0
            const lower = Math.floor(position)
            const upper = Math.min(segments.length - 1, lower + 1)
            const fraction = position - lower
            const interpolatedX = segments[lower].x
                + axisDelta(segments[lower].x, segments[upper].x, worldWidth) * fraction
            const interpolatedY = segments[lower].y
                + axisDelta(segments[lower].y, segments[upper].y, worldHeight) * fraction
            const segment = {
                x: deadlyWalls ? interpolatedX : wrapCoordinate(interpolatedX, worldWidth),
                y: deadlyWalls ? interpolatedY : wrapCoordinate(interpolatedY, worldHeight)
            }
            const angle = random() * Math.PI * 2
            const force = 22 + random() * 90
            addFood(segment.x + (random() - 0.5) * snake.radius,
                    segment.y + (random() - 0.5) * snake.radius,
                    foodValue * (0.8 + random() * 0.4), snake.colorIndex,
                    Math.cos(angle) * force, Math.sin(angle) * force,
                    18 + random() * 16, feastId, index, emittedCount)
        }
        snake.alive = false
        snake.respawn = 2.2 + random() * 3.2
        snake.segments = []
        ++deathCount
        if (snake.deathReason === "wall")
            ++wallDeathCount
        else if (snake.deathReason === "head")
            ++headDeathCount
        else if (snake.deathReason === "self")
            ++selfDeathCount
        else
            ++bodyDeathCount
        return emittedCount
    }

    function updateFood(seconds) {
        for (let index = food.length - 1; index >= 0; --index) {
            const particle = food[index]
            particle.attraction = 0
            if (particle.life > 0) {
                particle.life -= seconds
                if (particle.life <= 0) {
                    food.splice(index, 1)
                    continue
                }
            }
            if (Math.abs(particle.vx) + Math.abs(particle.vy) < 0.1)
                continue
            particle.x += particle.vx * seconds
            particle.y += particle.vy * seconds
            const damping = Math.pow(0.16, seconds)
            particle.vx *= damping
            particle.vy *= damping
            if (!deadlyWalls) {
                particle.x = wrapCoordinate(particle.x, worldWidth)
                particle.y = wrapCoordinate(particle.y, worldHeight)
            } else if (particle.x < 2 || particle.x > worldWidth - 2) {
                particle.x = clamp(particle.x, 2, worldWidth - 2)
                particle.vx *= -0.45
            }
            if (deadlyWalls && (particle.y < 2 || particle.y > worldHeight - 2)) {
                particle.y = clamp(particle.y, 2, worldHeight - 2)
                particle.vy *= -0.45
            }
        }
        let additions = 0
        while (food.length < desiredFoodCount && additions < 3) {
            addAmbientFood()
            ++additions
        }
    }

    function stepSimulation(seconds) {
        simulationTime += seconds
        updateFood(seconds)
        foodAnalysisCooldown -= seconds
        if (foodAnalysisCooldown <= 0) {
            analyzeFoodClusters()
            foodAnalysisCooldown = 0.32
        }
        growthSlots = Math.max(0, maximumWorldSegments() - totalLiveSegments())
        updateSnakeBrains(seconds)
        for (let index = 0; index < snakes.length; ++index) {
            const snake = snakes[index]
            if (!snake.alive) {
                snake.respawn -= seconds
                if (snake.respawn <= 0)
                    snakes[index] = makeSnake(index)
                continue
            }
            moveSnake(snake, index, seconds)
        }
        feedSnakes(seconds)
        markCollisions()
        for (let index = 0; index < snakes.length; ++index) {
            if (snakes[index].alive && snakes[index].dying)
                explodeSnake(snakes[index])
        }
    }

    function advance(deltaSeconds) {
        if (snakes.length !== desiredSnakeCount) {
            initializeWorld()
        } else {
            synchronizeWorldGeometry()
        }
        accumulator += Math.min(0.1, Math.max(0, deltaSeconds))
        const fixedStep = 1 / 60
        let steps = 0
        while (accumulator >= fixedStep && steps < 6) {
            stepSimulation(fixedStep)
            accumulator -= fixedStep
            ++steps
        }
        if (steps === 6 && accumulator > fixedStep * 6)
            accumulator = fixedStep
        renderAlpha = clamp(accumulator / fixedStep, 0, 1)
        requestFrame()
    }

    function drawParticles(painter) {
        for (let index = 0; index < food.length; ++index) {
            const particle = food[index]
            particle.renderSize = particle.size
                * (0.82 + Math.sin(simulationTime * 3 + particle.phase) * 0.18)
        }

        painter.globalAlpha = 0.55
        painter.lineCap = "round"
        for (let index = 0; index < food.length; ++index) {
            const particle = food[index]
            if (!particle.attraction || particle.attraction <= 0)
                continue
            const dx = axisDelta(particle.x, particle.attractionX, worldWidth)
            const dy = axisDelta(particle.y, particle.attractionY, worldHeight)
            const distance = Math.max(0.001, Math.sqrt(dx * dx + dy * dy))
            const trailLength = particle.size * (2 + particle.attraction * 5)
            painter.strokeStyle = palette[particle.colorIndex % palette.length]
            painter.lineWidth = Math.max(1, particle.size * 0.75)
            painter.beginPath()
            painter.moveTo(particle.x, particle.y)
            painter.lineTo(particle.x - dx / distance * trailLength,
                           particle.y - dy / distance * trailLength)
            painter.stroke()
        }

        painter.globalAlpha = 0.13
        for (let colorIndex = 0; colorIndex < palette.length; ++colorIndex) {
            painter.fillStyle = palette[colorIndex]
            painter.beginPath()
            for (let index = 0; index < food.length; ++index) {
                const particle = food[index]
                if (particle.colorIndex % palette.length !== colorIndex)
                    continue
                painter.arc(particle.x, particle.y,
                            particle.renderSize * 3.2, 0, Math.PI * 2)
            }
            painter.fill()
        }

        painter.globalAlpha = 0.9
        for (let colorIndex = 0; colorIndex < palette.length; ++colorIndex) {
            painter.fillStyle = palette[colorIndex]
            painter.beginPath()
            for (let index = 0; index < food.length; ++index) {
                const particle = food[index]
                if (particle.colorIndex % palette.length !== colorIndex)
                    continue
                painter.arc(particle.x, particle.y,
                            particle.renderSize, 0, Math.PI * 2)
            }
            painter.fill()
        }

        painter.globalAlpha = 0.85
        painter.fillStyle = "#ffffff"
        painter.beginPath()
        for (let index = 0; index < food.length; ++index) {
            const particle = food[index]
            const size = particle.renderSize
            painter.arc(particle.x - size * 0.24, particle.y - size * 0.24,
                        Math.max(0.7, size * 0.3), 0, Math.PI * 2)
        }
        painter.fill()
    }

    function renderSegments(snake) {
        // Deadly-wall worlds never cross a seam, so paint the simulation's
        // existing point array directly. This is the common path and avoids
        // allocating hundreds of temporary JavaScript objects every frame.
        if (deadlyWalls)
            return snake.segments
        const displaySegments = []
        for (let index = 0; index < snake.segments.length; ++index) {
            const segment = snake.segments[index]
            const previousX = segment.previousX === undefined ? segment.x : segment.previousX
            const previousY = segment.previousY === undefined ? segment.y : segment.previousY
            const x = previousX + axisDelta(previousX, segment.x, worldWidth) * renderAlpha
            const y = previousY + axisDelta(previousY, segment.y, worldHeight) * renderAlpha
            displaySegments.push({
                x: deadlyWalls ? x : wrapCoordinate(x, worldWidth),
                y: deadlyWalls ? y : wrapCoordinate(y, worldHeight)
            })
        }
        const rendered = [{ x: displaySegments[0].x, y: displaySegments[0].y }]
        for (let index = 1; index < displaySegments.length; ++index) {
            rendered.push({
                x: rendered[index - 1].x
                   + axisDelta(displaySegments[index - 1].x,
                               displaySegments[index].x, worldWidth),
                y: rendered[index - 1].y
                   + axisDelta(displaySegments[index - 1].y,
                               displaySegments[index].y, worldHeight)
            })
        }
        return rendered
    }

    function drawSnakeCopy(painter, snake, leaderLength, segments, offsetX, offsetY) {
        const color = palette[snake.colorIndex % palette.length]
        const tail = segments[segments.length - 1]

        painter.lineCap = "round"
        painter.lineJoin = "round"
        painter.beginPath()
        painter.moveTo(tail.x + offsetX, tail.y + offsetY)
        for (let index = segments.length - 2; index >= 0; --index)
            painter.lineTo(segments[index].x + offsetX, segments[index].y + offsetY)
        painter.globalAlpha = 0.42
        painter.strokeStyle = "#050710"
        painter.lineWidth = snake.radius * 2.55
        painter.stroke()
        painter.globalAlpha = 0.96
        painter.strokeStyle = color
        painter.lineWidth = snake.radius * 1.92
        painter.stroke()
        painter.globalAlpha = 0.22
        painter.strokeStyle = "#ffffff"
        painter.lineWidth = Math.max(1, snake.radius * 0.42)
        painter.stroke()

        painter.fillStyle = "#ffffff"
        painter.globalAlpha = 0.18
        painter.beginPath()
        for (let index = 5; index < segments.length; index += 6) {
            painter.arc(segments[index].x + offsetX, segments[index].y + offsetY,
                        snake.radius * 0.34, 0, Math.PI * 2)
        }
        painter.fill()

        const head = segments[0]
        painter.globalAlpha = 1
        painter.fillStyle = color
        painter.beginPath()
        painter.arc(head.x + offsetX, head.y + offsetY,
                    snake.radius * 1.08, 0, Math.PI * 2)
        painter.fill()

        const forwardX = Math.cos(snake.angle)
        const forwardY = Math.sin(snake.angle)
        const sideX = -forwardY
        const sideY = forwardX
        const eyeForward = snake.radius * 0.48
        const eyeSide = snake.radius * 0.46
        const eyeRadius = Math.max(1.7, snake.radius * 0.31)
        painter.fillStyle = "#ffffff"
        painter.beginPath()
        for (let side = -1; side <= 1; side += 2) {
            const eyeX = head.x + offsetX + forwardX * eyeForward + sideX * eyeSide * side
            const eyeY = head.y + offsetY + forwardY * eyeForward + sideY * eyeSide * side
            painter.arc(eyeX, eyeY, eyeRadius, 0, Math.PI * 2)
        }
        painter.fill()

        painter.fillStyle = "#11131a"
        painter.beginPath()
        for (let side = -1; side <= 1; side += 2) {
            const eyeX = head.x + offsetX + forwardX * eyeForward + sideX * eyeSide * side
            const eyeY = head.y + offsetY + forwardY * eyeForward + sideY * eyeSide * side
            painter.arc(eyeX + forwardX * eyeRadius * 0.34,
                        eyeY + forwardY * eyeRadius * 0.34,
                        eyeRadius * 0.48, 0, Math.PI * 2)
        }
        painter.fill()

        if (segments.length === leaderLength) {
            const crownCenterX = head.x + offsetX - forwardX * snake.radius * 0.32
            const crownCenterY = head.y + offsetY - forwardY * snake.radius * 0.32
            function crownPoint(sideAmount, forwardAmount) {
                return {
                    x: crownCenterX + sideX * snake.radius * sideAmount
                       + forwardX * snake.radius * forwardAmount,
                    y: crownCenterY + sideY * snake.radius * sideAmount
                       + forwardY * snake.radius * forwardAmount
                }
            }
            const points = [crownPoint(-0.82, -0.42), crownPoint(-0.82, 0.58),
                            crownPoint(-0.34, 0.18), crownPoint(0, 0.98),
                            crownPoint(0.34, 0.18), crownPoint(0.82, 0.58),
                            crownPoint(0.82, -0.42)]
            painter.fillStyle = "#ffd84a"
            painter.strokeStyle = "#8b5a00"
            painter.lineWidth = Math.max(1, snake.radius * 0.13)
            painter.beginPath()
            painter.moveTo(points[0].x, points[0].y)
            for (let index = 1; index < points.length; ++index)
                painter.lineTo(points[index].x, points[index].y)
            painter.closePath()
            painter.fill()
            painter.stroke()
            painter.fillStyle = "#fff2a0"
            painter.beginPath()
            painter.arc(crownCenterX + forwardX * snake.radius * 0.04,
                        crownCenterY + forwardY * snake.radius * 0.04,
                        Math.max(0.8, snake.radius * 0.12), 0, Math.PI * 2)
            painter.fill()
        }
    }

    function drawSnake(painter, snake, leaderLength) {
        if (!snake.alive || snake.segments.length < 2)
            return
        const segments = renderSegments(snake)
        if (deadlyWalls) {
            drawSnakeCopy(painter, snake, leaderLength, segments, 0, 0)
            return
        }

        let minimumX = segments[0].x
        let maximumX = segments[0].x
        let minimumY = segments[0].y
        let maximumY = segments[0].y
        for (let index = 1; index < segments.length; ++index) {
            minimumX = Math.min(minimumX, segments[index].x)
            maximumX = Math.max(maximumX, segments[index].x)
            minimumY = Math.min(minimumY, segments[index].y)
            maximumY = Math.max(maximumY, segments[index].y)
        }
        const margin = snake.radius * 3
        const xOffsets = wrappingOffsets(minimumX, maximumX, worldWidth, margin)
        const yOffsets = wrappingOffsets(minimumY, maximumY, worldHeight, margin)
        for (let xIndex = 0; xIndex < xOffsets.length; ++xIndex) {
            for (let yIndex = 0; yIndex < yOffsets.length; ++yIndex) {
                drawSnakeCopy(painter, snake, leaderLength, segments,
                              xOffsets[xIndex], yOffsets[yIndex])
            }
        }
    }

    Canvas {
        id: snakeCanvas
        visible: root.nativeRenderer === null
        width: Math.max(1, Math.ceil(root.width * root.canvasScale))
        height: Math.max(1, Math.ceil(root.height * root.canvasScale))
        transformOrigin: Item.TopLeft
        scale: 1 / root.canvasScale
        renderTarget: Canvas.FramebufferObject
        renderStrategy: Canvas.Cooperative
        onPaint: {
            const painter = getContext("2d")
            painter.reset()
            painter.clearRect(0, 0, width, height)
            painter.save()
            painter.scale(root.canvasScale, root.canvasScale)
            painter.translate(root.drawOffsetX, root.drawOffsetY)
            root.drawParticles(painter)

            let leaderLength = 0
            for (let index = 0; index < root.snakes.length; ++index) {
                if (root.snakes[index].alive)
                    leaderLength = Math.max(leaderLength, root.snakes[index].segments.length)
            }
            const ordered = root.snakes.slice().sort(function(left, right) {
                return left.segments.length - right.segments.length
            })
            for (let index = 0; index < ordered.length; ++index)
                root.drawSnake(painter, ordered[index], leaderLength)
            painter.restore()
        }
    }

    FrameClock {
        presentationClock: root.context ? root.context.presentationClock : null
        running: !root.reducedMotion && root.simulationDriver
        onTick: function(deltaSeconds) { root.advance(deltaSeconds) }
    }

    Timer {
        id: initializeTimer
        interval: 1
        repeat: false
        onTriggered: root.synchronizeWorldGeometry()
    }

    onContextChanged: initializeTimer.restart()
    onNativeRendererChanged: requestFrame()
    onWorldWidthChanged: initializeTimer.restart()
    onWorldHeightChanged: initializeTimer.restart()
    Component.onCompleted: initializeTimer.restart()
}
