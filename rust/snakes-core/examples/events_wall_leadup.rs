// SPDX-License-Identifier: GPL-3.0-or-later
//! Replay the 0.17 F1 seed-73 wall deaths with production RNG and controls.
//! Run through heavy; logging is diagnostic-only and outside steady-state AI.
#[cfg(feature = "desktop-diag")]
mod replay {
    use snakes_core::{
        ai::AiController,
        controller::{Controller, FaceIntent, Steering},
        *,
    };
    struct Observer {
        ai: AiController,
        from: u64,
        ids: Vec<u32>,
    }
    impl Controller for Observer {
        fn face_intent(&self, id: u32) -> FaceIntent {
            self.ai.face_intent(id)
        }
        fn intent_flags(&self, id: u32) -> Option<u32> {
            self.ai.intent_flags(id)
        }
        fn steer(&mut self, w: &World, s: SnakeView<'_>) -> Steering {
            let out = self.ai.steer(w, s);
            if w.tick() >= self.from && self.ids.contains(&s.id) {
                let d = self.ai.decision(s.id as usize);
                let o = self.ai.desktop_observation(s.id as usize);
                let (speed, turn) = w.motion_limits(s.id as usize, out.rush).unwrap();
                println!(
                    "pre tick={} id={} gen={} len={} head={:?} angle={:.16} radius={:.16} speed={:.16} turn={:.16} frozen={} effect={}/{} night={} selected={} safe={} horizon={} mode={} target={} prey={} goal={:?} desired={:.16} rush={} candidates={:?}",
                    w.tick(),
                    s.id,
                    s.generation,
                    s.segments.len(),
                    s.segments[0].current,
                    s.angle,
                    s.radius,
                    speed,
                    turn,
                    s.face.frozen_ticks,
                    s.effect_kind,
                    s.effect_ticks,
                    w.world_event.night,
                    d.selected,
                    o.safe_ticks,
                    d.horizon,
                    o.mode,
                    o.target,
                    o.prey,
                    d.goal,
                    out.desired_angle,
                    out.rush,
                    d.candidates.map(|c| (c.safe_ticks, c.desired, c.rush))
                );
                if w.tick() % 60 == 0 {
                    println!(
                        "body tick={} id={} traits={:?} segments={:?}",
                        w.tick(),
                        s.id,
                        s.traits,
                        s.segments
                    );
                }
            }
            out
        }
    }
    pub fn run() {
        let args: Vec<String> = std::env::args().collect();
        let number = |key: &str, default: u64| {
            args.iter()
                .find_map(|a| a.strip_prefix(key).and_then(|v| v.parse().ok()))
                .unwrap_or(default)
        };
        let from = number("--from=", 21000);
        let ticks = number("--ticks=", 23300);
        let ids = args
            .iter()
            .find_map(|a| a.strip_prefix("--ids="))
            .unwrap_or("1,11")
            .split(',')
            .map(|id| id.parse().unwrap())
            .collect();
        let mut w = World::new(Config {
            width: 3440.0,
            height: 1440.0,
            density: 80.0,
            scale: 200.0,
            speed: 300.0,
            trails: 100.0,
            intelligence: 100.0,
            self_collisions: true,
            deadly_walls: true,
            rules: RuleSet::V2,
            seed: number("--seed=", 73) as i32,
            ..Default::default()
        })
        .unwrap();
        w.resize(5360.0, 1440.0).unwrap();
        w.resize(7920.0, 1440.0).unwrap();
        let mut ai = Observer {
            ai: AiController::new(),
            from,
            ids,
        };
        ai.ai.enable_diagnostics();
        while w.tick() < ticks {
            w.step(&mut ai);
            if w.tick() >= from {
                for e in w.collision_events() {
                    println!("death {e:?}");
                }
            }
        }
        println!("stats {:?}", w.stats());
    }
}
#[cfg(feature = "desktop-diag")]
fn main() {
    replay::run();
}
#[cfg(not(feature = "desktop-diag"))]
fn main() {
    panic!("enable desktop-diag");
}
