//! Hyperspace: an analytic voyage through a streamed, seeded galaxy.
mod events;
mod noise;
mod optics;
mod sky;
mod space;
#[cfg(test)]
mod tests;
mod universe;
mod voyage;
use super::{Descriptor, MirroredPolicy, schema::Parameter};
use super::{
    common::{clock::Rate, rng::hash64},
    world::*,
};
use space::{Kind, V};
use std::sync::Arc;
use universe::Stream;
use voyage::{Leg, Pose};
#[allow(non_snake_case)]
#[derive(Clone, Debug)]
pub struct Params {
    pub flight: &'static str,
    pub steering: i32,
    pub nebulae: i32,
    pub events: bool,
    panoramic: bool,
}
impl Default for Params {
    fn default() -> Self {
        Self {
            flight: "voyage",
            steering: 40,
            nebulae: 45,
            events: true,
            panoramic: false,
        }
    }
}
pub const PARAMETERS: &[Parameter] = &[
    Parameter::enumeration(1, c"flight", c"voyage|hyperspace|cruise", 2., 0.),
    Parameter::integer(2, c"steering", 0., 100., 40.),
    Parameter::integer(3, c"nebulae", 0., 100., 45.),
    Parameter::boolean(4, c"events", true),
];
pub const DESCRIPTOR: Descriptor = Descriptor {
    id: 3,
    key: "starfield",
    available: true,
    rate: 30,
    mirrored: MirroredPolicy::SharedCanonical,
    fit: super::MirroredFit::Height,
    parameters: PARAMETERS,
    create: |input| Some(Box::new(Voyage::new(input))),
};

