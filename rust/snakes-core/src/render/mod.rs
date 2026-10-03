// SPDX-License-Identifier: GPL-3.0-or-later
//! Per-window, allocation-free classic triangle tessellation. All arithmetic
//! preceding the final vertex conversion stays f64, as in the Qt renderer.
mod geometry;
mod shader;
pub(crate) mod items;
pub use shader::ShaderVertex;
use geometry::{P, Sink, delta, wrap, copies, visible, prepare};
use crate::ffi::{SnakeRecord, SegmentRecord, FoodRecord, ItemRecord, EventRecord, FrameInfo};
use crate::{flags, MAX_SNAKES, MAX_SEGMENTS};
// Far beyond supported worlds (<=16384) and physical radii, but small
// enough that all projected/effect coordinates stay representable as f32.
// Use comparisons: they reject NaN and infinities without a separate scan.
pub(crate) const NUMERIC_LIMIT: f64 = 1.0e9;
#[inline]
pub(crate) fn coordinate(x: f64) -> bool { x.abs()<=NUMERIC_LIMIT }
#[inline]
fn coordinate32(x: f32) -> bool { x.abs()<=NUMERIC_LIMIT as f32 }
#[inline]
fn nonnegative(x: f64) -> bool { (0.0..=NUMERIC_LIMIT).contains(&x) }
#[inline]
fn segment_valid(s: &SegmentRecord) -> bool {
    // Straight-line f32 checks let LLVM validate all four borrowed fields
    // together; do not convert/branch separately for every coordinate.
    coordinate32(s.x) & coordinate32(s.y)
        & coordinate32(s.previous_x) & coordinate32(s.previous_y)
}
#[inline]
pub(crate) fn snake_valid(s: &SnakeRecord) -> bool {
    nonnegative(s.radius) && coordinate(s.angle) && coordinate(s.desired_angle)
}
#[inline]
fn food_valid(f: &FoodRecord) -> bool {
    coordinate32(f.x) & coordinate32(f.y) & (0.0..=NUMERIC_LIMIT as f32).contains(&f.size)
        & coordinate32(f.phase) & (0.0..=1.0).contains(&f.attraction)
        & coordinate32(f.attraction_x) & coordinate32(f.attraction_y)
}
#[inline]
pub(crate) fn frame_valid(info: &FrameInfo, p: &Params) -> bool {
    (1.0e-6..=NUMERIC_LIMIT).contains(&info.world_width)
        && (1.0e-6..=NUMERIC_LIMIT).contains(&info.world_height)
        && coordinate(info.simulation_time)
        && nonnegative(p.viewport_width) && nonnegative(p.viewport_height)
        && [p.scale_x,p.scale_y].iter().all(|x|*x==0.0 || (1.0e-6..=1.0e6).contains(x))
        && coordinate(p.offset_x) && coordinate(p.offset_y) && coordinate(p.presentation_time)
        && (0.0..=1.0).contains(&p.interpolation) && p.deadly_walls<=1 && p.developer_mode<=1
}
// Frozen snapshots can retain the last moving tick's previous coordinates.
// Use the same policy for the body and every head-attached primitive/overlay.
fn moving(s: &SnakeRecord) -> bool {
    s.alive != 0 && s.flags & (flags::CORPSE | flags::FROZEN) == 0
}
#[inline(always)]
fn position(seg: &SegmentRecord, interpolate: bool, info: &FrameInfo, p: &Params) -> P {
    if !segment_valid(seg) { return P::new(f64::NAN, f64::NAN); }
    if !interpolate {
        return P::new(seg.x as f64, seg.y as f64);
    }
    let walls = p.deadly_walls != 0;
    P::new(seg.previous_x as f64 + delta(seg.previous_x as f64, seg.x as f64, info.world_width, walls)*p.interpolation,
        seg.previous_y as f64 + delta(seg.previous_y as f64, seg.y as f64, info.world_height, walls)*p.interpolation)
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8
}
impl Color {
    fn new(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha
        }
    }
    fn alpha(self, alpha: u8) -> Self {
        Self {
            alpha,
            ..self
        }
    }
    fn fade(self, f: f64) -> Self {
        if f == 1.0 { return self; }
        self.alpha((self.alpha as f64*f).round() as u8)
    }
    fn boost(self) -> Self {
        let mix = |x: u8| (x as f64+(255-x) as f64*0.35).round() as u8;
        Self::new(mix(self.red), mix(self.green), mix(self.blue), self.alpha)
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vertex {
    pub x: f32,
    pub y: f32,
    pub color: Color
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Params {
    pub viewport_width: f64,
    pub viewport_height: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub offset_x: f64,
    pub offset_y: f64,
    pub interpolation: f64,
    pub presentation_time: f64,
    pub deadly_walls: u32,
    pub developer_mode: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Output {
    pub vertex_count: usize,
    pub dense_food: u32,
    pub reserved: u32
}
#[derive(Clone, Copy, Default)]
struct Trail {
    generation: u32,
    points: [P;15],
    ticks: [u64;15],
    head: usize,
    len: usize
}
// One slot is one visible logical effect copy, regardless of its triangles.
// Count required vertices too, so a buffer-growth retry sees the same budget.
#[derive(Default)]
struct EffectBudget { copies: usize }
impl EffectBudget {
    fn full(&self)->bool { self.copies>=8 }
    fn emitted(&mut self,before:usize,after:usize) { self.copies+=usize::from(after>before); }
}
#[derive(Clone, Copy, Default)]
struct Corpse {
    generation: u32,
    time: f64,
    active: bool
}
pub struct Renderer {
    points: Vec<P>,
    mapped: Vec<P>,
    normals: Vec<P>,
    valid: Vec<bool>,
    circles: [[P;13];13],
    dense: bool,
    trails: [Trail;MAX_SNAKES],
    corpses: [Corpse;MAX_SNAKES],
    last_frame: Option<(u64, u64)>,
    event_tick: Option<u64>,
    tapers: Vec<f64>,
    brightness: Vec<u8>,
    // Only mixed-colour waves need per-sample origins; retained, never allocated per frame.
    wave_origins: Vec<u8>,
    taper_lengths: [usize;MAX_SNAKES],
    pub reduced_motion: bool,
    items: [ItemRecord;crate::MAX_ITEMS],
    item_count: usize,
    item_radius: f64,
    effects: [shader::Effect;8],
    effect_head: usize,
    waves: [[shader::Wave;2];MAX_SNAKES],
    shader_generations: [u32;MAX_SNAKES],
    shader_flags: [u32;MAX_SNAKES],
    shader_limits: Vec<f64>,
    shader_taper_bytes: Vec<u8>,
    shader_taper_lengths: [usize;MAX_SNAKES],
    shader_angles: [f64;MAX_SNAKES],
    shader_previous_angles: [f64;MAX_SNAKES],
}
impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}
impl Renderer {
    pub fn new() -> Self {
        let mut circles = [[P::default();13];13];
        for sides in [4, 5, 6, 7, 8, 12] {
            for side in 0..=sides {
                let a = std::f64::consts::TAU*side as f64/sides as f64;
                circles[sides][side] = P::new(a.cos(), a.sin());
            }
        }
        Self {
            points: vec![P::default();MAX_SEGMENTS],
            mapped: vec![P::default();MAX_SEGMENTS],
            normals: vec![P::default();MAX_SEGMENTS],
            valid: vec![false;MAX_SEGMENTS],
            circles,
            dense: false,
            trails: [Trail::default();MAX_SNAKES],
            corpses: [Corpse::default();MAX_SNAKES],
            last_frame: None,
            event_tick: None,
            tapers: vec![0.0;MAX_SNAKES*MAX_SEGMENTS],
            brightness: vec![0;MAX_SEGMENTS],
            wave_origins: vec![0;MAX_SEGMENTS],
            taper_lengths: [0;MAX_SNAKES],
            reduced_motion: false,
            items: [ItemRecord::default();crate::MAX_ITEMS],
            item_count: 0,
            item_radius: 0.0,
            effects: [shader::Effect::default();8],
            effect_head: 0,
            waves: [[shader::Wave::default();2];MAX_SNAKES],
            shader_generations: [0;MAX_SNAKES],
            shader_flags: [0;MAX_SNAKES],
            shader_limits: vec![0.0;MAX_SEGMENTS],
            shader_taper_bytes: vec![0;MAX_SNAKES*MAX_SEGMENTS],
            shader_taper_lengths: [0;MAX_SNAKES],
            shader_angles: [0.0;MAX_SNAKES],
            shader_previous_angles: [0.0;MAX_SNAKES]
        }
    }
    pub fn reset(&mut self) {
        self.trails.fill(Trail::default());
        self.corpses.fill(Corpse::default());
        self.last_frame = None;
        self.event_tick = None;
        self.dense = false;
        self.effects.fill(shader::Effect::default());
        self.effect_head=0;
        self.waves.fill([shader::Wave::default();2]);
        self.shader_generations.fill(0);
        self.shader_flags.fill(0);
    }
    fn history(&mut self, info: &FrameInfo, snakes: &[SnakeRecord], segments: &[SegmentRecord], events: &[EventRecord]) {
        if self.last_frame.is_some_and(|(tick, generation)|info.tick<tick || generation!=info.geometry_generation) {
            self.reset();
        }
        if self.last_frame==Some((info.tick, info.geometry_generation)) {
            return;
        }
        // Advance the deduplication cutoff after shader_history consumes events.
        if let Some(t) = events.iter().map(|e|e.tick).max() {
            self.event_tick = Some(self.event_tick.map_or(t, |old|old.max(t)));
        }
        for s in snakes {
            if !snake_valid(s) { continue; }
            let trail = &mut self.trails[s.id as usize];
            if trail.generation != s.generation || (trail.len != 0 && (!moving(s) || s.flags & flags::BOOSTING == 0)) {
                *trail = Trail {
                    generation: s.generation, ..Trail::default()
                };
            }
            if moving(s) && s.flags&flags::BOOSTING!=0 && s.segment_count>0 {
                let tail = segments[(s.segment_offset+s.segment_count-1) as usize];
                trail.points[trail.head] = if segment_valid(&tail) { P::new(tail.x as f64, tail.y as f64) }
                    else { P::new(f64::NAN, f64::NAN) };
                trail.ticks[trail.head] = info.tick;
                trail.head = (trail.head+1)%15;
                trail.len = (trail.len+1).min(15);
            }
            let corpse = &mut self.corpses[s.id as usize];
            if s.flags&flags::CORPSE!=0 {
                if !corpse.active || corpse.generation!=s.generation {
                    *corpse = Corpse {
                        generation: s.generation,
                        time: info.simulation_time,
                        active: true
                    };
                }
            } else {
                corpse.active = false;
            }
        }
        self.last_frame = Some((info.tick, info.geometry_generation));
    }
    pub fn build(&mut self, info: &FrameInfo, snakes: &[SnakeRecord], segments: &[SegmentRecord], food: &[FoodRecord], events: &[EventRecord], palette: &[Color], p: &Params, output: &mut [Vertex]) -> Output {
        if !frame_valid(info,p) { return Output::default(); }
        self.shader_history(info, snakes, segments, events);
        self.dense = if self.dense {
            food.len()>=280
        } else {
            food.len()>340
        };
        let view = P::new(p.viewport_width, p.viewport_height);
        let walls = p.deadly_walls!=0;
        let sx = p.scale_x;
        let sy = p.scale_y;
        // An empty projection has no visible geometry. Avoid converting pixel
        // minimums back to world bounds by dividing by a zero scale.
        if sx==0.0 || sy==0.0 {
            return Output { dense_food: self.dense as u32, ..Output::default() };
        }
        let scale = (sx*sy).sqrt();
        let inverse_x = 1.0/sx;
        let inverse_y = 1.0/sy;
        let arena = P::new(info.world_width, info.world_height);
        let off = P::new(p.offset_x, p.offset_y);
        let map = |v: P|P::new(v.x*sx+off.x, v.y*sy+off.y);
        let color = |i: u32|if palette.is_empty() {
            Color::new(0, 255, 255, 255)
        } else {
            palette[i as usize%palette.len()]
        };
        let mut sink = Sink {
            output,
            count: 0,
            view,
            circles: &self.circles
        };
        for f in food {
            if !food_valid(f) { continue; }
            let pulse = 0.82+(p.presentation_time*3.0+f.phase as f64).sin()*0.18;
            let pellet = f.kind==2;
            let world_size = f.size as f64*pulse*if pellet {
                0.55
            } else {
                1.0
            };
            let size = world_size*scale;
            let c = color(f.color_index);
            let pos = P::new(f.x as f64, f.y as f64);
            // Include minimum-sized highlights and the entire vacuum streak,
            // whose endpoint may cross a seam even when the halo does not.
            let mut min = pos;
            let mut max = pos;
            if !walls {
                let halo = size*3.2;
                let highlight = if pellet { 0.0 } else { size*0.24+(size*0.3).max(0.7) };
                let extent = halo.max(highlight);
                let margin = P::new(extent*inverse_x, extent*inverse_y);
                min = pos-margin;
                max = pos+margin;
            }
            let mut streak = P::default();
            let streak_width = (size*0.38).max(0.5);
            if f.attraction>0.0 {
                let pull = P::new(delta(pos.x, f.attraction_x as f64, info.world_width, walls)*sx, delta(pos.y, f.attraction_y as f64, info.world_height, walls)*sy);
                let len = pull.length();
                if len>0.001 {
                    streak = pull/len*(size*(2.0+f.attraction as f64*5.0));
                    if !walls {
                        let end = pos-P::new(streak.x*inverse_x, streak.y*inverse_y);
                        let margin = P::new(streak_width*inverse_x, streak_width*inverse_y);
                        min = P::new(min.x.min(end.x-margin.x), min.y.min(end.y-margin.y));
                        max = P::new(max.x.max(end.x+margin.x), max.y.max(end.y+margin.y));
                    }
                }
            }
            let (xs, ys) = copies(min, max, P::default(), arena, walls);
            for xi in xs.first..=xs.last {
                for yi in ys.first..=ys.last {
                    let center = map(pos+P::new(xi as f64*xs.extent, yi as f64*ys.extent));
                    if streak.x!=0.0 || streak.y!=0.0 {
                        sink.segment(center, center-streak, streak_width, c.alpha(if pellet { 70 } else { 150 }));
                    }
                    let sides = if pellet {
                        4
                    } else if self.dense {
                        6
                    } else {
                        8
                    };
                    sink.disc(center, size*3.2, c.alpha(if pellet {
                        12
                    } else {
                        30
                    }), sides);
                    sink.disc(center, size, c.alpha(if pellet {
                        110
                    } else {
                        230
                    }), sides);
                    if !pellet {
                        sink.disc(center-P::new(size*0.24, size*0.24), (size*0.3).max(0.7), Color::new(255, 255, 255, 215), if self.dense {
                            4
                        } else {
                            5
                        });
                    }
                }
            }
        }
        for s in snakes {
            let corpse = s.flags&flags::CORPSE!=0;
            let n = s.segment_count as usize;
            if !snake_valid(s) || (s.alive==0 && !corpse)||n<2 {
                continue;
            }
            let pickup_waves=self.waves[s.id as usize].map(|w| {
                let center=(p.presentation_time-w.time)*1.3*(n-1) as f64;
                if w.active && w.kind!=0 && (0.0..(n as f64+3.5)).contains(&center) {
                    (center,w.kind)
                } else { (0.0,0) }
            });
            let has_pickup_wave=pickup_waves.iter().any(|&(_,kind)|kind!=0);
            let body = &segments[s.segment_offset as usize..s.segment_offset as usize+n];
            let points = &mut self.points[..n];
            let interpolate = moving(s);
            // Keep the stationary decision outside the segment loop. Separating
            // interpolation from sequential unwrap also lets moving segments
            // interpolate together without a per-segment state branch.
            if interpolate {
                for (point, seg) in points.iter_mut().zip(body) {
                    *point = position(seg, true, info, p);
                }
            } else {
                for (point, seg) in points.iter_mut().zip(body) {
                    *point = position(seg, false, info, p);
                }
            }
            if !walls {
                for i in 0..n {
                    let mut v = P::new(wrap(points[i].x, info.world_width), wrap(points[i].y, info.world_height));
                    if i>0 {
                        let prev = points[i-1];
                        v = P::new(prev.x+delta(wrap(prev.x, info.world_width), v.x, info.world_width, false), prev.y+delta(wrap(prev.y, info.world_height), v.y, info.world_height, false));
                    }
                    points[i] = v;
                }
            }
            let(mut minx, mut maxx, mut miny, mut maxy) = (points[0].x, points[0].x, points[0].y, points[0].y);
            // C++ std::min/max keep their first operand on NaN.
            for v in points.iter() {
                if v.x<minx {
                    minx = v.x;
                }
                if v.x>maxx {
                    maxx = v.x;
                }
                if v.y<miny {
                    miny = v.y;
                }
                if v.y>maxy {
                    maxy = v.y;
                }
            }
            if ![minx, maxx, miny, maxy].iter().all(|x|x.is_finite()) {
                continue;
            }
            let radius = s.radius*scale;
            let eye_r = (radius*0.31).max(1.7);
            // Pixel minimums on eyes/crown strokes must survive small scales.
            let mut margin = radius*1.4;
            // Above 2.4px the existing margin encloses every pixel minimum:
            // eyes: .665r+1.7 <= 1.4r; crown stroke: 1.105r+.65 <= 1.4r;
            // gem: .36r+.8 <= 1.4r. Avoid extra bounds work at normal sizes.
            if radius<2.4 {
                margin = margin.max(radius*0.665+eye_r);
                if s.flags&flags::LEADER!=0 {
                    margin = margin.max(radius*1.105+(radius*0.09).max(0.65));
                    margin = margin.max(radius*0.36+(radius*0.12).max(0.8));
                }
            }
            let world_margin = if walls { P::default() } else {
                P::new((s.radius*3.0).max(margin*inverse_x), (s.radius*3.0).max(margin*inverse_y))
            };
            let (xs, ys) = copies(P::new(minx, miny), P::new(maxx, maxy), world_margin, arena, walls);
            let active_kind=if s.alive!=0 && !corpse && s.effect_ticks>0 {s.effect_kind} else {0};
            let effect_time=if self.reduced_motion {0.0} else {p.presentation_time};
            let fade = if corpse {
                (1.0-(p.presentation_time-self.corpses[s.id as usize].time).max(0.0)/0.55).clamp(0.0, 1.0)
            } else if active_kind==3 {
                0.45+0.06*(effect_time*9.0).sin()
            } else {
                1.0
            };
            let c = if s.flags&flags::BOOSTING!=0 {
                color(s.color_index).boost()
            } else {
                color(s.color_index)
            };
            // The ring holds at most 14 edges; each gets every seam copy.
            if interpolate && s.flags & flags::BOOSTING != 0 {
                let trail = &self.trails[s.id as usize];
                let mut previous: Option<P>=None;
                for i in 0..trail.len {
                    let index = (trail.head+15-trail.len+i)%15;
                    let age = p.presentation_time - info.simulation_time
                        + info.tick.saturating_sub(trail.ticks[index]) as f64 * crate::STEP_SECONDS;
                    if age >= 0.5 { continue; }
                    let pos = trail.points[index];
                    if let Some(prev) = previous {
                        let d = P::new(delta(prev.x, pos.x, info.world_width, walls), delta(prev.y, pos.y, info.world_height, walls));
                        let a = map(prev);
                        let b = map(prev+d);
                        let width = (radius*0.96*2.4).max(0.5);
                        let end = prev+d;
                        let (tx, ty) = copies(P::new(prev.x.min(end.x), prev.y.min(end.y)),
                            P::new(prev.x.max(end.x), prev.y.max(end.y)),
                            P::new(width*inverse_x, width*inverse_y), arena, walls);
                        for x in tx.first..=tx.last {
                            for y in ty.first..=ty.last {
                                let shift = P::new(x as f64*tx.extent*sx, y as f64*ty.extent*sy);
                                sink.segment(a+shift, b+shift, width, c.alpha((110.0 * (1.0 - age.max(0.0) / 0.5)).round() as u8));
                            }
                        }
                    }
                    previous = Some(pos);
                }
            }
            // A shortest wrapped edge spans at most half an arena per axis.
            // Tile long bodies one edge at a time, retaining neighboring points
            // for smooth normals. This bounds work by (n-1) primitive budgets,
            // rather than body length times the whole-body copy product.
            // Ordinary scenes retain their original mapping and draw order.
            let split = !walls && (maxx-minx>arena.x || maxy-miny>arena.y);
            // Split sections share the same outline/fill/decoration layers;
            // later outlines must not paint over earlier section fills.
            for layer in 0..if split { 3 } else { 1 } {
                for section in 0..if split { n-1 } else { 1 } {
                    let (start,end) = if split { (section,section+1) } else { (0,n-1) };
                    let (minx,maxx,miny,maxy,xs,ys) = if split {
                        let a=points[start];let b=points[end];
                        let min=P::new(a.x.min(b.x),a.y.min(b.y));
                        let max=P::new(a.x.max(b.x),a.y.max(b.y));
                        let (xs,ys)=copies(min,max,world_margin,arena,false);
                        (min.x,max.x,min.y,max.y,xs,ys)
                    } else { (minx,maxx,miny,maxy,xs,ys) };
                    for xi in xs.first..=xs.last {
                        for yi in ys.first..=ys.last {
                            let shift = P::new(xi as f64*xs.extent, yi as f64*ys.extent);
                            if !visible((minx+shift.x)*sx+off.x-margin, (miny+shift.y)*sy+off.y-margin, (maxx-minx)*sx+margin*2.0, (maxy-miny)*sy+margin*2.0, view) {
                                continue;
                            }
                            let mapped = &mut self.mapped[..n];
                            let first = start.saturating_sub(1);
                            let last = (end+2).min(n);
                            for (to, v) in mapped[first..last].iter_mut().zip(&points[first..last]) {
                                *to = map(*v+shift);
                            }
                            prepare(&mapped[first..last], &mut self.normals[first..last], &mut self.valid[first..last]);
                            if !split || layer==0 {
                                let outline = if active_kind==1 {
                                    items::accent(1,palette).alpha(215)
                                } else if active_kind==3 {
                                    items::accent(3,palette).alpha(150)
                                } else {Color::new(5, 7, 16, 175)}.fade(fade);
                                sink.ribbon(&mapped[start..=end], &self.normals[start..=end], &self.valid[start..=end], radius*1.275, outline);
                                if end==n-1 { sink.disc(mapped[end], radius*1.275, outline, 12); }
                            }
                            if !split || layer==1 {
                                if has_pickup_wave {
                                    for edge in start..end {
                                        let mut weight=0.0_f64;let mut kind=0;
                                        for &(center,origin) in &pickup_waves {
                                            if origin==0 {continue;}
                                            let strength=(1.0-((edge as f64-center)/3.5).abs()).max(0.0)*0.95;
                                            if strength>weight {weight=strength;kind=origin;}
                                        }
                                        let accent=items::accent(kind,palette);
                                        let mix=|a:u8,b:u8|(a as f64+(b as f64-a as f64)*weight).round() as u8;
                                        let tint=Color::new(mix(c.red,accent.red),mix(c.green,accent.green),mix(c.blue,accent.blue),245).fade(fade);
                                        sink.ribbon(&mapped[edge..=edge+1],&self.normals[edge..=edge+1],&self.valid[edge..=edge+1],radius*0.96,tint);
                                    }
                                } else {
                                    sink.ribbon(&mapped[start..=end], &self.normals[start..=end], &self.valid[start..=end], radius*0.96, c.alpha(245).fade(fade));
                                }
                                if end==n-1 { sink.disc(mapped[end], radius*0.96, c.alpha(245).fade(fade), 12); }
                            }
                            if split && layer!=2 { continue; }
                            let first_mark=((start+1)/6)*6+5;
                            for i in (first_mark..=end).step_by(6) {
                                sink.disc(mapped[i], radius*0.34, Color::new(255, 255, 255, 46).fade(fade), 6);
                            }
                            if start!=0 { continue; }
                            let head = mapped[0];
                            sink.disc(head, radius*1.08, c.alpha(255).fade(fade), 12);
                            let mut forward = P::new(s.angle.cos()*sx, s.angle.sin()*sy);
                            let len = forward.length();
                            if len>0.001 {
                                forward = forward/len;
                            }
                            let side = P::new(-forward.y, forward.x);
                            for direction in [-1.0, 1.0] {
                                let eye = head+forward*(radius*0.48)+side*(radius*0.46*direction);
                                let white = if s.flags&flags::HUNTING!=0 {
                                    Color::new(255, 190, 80, 255)
                                } else {
                                    Color::new(255, 255, 255, 255)
                                };
                                let pupil = if s.flags&flags::TRAPPED!=0 {
                                    0.12
                                } else if s.flags&flags::HUNTING!=0 {
                                    0.30
                                } else {
                                    0.48
                                };
                                sink.disc(eye, eye_r, white.fade(fade), 8);
                                sink.disc(eye+forward*(eye_r*0.34), eye_r*pupil, Color::new(17, 19, 26, 255).fade(fade), 7);
                            }
                            if s.flags&flags::LEADER!=0 {
                                let center = head-forward*(radius*0.32);
                                let crown = [center-side*(radius*0.82)-forward*(radius*0.42), center-side*(radius*0.82)+forward*(radius*0.58), center-side*(radius*0.34)+forward*(radius*0.18), center+forward*(radius*0.98), center+side*(radius*0.34)+forward*(radius*0.18), center+side*(radius*0.82)+forward*(radius*0.58), center+side*(radius*0.82)-forward*(radius*0.42)];
                                for i in 0..7 {
                                    sink.triangle(center, crown[i], crown[(i+1)%7], Color::new(255, 216, 74, 255).fade(fade));
                                }
                                for i in 0..7 {
                                    sink.segment(crown[i], crown[(i+1)%7], (radius*0.09).max(0.65), Color::new(109, 67, 0, 255).fade(fade));
                                }
                                sink.disc(center+forward*(radius*0.04), (radius*0.12).max(0.8), Color::new(255, 242, 160, 255).fade(fade), 6);
                            }
                        }
                    }
                }
            }
        }
        self.classic_items(info,p,palette,&mut sink);
        let mut effect_budget=EffectBudget::default();
        self.classic_effects(info,p,palette,&mut sink,&mut effect_budget);
        self.classic_warnings(info,p,palette,snakes,segments,&mut sink,&mut effect_budget);
        if p.developer_mode!=0 {
            for s in snakes {
                if !snake_valid(s) || s.alive==0 || s.flags&flags::CORPSE!=0 || s.segment_count==0 {
                    continue;
                }
                let seg = segments[s.segment_offset as usize];
                let mut start = position(&seg, moving(s), info, p);
                if !walls {
                    start = P::new(wrap(start.x, info.world_width), wrap(start.y, info.world_height));
                }
                let len = (s.radius*7.0).max(52.0);
                let width = (1.9*scale).max(1.2);
                let arrow = P::new(s.desired_angle.cos()*len, s.desired_angle.sin()*len);
                let end = start+arrow;
                let wing = (11.0*scale).max(8.0)+width;
                let (xs, ys) = copies(P::new(start.x.min(end.x), start.y.min(end.y)),
                    P::new(start.x.max(end.x), start.y.max(end.y)),
                    P::new(wing*inverse_x, wing*inverse_y), arena, walls);
                for xi in xs.first..=xs.last {
                    for yi in ys.first..=ys.last {
                        let a = map(start+P::new(xi as f64*xs.extent, yi as f64*ys.extent));
                        let end = a+P::new(s.desired_angle.cos()*len*sx, s.desired_angle.sin()*len*sy);
                        let c = Color::new(255, 255, 255, 235);
                        sink.segment(a, end, width, c);
                        let angle = (end.y-a.y).atan2(end.x-a.x);
                        for side in [-0.62, 0.62] {
                            let wing = angle+std::f64::consts::PI+side;
                            sink.segment(end, end+P::new(wing.cos(), wing.sin())*(11.0*scale).max(8.0), width, c);
                        }
                    }
                }
            }
        }
        Output {
            vertex_count: sink.count,
            dense_food: self.dense as u32,
            reserved: 0
        }
    }
}
