// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only observations used by controllers and diagnostics.
use super::*;
impl World {
    /// Independent diagnostic snapshot. Allocates only when explicitly called;
    /// simulation and controller ticks never invoke it.
    pub fn diagnostic_snapshot(&self) -> Self {
        let mut copy = Self::new(self.config).expect("validated configuration");
        copy.snakes.clone_from(&self.snakes);
        copy.segments.clone_from(&self.segments);
        copy.food.clone_from(&self.food);
        copy.trails.clone_from(&self.trails);
        copy.trail_capacity = self.trail_capacity;
        copy.grid_heads.clone_from(&self.grid_heads);
        copy.grid_next.clone_from(&self.grid_next);
        copy.grid_columns = self.grid_columns;
        copy.grid_rows = self.grid_rows;
        copy.rng = self.rng;
        copy.next_food = self.next_food;
        copy.next_feast = self.next_feast;
        copy.tick = self.tick;
        copy.time = self.time;
        copy.geometry_generation = self.geometry_generation;
        copy.growth_slots = self.growth_slots;
        copy.deaths = self.deaths;
        copy.generations = self.generations;
        #[cfg(feature = "parity")]
        {
            copy.events.clone_from(&self.events);
            copy.parity_record = self.parity_record;
            copy.parity_killers.clone_from(&self.parity_killers);
        }
        copy
    }

    /// Exact mechanics speed/turn limits for a proposed rush. Recomputes the
    /// growth-block flag just as move_snake does, without changing the world.
    pub fn motion_limits(&self, id: usize, rush: f64) -> Option<(f64, f64)> {
        let mut s = *self.snakes.get(id)?;
        s.rush = rush.clamp(0.0, 1.0);
        s.blocked = s.len >= self.maximum_snake_segments(&s) || self.growth_slots == 0;
        Some((self.speed(&s), self.turn_rate(&s)))
    }
    /// Conservative time to spend the current growth reserve. Growth consumes
    /// nutrition only after stretch reaches one segment, not once per tick.
    /// Accounts for the exact cost/stretch rule; 65% speed allows for slowing
    /// and radius changes during these at most twelve growth increments.
    pub fn tail_growth_delay(&self, id: usize) -> Option<f64> {
        let s=self.snakes.get(id)?;
        if s.len>=self.maximum_snake_segments(s) || self.growth_slots==0 {return Some(0.0);}
        let speed=self.motion_limits(id,0.0)?.0;
        let increments=(s.growth/Self::growth_cost(s)).ceil();
        Some((increments-s.stretch).max(0.0)*s.radius*1.18/(speed*0.65*0.62).max(1.0))
    }
    /// Score: birth length plus nutrition consumed (not the growth balance).
    pub fn nutrition(&self, id: usize) -> Option<f64> {
        self.snakes.get(id).map(|s| s.score)
    }
    /// Collision result of the latest tick; inspect newly dead slots before the next step.
    pub fn last_death_reason(&self, id: usize) -> Option<DeathReason> {
        self.snakes.get(id).map(|s| s.dying)
    }
    pub fn displacement(&self, from: Point, to: Point) -> Point {
        self.config.geometry().delta(from, to)
    }
    pub fn canonical_point(&self, p: Point) -> Point {
        if self.config.deadly_walls || (p.x>=0.0 && p.x<self.config.width && p.y>=0.0 && p.y<self.config.height) {
            p
        } else { self.config.geometry().wrap(p) }
    }
}
