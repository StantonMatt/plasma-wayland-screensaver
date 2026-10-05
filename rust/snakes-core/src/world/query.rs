// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only observations used by controllers and diagnostics.
use super::*;
impl World {
    /// Fixed diagnostic arena, never called by the simulation. Each tuple is
    /// (head, heading, length, aggression); bodies start straight at radius 6.
    /// Inactive slots cannot respawn during a benchmark. Normal mechanics,
    /// including ambient replenishment and death food, still run unmodified.
    pub fn diagnostic_arena(config: Config, snakes: &[(Point, f64, usize, f64)], food: &[Point]) -> Result<Self, &'static str> {
        config.validate().map_err(|_| "invalid configuration")?;
        if snakes.is_empty() || snakes.len()>config.snake_count() || food.len()>config.maximum_food()
            || snakes.iter().any(|(p,a,len,aggression)| !p.x.is_finite() || !p.y.is_finite() || !a.is_finite()
                || *len==0 || *len>MAX_SEGMENTS || !aggression.is_finite() || !(0.0..=1.0).contains(aggression))
            || food.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
            return Err("invalid fixture");
        }
        let mut w=Self::new(config).map_err(|_| "invalid configuration")?;
        for s in &mut w.snakes {s.alive=false;s.len=0;s.respawn=1e9;}
        for (id,&(head,angle,len,aggression)) in snakes.iter().enumerate() {
            let s=&mut w.snakes[id];
            *s=Snake {generation:1,alive:true,len,angle,desired:angle,base_radius:6.0,radius:6.0,
                birth_len:len,color:id as u32,score:len as f64,
                traits:Traits {speed_bias:1.0,aggression,..Traits::default()},..Snake::default()};
            for j in 0..len {
                let p=w.canonical_point(Point{x:head.x-angle.cos()*6.0*1.18*j as f64,y:head.y-angle.sin()*6.0*1.18*j as f64});
                w.segments[id*MAX_SEGMENTS+j]=Segment {current:p,previous:p};
            }
            w.rebuild_trail(id);
        }
        w.food.clear();
        for (i,&p) in food.iter().enumerate() {
            w.food.push(Food {id:i as u64+1,p,value:1.0,life:1000.0,owner:-1,..Food::default()});
        }
        w.next_food=food.len() as u64+1;
        w.growth_slots=config.maximum_world_segments().saturating_sub(w.stats().total_segments as usize);
        Ok(w)
    }
    /// Sample the physical head trail by arc distance, without the safety
    /// grid's conservative release deadline. Binary search uses retained storage.
    pub(crate) fn forecast_trail_point(&self,id:usize,behind:f64)->Point {
        let s=self.snakes[id];let newest=self.trail_point(id,s.trail_len-1);
        let target=newest.distance-behind;
        let mut lo=0;let mut hi=s.trail_len-1;
        while hi-lo>1 {
            let mid=(lo+hi)/2;
            if self.trail_point(id,mid).distance>target {hi=mid;} else {lo=mid;}
        }
        let a=self.trail_point(id,lo);let b=self.trail_point(id,hi);
        let t=((target-a.distance)/(b.distance-a.distance).max(0.0001)).clamp(0.0,1.0);
        self.canonical_point(Point{x:a.p.x+(b.p.x-a.p.x)*t,y:a.p.y+(b.p.y-a.p.y)*t})
    }
    /// A fractional nutrition balance reserves tail room but cannot actually
    /// stretch the physical trail until it can pay for a segment.
    pub(crate) fn forecast_growth_active(&self,id:usize)->bool {
        let s=&self.snakes[id];
        self.growth_allowed(s) && s.growth>=Self::growth_cost(s)
    }
    /// Independent diagnostic snapshot. Allocates only when explicitly called;
    /// simulation and controller ticks never invoke it.
    pub fn diagnostic_snapshot(&self) -> Self {
        let mut copy = Self::new(self.config).expect("validated configuration");
        copy.snakes.clone_from(&self.snakes);
        copy.faces=self.faces;copy.bubbles=self.bubbles;copy.bubble_count=self.bubble_count;copy.world_event=self.world_event;
        copy.segments.clone_from(&self.segments);
        copy.food.clone_from(&self.food);
        copy.items.clone_from(&self.items);
        copy.item_timer = self.item_timer;
        copy.prism_timer = self.prism_timer;
        copy.detached=self.detached;copy.detached_points.clone_from(&self.detached_points);
        copy.next_item = self.next_item;
        copy.last_item_kind = self.last_item_kind;
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
        copy.collisions = self.collisions;
        copy.consumptions.clone_from(&self.consumptions);
        copy.generations = self.generations;
        copy.leader = self.leader;
        copy.frame_events = self.frame_events;
        copy.event_start = self.event_start;
        copy.event_count = self.event_count;
        #[cfg(feature = "parity")]
        {
            copy.events.clone_from(&self.events);
            copy.parity_record = self.parity_record;
            copy.parity_killers.clone_from(&self.parity_killers);
        }
        copy
    }

    /// Replace one diagnostic fixture's body with a measured curved trail.
    /// Not a runtime control: call before constructing the fixture controller.
    pub fn diagnostic_body(&mut self,id:usize,points:&[Point],angle:f64)->Result<(), &'static str> {
        if id>=self.snakes.len() || points.is_empty() || points.len()>MAX_SEGMENTS || !angle.is_finite()
            || points.iter().any(|p|!p.x.is_finite() || !p.y.is_finite()) {return Err("invalid body");}
        let s=&mut self.snakes[id];
        s.alive=true;s.len=points.len();s.birth_len=points.len();s.score=points.len() as f64;
        s.angle=angle;s.desired=angle;s.growth=0.0;s.stretch=0.0;
        for (j,&p) in points.iter().enumerate() {
            let p=self.canonical_point(p);self.segments[id*MAX_SEGMENTS+j]=Segment {current:p,previous:p};
        }
        self.rebuild_trail(id);
        self.growth_slots=self.config.maximum_world_segments().saturating_sub(self.stats().total_segments as usize);
        Ok(())
    }

    /// Last applied rush, for prediction of a rival's observed motion.
    pub fn observed_rush(&self,id:usize)->Option<f64> {self.snakes.get(id).map(|s|s.rush)}
    /// Segments still payable in an observed burst, or the price of a new one.
    pub fn boost_segment_cost(&self,id:usize)->Option<usize> {
        self.snakes.get(id).map(|s|if s.boost_ticks>0 {
            s.boost_cost.saturating_sub(s.boost_paid) as usize
        } else if effects::modifiers(s.effect_kind,s.effect_ticks).free_boost {0} else {2+s.len/100})
    }
    /// Additional physical inputs used to invalidate the controller's cached
    /// single-burst schedules. These counters have no presentation payload.
    pub(crate) fn motion_cache_counters(&self,id:usize)->(u8,u8,bool) {
        let s=&self.snakes[id];(s.boost_cost,s.boost_paid,self.growth_slots>0)
    }
    /// Exact mechanics speed/turn limits for a proposed rush. Recomputes the
    /// growth-block flag just as move_snake does, without changing the world.
    pub fn motion_limits(&self, id: usize, rush: f64) -> Option<(f64, f64)> {
        let mut s = *self.snakes.get(id)?;
        s.rush = if self.config.rules == RuleSet::Classic { rush.clamp(0.0, 1.0) }
            else if s.boost_ticks > 0 || (rush > 0.0 && self.boost_ready(id)) { 0.6 } else { 0.0 };
        s.blocked = !self.growth_allowed(&s);
        Some((self.speed(&s), self.turn_rate(&s)))
    }
    /// Motion on a future tick of a single requested burst, holding the
    /// observed nutrition reserve fixed. Includes scheduled tail payments and
    /// burst expiry; does not invent future food, controls or new boosts.
    /// `offset=0` is the next executed tick (active counters decrement first).
    pub fn forecast_motion_limits(&self, id: usize, rush: f64, offset: usize) -> Option<(f64, f64)> {
        if self.config.rules == RuleSet::Classic { return self.motion_limits(id,rush); }
        self.snakes.get(id)?;
        Some(effects::forecast_motion_before_tick(self,id,rush,offset))
    }
    /// The same single-burst forecast through expiry. Recompute expensive
    /// turning-radius limits only when payment or expiry changes motion.
    pub fn forecast_motion_schedule(&self,id:usize,rush:f64)->Option<[(f64,f64);25]> {
        self.snakes.get(id)?;
        if self.config.rules==RuleSet::Classic {return Some([self.motion_limits(id,rush)?;25]);}
        Some(effects::forecast_schedule_before_tick(self,id,rush))
    }

    /// Conservative time to spend the current growth reserve. Growth consumes
    /// nutrition only after stretch reaches one segment, not once per tick.
    /// Accounts for the exact cost/stretch rule; 65% speed allows for slowing
    /// and radius changes during these at most twelve growth increments.
    pub fn tail_growth_delay(&self, id: usize) -> Option<f64> {
        let s=self.snakes.get(id)?;
        if !self.growth_allowed(s) {return Some(0.0);}
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
    /// Latest tick's exact deaths, in slot order. No allocation and no effect
    /// on simulation, RNG, collision precedence or stable C ABI.
    pub fn collision_events(&self) -> impl Iterator<Item = &CollisionEvent> {
        self.collisions.iter().filter(|e|e.reason != DeathReason::None)
    }
    /// Exact food consumptions in the latest step, in mechanics order:
    /// (particle ID, eater slot, eater generation, particle position, value).
    /// Expiry and death-food eviction never produce records. Storage is bounded
    /// by MAX_FOOD and reused; observing it does not affect mechanics or RNG.
    pub fn consumption_events(&self) -> impl Iterator<Item = (u64, u32, u32, Point, f64)> + '_ {
        self.consumptions.iter().copied()
    }
    /// Latest head sweep, retained even when death clears the visible length.
    /// Read immediately after step, before a later respawn replaces the slot.
    pub fn collision_head(&self,id:usize)->Option<Segment> {
        self.snakes.get(id).map(|_|self.segments[id*MAX_SEGMENTS])
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