struct Art {
    input: WorldInput,
    params: Params,
    shutter: f64,
    palette: [[f32; 4]; 6],
    anchor: [f64; 2],
    min_area: f64,
    still: Pose,
    vista: space::Mark,
}
#[derive(Clone)]
struct Frame {
    art: Arc<Art>,
    clouds: Arc<sky::CloudStream>,
    moments: Arc<Vec<events::Moment>>,
    leg: Arc<Leg>,
    prev: Arc<Leg>,
    next: Option<Arc<Leg>>,
    atlas: Arc<universe::Atlas>,
    script: Arc<Vec<Event>>,
}
impl Frame {
    fn leg_at(&self, t: f64) -> &Leg {
        if t < self.leg.start {
            &self.prev
        } else if t >= self.leg.end {
            self.next.as_deref().unwrap_or(&self.leg)
        } else {
            &self.leg
        }
    }
    fn pose(&self, t: f64) -> Pose {
        if self.art.input.reduced_motion {
            self.art.still
        } else {
            self.leg_at(t).pose(t)
        }
    }
}
pub struct Voyage {
    frame: Frame,
    stream: Stream,
    calendar: events::Calendar,
    tick: u64,
}
impl Voyage {
    pub fn new(input: WorldInput) -> Self {
        let mut params = Params {
            panoramic: input.mode == MonitorMode::Spanning,
            ..Params::default()
        };
        for v in &input.parameters {
            match v.id {
                1 => params.flight = ["voyage", "hyperspace", "cruise"][v.payload as usize],
                2 => params.steering = v.payload as i32,
                3 => params.nebulae = v.payload as i32,
                4 => params.events = v.payload != 0,
                _ => {}
            }
        }
        let palette_order = [
            [3, 2, 1, 0, 4, 3],
            [5, 0, 1, 2, 3, 4],
            [4, 3, 2, 0, 1, 3],
            [3, 2, 5, 0, 1, 3],
            [3, 2, 1, 0, 5, 3],
            [4, 0, 1, 1, 0, 5],
        ];
        let palette = palette_order[input.palette as usize]
            .map(|i| crate::visuals::palette::color(input.palette, i));
        let rest = input.layout.eye_anchor();
        let min_area = input
            .layout
            .outputs
            .iter()
            .map(|o| o.rect.area())
            .fold(f64::INFINITY, f64::min);
        let (park, vista) = voyage::observatory(input.seed);
        let still = Pose {
            direction: V(0., 0., 1.),
            p: park,
            attitude: space::Attitude::new(0., 0., 0.),
            warp: 0.,
            surge: 0.,
            exposure: 0.,
            bow: 0.,
        };
        let start = if input.reduced_motion {
            park
        } else {
            V(0.43, 0.57, 0.36)
        };
        let shutter = if input.trails <= 0.35 {
            (1. + 7. * input.trails / 0.35) / 240.
        } else {
            1. / 30. + (1. / 8. - 1. / 30.) * (input.trails - 0.35) / 0.65
        };
        let leg = Arc::new(Leg::plan(
            input.seed,
            0,
            0.,
            start,
            V(0., 0., 1.),
            0.,
            [Kind::Sun, Kind::Sun],
            &params,
            input.speed,
            &[],
        ));
        let stream = Stream::new(input.seed, start, input.density);
        let art = Arc::new(Art {
            params,
            input,
            shutter,
            palette,
            anchor: [rest.x, rest.y],
            min_area,
            still,
            vista,
        });
        let mut calendar = events::Calendar::new(art.input.seed);
        let mut frame = Frame {
            moments: Arc::default(),
            art,
            clouds: Arc::default(),
            prev: leg.clone(),
            next: None,
            leg,
            atlas: stream.atlas.clone(),
            script: Arc::default(),
        };
        sky::CloudStream::update(&mut frame, 0, true);
        calendar.plan(&mut frame);
        Self {
            calendar,
            frame,
            stream,
            tick: 0,
        }
    }
}
impl LivingWorld for Voyage {
    fn rate(&self) -> Rate {
        Rate::new(30, 1).unwrap()
    }
    fn step(&mut self, tick: u64) {
        self.tick = tick;
        if self.frame.art.input.reduced_motion {
            return;
        }
        let t = tick as f64 / 30.;
        let a = self.frame.art.clone();
        let plan = |f: &Frame| {
            Arc::new(
                f.leg
                    .next(a.input.seed, &a.params, a.input.speed, &f.script),
            )
        };
        while t >= self.frame.leg.end {
            let next = self.frame.next.take().unwrap_or_else(|| plan(&self.frame));
            self.frame.prev = std::mem::replace(&mut self.frame.leg, next);
            self.calendar.plan(&mut self.frame);
        }
        // The gas stream samples a future key. Keep its next pose identical
        // whether a world was sought directly or visited every planning tick.
        if self.frame.next.is_none() && t + 3. >= self.frame.leg.end {
            self.frame.next = Some(plan(&self.frame));
        }
        let p = self.frame.leg.pose(t).p;
        sky::CloudStream::update(&mut self.frame, tick, false);
        self.stream.follow(p);
        self.frame.atlas = self.stream.atlas.clone();
    }
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            time_ns: self.rate().tick_ns(self.tick),
            tick: self.tick,
            generations: Generations {
                layout: self.frame.art.input.layout.generation,
                topology: self.frame.leg.serial + 1,
                data: self.stream.revision,
                sample: self.tick,
            },
            typed: Some(Arc::new(self.frame.clone())),
            emission_estimate: self.emission_estimate(),
            ..Snapshot::default()
        }
    }
    fn emission_estimate(&self) -> f64 {
        0.035 + 0.02 * (self.frame.art.params.nebulae as f64 * 0.01)
    }
    fn state_hash(&self) -> u64 {
        let p = self.frame.pose(self.tick as f64 / 30.).p;
        hash64(
            self.frame.art.input.seed
                ^ self.tick
                ^ p.0.to_bits()
                ^ p.1.to_bits()
                ^ p.2.to_bits()
                ^ self.frame.leg.destination.id,
        )
    }
    #[cfg(feature = "visual-capture")]
    fn inject(&mut self, e: Event) -> Result<(), &'static str> {
        if !(1..=7).contains(&e.id) || e.at_ns < 0 {
            return Err("unknown voyage override");
        }
        Arc::make_mut(&mut self.frame.script).push(e);
        let a = self.frame.art.clone();
        if e.id <= 3 && self.frame.leg.serial == 0 {
            let origin = self.frame.leg.pose(0.).p;
            self.frame.leg = Arc::new(Leg::plan(
                a.input.seed,
                0,
                0.,
                origin,
                V(0., 0., 1.),
                0.,
                [Kind::Sun, Kind::Sun],
                &a.params,
                a.input.speed,
                &self.frame.script,
            ));
            self.frame.prev = self.frame.leg.clone();
            self.frame.next = None;
            sky::CloudStream::update(&mut self.frame, self.tick, true);
        }
        if self.frame.leg.serial == 0 {
            self.calendar = events::Calendar::new(a.input.seed);
            self.calendar.plan(&mut self.frame);
        }
        Ok(())
    }
}
