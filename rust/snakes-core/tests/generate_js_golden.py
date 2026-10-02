#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Extract unchanged mechanics functions from QML and run them under Node.
No packages/network. Regenerate: python3 tests/generate_js_golden.py (crate cwd).
World AI/safety hooks are replaced with deterministic scripted headings.
"""
import hashlib
import json
from pathlib import Path
import re
import subprocess

crate = Path(__file__).resolve().parents[1]
source = (crate.parents[1] / 'tests/parity/snakes/oracle/Snakes.qml').read_text()
names = '''random clamp normalizeAngle distanceSquared wrapCoordinate axisDelta
worldDistanceSquared worldSegmentDistanceSquared planarPointSegmentDistanceSquared
crossProduct pointOnPlanarSegment worldSegmentsDistanceSquared baseRadius
maximumSnakeSegments maximumWorldSegments totalLiveSegments segmentGrowthCost
spawnPosition makeSnake rebuildSnakeTrail ensureSnakeTrail appendHeadTrailPoint
placeSegmentsOnTrail addFood addAmbientFood initializeWorld minimumSnakeTurnRadius
snakeTurnRate snakeSpeed steerSnake updateSnakeRadius moveSnake consumeFoodParticle
feedSnakes collisionCell markCollisions deathParticleCount makeRoomForDeathFood
explodeSnake updateFood stepSimulation'''.split()
functions = []
for name in names:
    start = re.search(r'    function ' + name + r'\(', source).start()
    brace = source.index('{', start)
    depth = 1
    end = brace + 1
    while depth:
        depth += (source[end] == '{') - (source[end] == '}')
        end += 1
    functions.append(source[start:end])
setup = '''
let context, seed, worldWidth, worldHeight, snakes, food, palette,
randomState, accumulator, renderAlpha, simulationTime, nextFeastId, nextFoodId,
foodAnalysisCooldown, growthSlots, deathCount, wallDeathCount, headDeathCount,
bodyDeathCount, selfDeathCount, brainCursor, activeBrainPlan, lastBrainWorkUnits,
safetyCells, safetyUsedCells, safetyCellColumns, safetyCellRows,
safetyCellSnakeCount, safetyCellWidth, safetyCellHeight, initializedWidth,
initializedHeight, desiredSnakeCount, desiredFoodCount, maximumFoodCount,
intelligence, deadlyWalls, selfCollisions, scriptTick;
function requestFrame() {}
function analyzeFoodClusters() {}
function applyCollisionSafety() {}
function applyWallSafety() {}
function updateSnakeBrains() {
    for (let i=0; i<snakes.length; ++i) if (snakes[i].alive) {
        snakes[i].desiredAngle=scriptTick*0.017+i*0.41;
        snakes[i].rush=scriptTick%100<20 ? 0.12 : 0;
    }
}
function snapshot() {
    return {tick:scriptTick,rng:randomState,deaths:deathCount,
        traits:snakes.map(s=>[s.speedBias,s.turnBias,s.wanderPhase,s.aggression]),
        snake:snakes.map(s=>[s.alive?1:0,s.segments.length,s.radius,s.angle,s.growth,
            s.segments.length?s.segments[0].x:0,s.segments.length?s.segments[0].y:0]),
        food:[food.length,...['x','y','value','life'].map(k=>food.reduce((a,f)=>a+f[k],0))],
        segments:['x','y','previousX','previousY'].map(k=>snakes.reduce((a,s)=>a+s.segments.reduce((b,p)=>b+p[k],0),0))};
}
let result=[];
for (let scenario of [0,1,2]) {
    seed=[73,20260814,-2147483648][scenario];
    worldWidth=scenario===1?3440:1280;worldHeight=scenario===1?1440:720;
    context={animationScale:100,animationSpeed:100};
    palette=new Array(7);desiredSnakeCount=scenario===1?14:9;
    desiredFoodCount=scenario===1?183:82;maximumFoodCount=desiredFoodCount+260;
    intelligence=scenario===1?1:0.75;deadlyWalls=scenario!==2;selfCollisions=true;
    initializeWorld();scriptTick=0;let snapshots=[snapshot()];
    for (;scriptTick<600;) {
        stepSimulation(1/30);++scriptTick;
        if ([1,30,120,600].includes(scriptTick)) snapshots.push(snapshot());
    }
    result.push(snapshots);
}
let sequences=[];
for (let s of [0,1,73,-73,20260814,2147483647,-2147483648]) {
    randomState=(Math.abs(s)+1)*2654435761%4294967296;
    if (randomState<1) randomState=1;
    const initial=randomState;let values=[];
    for (let i=0;i<8;++i) {random();values.push(randomState);}
    sequences.push([s,initial,values]);
}
console.log(JSON.stringify({result,sequences}));
'''
# Declarations must precede calls in setup; insert extracted functions before
# the scenario loop, after all variable declarations and stub hooks.
setup = setup.replace('let result=[];', '\n'.join(functions) + '\nlet result=[];')
data = json.loads(subprocess.check_output(['/usr/bin/node'], input=setup.encode()))
lines = ['// SPDX-License-Identifier: GPL-3.0-or-later',
         '// Generated from unmodified QML mechanics by tests/generate_js_golden.py.',
         '// Source SHA-256: ' + hashlib.sha256(source.encode()).hexdigest(),
         'use super::*;', '#[test] fn rng_matches_js_f64_seed_and_lcg() {']
for seed, initial, values in data['sequences']:
    lines += [f'let mut rng=WorldRng::new({seed});assert_eq!(rng.state(),{initial});',
              f'for expected in {values!r} {{rng.random();assert_eq!(rng.state(),expected);}}']
lines += ['}', '#[test] fn scripted_mechanics_match_unmodified_javascript() {']
for scenario, snapshots in enumerate(data['result']):
    cfg = ('width:3440.0,height:1440.0,density:100.0,trails:100.0,intelligence:100.0,'
           if scenario == 1 else '')
    cfg += f'seed:{[73,20260814,-2147483648][scenario]},self_collisions:true,deadly_walls:{str(scenario!=2).lower()},'
    lines += ['{', f'let mut w=World::new(Config{{{cfg}..Config::default()}}).unwrap();',
              'let mut c=ScriptedController::new(|tick,s:SnakeView<\'_>|Steering{desired_angle:tick as f64*0.017+s.id as f64*0.41,rush:if tick%100<20 {0.12} else {0.0}});']
    for snap in snapshots:
        tick=snap['tick']
        lines += [f'while w.tick()<{tick} {{w.step(&mut c);}}' if tick else 'assert_eq!(w.tick(),0);',
                  f'assert_eq!(w.rng_state(),{snap["rng"]},"scenario {scenario} tick {tick} RNG");',
                  f'assert_eq!(w.stats().deaths,{snap["deaths"]});']
        for i, values in enumerate(snap['snake']):
            alive, length = values[:2]
            radius, angle, growth, x, y = map(float, values[2:])
            lines += [f'assert_eq!(w.snakes[{i}].alive,{str(bool(alive)).lower()});assert_eq!(w.snakes[{i}].len,{length});',
                      f'close(w.snakes[{i}].radius,{radius!r});close(w.snakes[{i}].angle,{angle!r});close(w.snakes[{i}].growth,{growth!r});']
            if alive:
                lines += [f'close(w.segments[{i}*MAX_SEGMENTS].current.x,{x!r});close(w.segments[{i}*MAX_SEGMENTS].current.y,{y!r});']
        for i, traits in enumerate(snap['traits']):
            for field, expected in zip(['speed_bias','turn_bias','wander_phase','aggression'], traits):
                lines += [f'close(w.snakes[{i}].traits.{field},{float(expected)!r});']
        n, x, y, value, life = snap['food']
        lines += [f'assert_eq!(w.food.len(),{n});',
                  f'close(w.food.iter().map(|f|f.p.x).sum(),{x!r});close(w.food.iter().map(|f|f.p.y).sum(),{y!r});',
                  f'close(w.food.iter().map(|f|f.value).sum(),{value!r});close(w.food.iter().map(|f|f.life).sum(),{life!r});']
        for k, expected in enumerate(snap['segments']):
            expected = float(expected)
            field = ['current.x','current.y','previous.x','previous.y'][k]
            lines += [f'close(w.snakes().flat_map(|s|s.segments).map(|s|s.{field}).sum(),{expected!r});']
    lines += ['}']
lines += ['}']
(crate/'src/world/golden.rs').write_text('\n'.join(lines)+'\n')
