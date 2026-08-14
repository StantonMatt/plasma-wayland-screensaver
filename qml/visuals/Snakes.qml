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
        return {
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
            score: length
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
            attractionY: y
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
            growthSlots: growthSlots
        })
    }

    function resizeSimulationWorld(sourceWidth, sourceHeight) {
        if (sourceWidth <= 0 || sourceHeight <= 0 || worldWidth <= 0 || worldHeight <= 0) {
            initializeWorld()
            return
        }
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
        food = state.food
        accumulator = state.accumulator
        renderAlpha = state.renderAlpha
        simulationTime = state.simulationTime
        randomState = state.randomState
        nextFeastId = state.nextFeastId
        nextFoodId = state.nextFoodId
        foodAnalysisCooldown = state.foodAnalysisCooldown
        growthSlots = state.growthSlots
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

    // Build the local obstacle set once per decision. Earlier versions scanned
    // every body segment again for every candidate heading, making dense worlds
    // expensive and still reacting too late.
    function collectHazards(snake, snakeIndex, horizon) {
        const head = snake.segments[0]
        const range = horizon + snake.radius * 5
        const rangeSquared = range * range
        const hazards = []
        for (let otherIndex = 0; otherIndex < snakes.length; ++otherIndex) {
            const other = snakes[otherIndex]
            if (!other.alive)
                continue
            const sameSnake = otherIndex === snakeIndex
            if (sameSnake && !selfCollisions)
                continue
            const firstSegment = sameSnake ? Math.min(8, other.segments.length) : 0
            const stride = sameSnake
                ? Math.max(1, Math.ceil(other.segments.length / 120))
                : (intelligence >= 0.7 ? 2 : 3)
            const otherSpeed = sameSnake ? 0 : snakeSpeed(other)
            for (let segmentIndex = firstSegment;
                    segmentIndex < other.segments.length; segmentIndex += stride) {
                const segment = other.segments[segmentIndex]
                const dx = axisDelta(head.x, segment.x, worldWidth)
                const dy = axisDelta(head.y, segment.y, worldHeight)
                if (dx * dx + dy * dy > rangeSquared)
                    continue
                hazards.push({
                    x: segment.x,
                    y: segment.y,
                    radius: other.radius,
                    self: sameSnake,
                    vx: segmentIndex === 0 ? Math.cos(other.angle) * otherSpeed : 0,
                    vy: segmentIndex === 0 ? Math.sin(other.angle) * otherSpeed : 0
                })
            }
        }
        return hazards
    }

    // Follow a curved prospective route and compare it with both bodies and
    // predicted rival-head positions. This accounts for the fact that a snake
    // cannot instantly rotate onto a safe straight ray.
    function headingRisk(snake, snakeIndex, angle, horizon, suppliedHazards) {
        const hazards = suppliedHazards || collectHazards(snake, snakeIndex, horizon)
        const head = snake.segments[0]
        const samples = 6
        const stepDistance = horizon / samples
        const turn = normalizeAngle(angle - snake.angle)
        const speed = Math.max(1, snakeSpeed(snake))
        const safetyScale = 1.12 + intelligence * 1.12
        let projectedX = head.x
        let projectedY = head.y
        let risk = 0
        for (let sample = 1; sample <= samples; ++sample) {
            const progress = sample / samples
            const routeAngle = snake.angle + turn * progress
            projectedX += Math.cos(routeAngle) * stepDistance
            projectedY += Math.sin(routeAngle) * stepDistance
            const margin = snake.radius * safetyScale
            if (deadlyWalls && (projectedX < margin || projectedX > worldWidth - margin
                    || projectedY < margin || projectedY > worldHeight - margin)) {
                risk += (samples + 1 - sample) * 12
            }

            const elapsed = stepDistance * sample / speed
            for (let hazardIndex = 0; hazardIndex < hazards.length; ++hazardIndex) {
                const hazard = hazards[hazardIndex]
                const hazardX = hazard.x + hazard.vx * elapsed
                const hazardY = hazard.y + hazard.vy * elapsed
                const dx = axisDelta(projectedX, hazardX, worldWidth)
                const dy = axisDelta(projectedY, hazardY, worldHeight)
                const distance = Math.sqrt(dx * dx + dy * dy)
                const selfScale = hazard.self ? 1.32 : 1
                const clearance = (snake.radius + hazard.radius) * safetyScale * selfScale
                if (distance < clearance) {
                    risk += (1 - distance / Math.max(1, clearance))
                            * (samples + 1 - sample) * (12 + intelligence * 24)
                            * (hazard.self ? 1.8 : 1)
                } else if (distance < clearance * 1.8) {
                    risk += (1 - (distance - clearance) / (clearance * 0.8))
                            * (samples + 1 - sample) * (0.8 + intelligence * 2.2)
                            * (hazard.self ? 1.5 : 1)
                }
            }
        }
        return risk
    }

    function planSteering(snake, snakeIndex) {
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
        let steerX = axisDelta(head.x, goal.x, worldWidth)
        let steerY = axisDelta(head.y, goal.y, worldHeight)
        const goalLength = Math.max(1, Math.sqrt(steerX * steerX + steerY * steerY))
        steerX /= goalLength
        steerY /= goalLength

        let avoidX = 0
        let avoidY = 0
        const boundaryMargin = Math.max(70 + intelligence * 145,
                                        snake.radius * (9 + intelligence * 10))
        if (deadlyWalls) {
            if (head.x < boundaryMargin)
                avoidX += (boundaryMargin - head.x) / boundaryMargin * 7
            if (head.x > worldWidth - boundaryMargin)
                avoidX -= (head.x - (worldWidth - boundaryMargin)) / boundaryMargin * 7
            if (head.y < boundaryMargin)
                avoidY += (boundaryMargin - head.y) / boundaryMargin * 7
            if (head.y > worldHeight - boundaryMargin)
                avoidY -= (head.y - (worldHeight - boundaryMargin)) / boundaryMargin * 7
        }

        const forwardX = Math.cos(snake.angle)
        const forwardY = Math.sin(snake.angle)
        const horizon = Math.max(90 + intelligence * 260,
                                 snake.radius * (10 + intelligence * 15))
        const hazards = collectHazards(snake, snakeIndex, horizon)
        const senseDistance = horizon * 0.72
        const senseSquared = senseDistance * senseDistance
        for (let hazardIndex = 0; hazardIndex < hazards.length; ++hazardIndex) {
            const hazard = hazards[hazardIndex]
            const towardX = axisDelta(head.x, hazard.x, worldWidth)
            const towardY = axisDelta(head.y, hazard.y, worldHeight)
            const distanceSquared = towardX * towardX + towardY * towardY
            if (distanceSquared < 0.001 || distanceSquared >= senseSquared)
                continue
            const distance = Math.sqrt(distanceSquared)
            const forwardness = towardX / distance * forwardX + towardY / distance * forwardY
            if (forwardness < -0.2)
                continue
            const strength = Math.pow(1 - distance / senseDistance, 2)
                             * (5 + intelligence * 18)
                             * (hazard.self ? 1.65 : 1)
            avoidX -= towardX / distance * strength
            avoidY -= towardY / distance * strength
        }

        const wander = (Math.sin(simulationTime * (0.7 + snake.speedBias * 0.2)
                                 + snake.wanderPhase) * 0.24 + snake.turnBias)
                       * (1 - intelligence * 0.82)
        const goalAngle = Math.atan2(steerY + avoidY + forwardX * wander,
                                     steerX + avoidX - forwardY * wander)
        const spread = 0.4 + intelligence * 1.15
        const candidates = [goalAngle, snake.angle,
                            snake.angle - spread * 0.3, snake.angle + spread * 0.3,
                            snake.angle - spread * 0.62, snake.angle + spread * 0.62,
                            snake.angle - spread, snake.angle + spread]
        let desiredAngle = goalAngle
        let bestScore = Number.MAX_VALUE
        for (let candidateIndex = 0; candidateIndex < candidates.length; ++candidateIndex) {
            const candidate = normalizeAngle(candidates[candidateIndex])
            const risk = headingRisk(snake, snakeIndex, candidate, horizon, hazards)
            const goalCost = Math.abs(normalizeAngle(candidate - goalAngle))
            const turnCost = Math.abs(normalizeAngle(candidate - snake.angle)) * 0.15
            const memoryCost = Math.abs(normalizeAngle(candidate - snake.desiredAngle))
                               * (0.1 + intelligence * 0.45)
            const score = risk * (0.6 + intelligence * 7.4)
                          + goalCost + turnCost + memoryCost
            if (score < bestScore) {
                bestScore = score
                desiredAngle = candidate
            }
        }
        return desiredAngle
    }

    function steerSnake(snake, snakeIndex, seconds) {
        snake.brainCooldown -= seconds
        if (snake.brainCooldown <= 0) {
            snake.desiredAngle = planSteering(snake, snakeIndex)
            snake.brainCooldown = 0.13 - intelligence * 0.07
                                  + (snakeIndex % 3) * 0.004
        }
        const difference = normalizeAngle(snake.desiredAngle - snake.angle)
        const maxTurn = snakeTurnRate(snake) * seconds
        snake.angle = normalizeAngle(snake.angle + clamp(difference, -maxTurn, maxTurn))
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
        steerSnake(snake, snakeIndex, seconds)
        const speed = snakeSpeed(snake)
        const head = snake.segments[0]
        head.x += Math.cos(snake.angle) * speed * seconds
        head.y += Math.sin(snake.angle) * speed * seconds
        if (!deadlyWalls) {
            head.x = wrapCoordinate(head.x, worldWidth)
            head.y = wrapCoordinate(head.y, worldHeight)
        }

        const spacing = snake.radius * 1.18
        if (growthAllowed && snake.growth >= growthCost) {
            snake.growthStretch = Math.min(1, (snake.growthStretch || 0)
                + speed * seconds / Math.max(1, spacing) * 0.62)
        } else {
            snake.growthStretch = 0
        }
        for (let segmentIndex = 1; segmentIndex < snake.segments.length; ++segmentIndex) {
            const previous = snake.segments[segmentIndex - 1]
            const segment = snake.segments[segmentIndex]
            const dx = axisDelta(segment.x, previous.x, worldWidth)
            const dy = axisDelta(segment.y, previous.y, worldHeight)
            const distance = Math.max(0.001, Math.sqrt(dx * dx + dy * dy))
            const targetSpacing = segmentIndex === 1
                ? spacing * (1 + (snake.growthStretch || 0)) : spacing
            segment.x = previous.x - dx / distance * targetSpacing
            segment.y = previous.y - dy / distance * targetSpacing
            if (!deadlyWalls) {
                segment.x = wrapCoordinate(segment.x, worldWidth)
                segment.y = wrapCoordinate(segment.y, worldHeight)
            }
        }

        if (growthAllowed && snake.growth >= growthCost && snake.growthStretch >= 1) {
            const head = snake.segments[0]
            const oldNeck = snake.segments[1]
            const dx = axisDelta(head.x, oldNeck.x, worldWidth)
            const dy = axisDelta(head.y, oldNeck.y, worldHeight)
            const distance = Math.max(0.001, Math.sqrt(dx * dx + dy * dy))
            let neckX = head.x + dx / distance * spacing
            let neckY = head.y + dy / distance * spacing
            if (!deadlyWalls) {
                neckX = wrapCoordinate(neckX, worldWidth)
                neckY = wrapCoordinate(neckY, worldHeight)
            }
            snake.segments.splice(1, 0, {
                x: neckX, y: neckY, previousX: neckX, previousY: neckY
            })
            snake.growth -= growthCost
            snake.growthStretch = 0
            growthSlots = Math.max(0, growthSlots - 1)
            updateSnakeRadius(snake)
        }
    }

    function feedSnakes(seconds, onlySnake) {
        for (let index = food.length - 1; index >= 0; --index) {
            const particle = food[index]
            let eater = null
            let closestDistance = Number.MAX_VALUE
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
                }
            }
            if (!eater)
                continue
            const head = eater.segments[0]
            const eatingDistance = eater.radius * 1.16 + particle.size
            if (closestDistance <= eatingDistance) {
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
                continue
            }
            const pull = clamp(1 - (closestDistance - eatingDistance)
                               / Math.max(1, eater.radius * 1.84), 0.08, 1)
            const travel = Math.min(closestDistance - eatingDistance,
                                    (55 + pull * 330) * seconds)
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

    function markCollisions() {
        for (let index = 0; index < snakes.length; ++index) {
            const snake = snakes[index]
            snake.dying = false
            if (!snake.alive)
                continue
            const head = snake.segments[0]
            if (deadlyWalls
                    && (head.x < 0 || head.x > worldWidth
                        || head.y < 0 || head.y > worldHeight))
                snake.dying = true
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
                if (worldDistanceSquared(a.segments[0].x, a.segments[0].y,
                                         b.segments[0].x, b.segments[0].y)
                        < headDistance * headDistance) {
                    const difference = a.segments.length - b.segments.length
                    if (Math.abs(difference) < 4) {
                        a.dying = true
                        b.dying = true
                    } else if (difference < 0) {
                        a.dying = true
                    } else {
                        b.dying = true
                    }
                }
            }
        }

        for (let index = 0; index < snakes.length; ++index) {
            const snake = snakes[index]
            if (!snake.alive || snake.dying)
                continue
            const head = snake.segments[0]
            for (let otherIndex = 0; otherIndex < snakes.length && !snake.dying; ++otherIndex) {
                const sameSnake = otherIndex === index
                if (sameSnake && !selfCollisions)
                    continue
                const other = snakes[otherIndex]
                if (!other.alive)
                    continue
                const collisionDistance = sameSnake
                    ? snake.radius * 1.42
                    : (snake.radius + other.radius) * 0.72
                const collisionSquared = collisionDistance * collisionDistance
                const firstSegment = sameSnake ? Math.min(10, other.segments.length) : 3
                for (let segmentIndex = firstSegment;
                        segmentIndex < other.segments.length; ++segmentIndex) {
                    const segment = other.segments[segmentIndex]
                    if (worldDistanceSquared(head.x, head.y, segment.x, segment.y)
                            < collisionSquared) {
                        snake.dying = true
                        break
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
