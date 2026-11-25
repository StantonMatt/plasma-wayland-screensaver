//! Test-only free-flight accounting and wall-time hover measurements.
use super::{
    arena::{V, v},
    body::Ball,
};
pub(super) struct FlightAudit {
    pub free_updates: u64,
    pub force_errors: u64,
    pub max_error: f64,
    pub max_still: f64,
    pub max_film_vertical_still: f64,
    pub film_end: f64,
    pub film_gravity: f64,
    pub film_id: usize,
    still: [f64; 61],
    film_still: [f64; 61],
}
impl Default for FlightAudit {
    fn default() -> Self {
        Self {
            free_updates: 0,
            force_errors: 0,
            max_error: 0.,
            max_still: 0.,
            max_film_vertical_still: 0.,
            film_end: 0.,
            film_gravity: 0.,
            film_id: 0,
            still: [0.; 61],
            film_still: [0.; 61],
        }
    }
}
impl FlightAudit {
    #[allow(clippy::too_many_arguments)]
    pub fn observe(
        &mut self,
        i: usize,
        b: &Ball,
        expected: Option<V>,
        cap: f64,
        drag: f64,
        elastic: f64,
        pace: f64,
        wall_dt: f64,
        time: f64,
        gravity_y: f64,
        ordinary: bool,
    ) {
        let free = ordinary && !b.contact && !b.sleeping;
        if free && let Some(mut velocity) = expected {
            velocity = if b.high_arc && !b.floating && b.flow_limit < cap {
                let limited = velocity.limit(cap);
                let lateral_limit = b.flow_limit.max(b.lateral_launch_budget);
                v(limited.x.clamp(-lateral_limit, lateral_limit), limited.y)
            } else {
                velocity.limit(cap)
            };
            let applied = if elastic == 1. && velocity.len2() <= 100. / (drag * drag) {
                1.
            } else {
                drag
            };
            let error = (b.v - velocity * applied).len();
            self.free_updates += 1;
            self.max_error = self.max_error.max(error);
            if error > 1e-8 {
                if self.force_errors < 3 {
                    println!(
                        "AIR_FORCE time={time} id={i} actual={:?} expected={:?} error={error}",
                        b.v,
                        velocity * applied
                    );
                }
                self.force_errors += 1;
            }
        }
        self.still[i] = if free && b.v.len() * pace < 150. {
            self.still[i] + wall_dt
        } else {
            0.
        };
        self.max_still = self.max_still.max(self.still[i]);
        self.film_still[i] =
            if free && (600. ..=660.).contains(&time) && (b.v.y * pace).abs() < 150. {
                self.film_still[i] + wall_dt
            } else {
                0.
            };
        if self.film_still[i] > self.max_film_vertical_still {
            self.max_film_vertical_still = self.film_still[i];
            self.film_end = time;
            self.film_gravity = gravity_y;
            self.film_id = i;
        }
    }
}
