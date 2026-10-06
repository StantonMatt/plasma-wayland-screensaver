// SPDX-License-Identifier: GPL-3.0-or-later
use snakes_core::{ffi::*, flags, event_flags};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
thread_local! {
    static COUNT: Cell<Option<usize>>=const {
        Cell::new(None)
    };
}
struct Allocator;
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        COUNT.with(|c|if let Some(n) = c.get() {
            c.set(Some(n+1));
        });
        unsafe {
            System.alloc(l)
        }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe {
            System.dealloc(p, l)
        }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        COUNT.with(|c|if let Some(n) = c.get() {
            c.set(Some(n+1));
        });
        unsafe {
            System.realloc(p, l, n)
        }
    }
}
#[global_allocator]static ALLOCATOR: Allocator = Allocator;
fn params() -> RenderParams {
    RenderParams {
        viewport_width: 3440.0,
        viewport_height: 1440.0,
        scale_x: 1.0,
        scale_y: 1.0,
        interpolation: 0.5,
        presentation_time: 20.0,
        deadly_walls: 1,
        ..Default::default()
    }
}
fn info() -> FrameInfo {
    FrameInfo {
        tick: 600,
        simulation_time: 20.0,
        world_width: 3440.0,
        world_height: 1440.0,
        ..Default::default()
    }
}
fn snake() -> SnakeRecord {
    SnakeRecord {
        id: 0,
        generation: 1,
        alive: 1,
        radius: 8.0,
        segment_count: 3,
        ..Default::default()
    }
}
fn body() -> [SegmentRecord;3] {
    std::array::from_fn(|i|SegmentRecord {
        x: 200.0-i as f32*8.0,
        y: 200.0,
        previous_x: 199.0-i as f32*8.0,
        previous_y: 200.0
    })
}
fn palette() -> [RenderColor;1] {
    [RenderColor {
        red: 77,
        green: 230,
        blue: 255,
        alpha: 255
    }]
}
fn build(r: &mut RenderHandle, s: SnakeRecord, p: RenderParams) -> Vec<RenderVertex> {
    let mut out = vec![RenderVertex::default();4096];
    let result = r.build(&info(), &[s], &body(), &[], &[], &palette(), &p, &mut out);
    out.truncate(result.vertex_count);
    out
}
#[test]fn packed_abi_layouts() {
    assert_eq!(std::mem::size_of::<RenderVertex>(), 12);
    assert_eq!(std::mem::align_of::<RenderVertex>(), 4);
    assert_eq!(std::mem::offset_of!(RenderVertex, color), 8);
    assert_eq!(std::mem::size_of::<RenderParams>(), 72);
    assert_eq!(std::mem::size_of::<RenderOutput>(), 16);
}
#[test]fn flags_change_colors_and_eyes_without_extra_vertices_and_only_leader_has_crown() {
    let mut r = RenderHandle::new();
    let mut s = snake();
    let ordinary = build(&mut r, s, params());
    s.flags = flags::BOOSTING;
    let boosted = build(&mut r, s, params());
    assert_eq!(ordinary.len(), boosted.len());
    assert!(boosted.iter().any(|v|v.color==RenderColor {
        red: 139,
        green: 239,
        blue: 255,
        alpha: 245
    }));
    s.flags = flags::HUNTING;
    let hunting = build(&mut r, s, params());
    assert_eq!(ordinary.len(), hunting.len());
    assert!(hunting.iter().any(|v|v.color==RenderColor {
        red: 255,
        green: 190,
        blue: 80,
        alpha: 255
    }));
    s.flags = flags::TRAPPED;
    let trapped = build(&mut r, s, params());
    assert_eq!(ordinary.len(), trapped.len());
    assert_ne!(ordinary, trapped);
    s.flags = flags::LEADER;
    assert_eq!(build(&mut r, s, params()).len(), ordinary.len()+81);
}
#[test]fn pellet_has_two_small_dim_four_sided_discs() {
    let mut r = RenderHandle::new();
    let mut f = FoodRecord {
        x: 200.0,
        y: 200.0,
        size: 4.0,
        ..Default::default()
    };
    let mut out = [RenderVertex::default();100];
    let full = r.build(&info(), &[], &[], &[f], &[], &palette(), &params(), &mut out);
    assert_eq!(full.vertex_count, 63);
    f.kind = 2;
    let pellet = r.build(&info(), &[], &[], &[f], &[], &palette(), &params(), &mut out);
    assert_eq!(pellet.vertex_count, 24);
    assert_eq!(out[0].color.alpha, 12);
    assert_eq!(out[12].color.alpha, 110);
    assert!(out[1].x-200.0<4.0*3.2);
}
#[test]fn contrail_ring_is_bounded_and_resets_on_generation_rewind_and_resize() {
    let mut r = RenderHandle::new();
    let mut s = snake();
    s.flags = flags::BOOSTING;
    let mut b = body();
    let mut i = info();
    let mut p = params();
    let mut out = [RenderVertex::default();4096];
    let base = r.build(&i, &[s], &b, &[], &[], &palette(), &p, &mut out).vertex_count;
    for t in 1..40 {
        i.tick += 1;
        i.simulation_time += 1.0/30.0;
        p.presentation_time = i.simulation_time;
        b[2].x -= 2.0;
        b[2].previous_x -= 2.0;
        let count = r.build(&i, &[s], &b, &[], &[], &palette(), &p, &mut out).vertex_count;
        assert_eq!(count, base+6*t.min(14));
        assert_eq!(r.build(&i, &[s], &b, &[], &[], &palette(), &p, &mut out).vertex_count, count);
    }
    s.generation += 1;
    i.tick += 1;
    assert_eq!(r.build(&i, &[s], &b, &[], &[], &palette(), &p, &mut out).vertex_count, base);
    i.tick = 0;
    assert_eq!(r.build(&i, &[s], &b, &[], &[], &palette(), &p, &mut out).vertex_count, base);
    i.geometry_generation += 1;
    assert_eq!(r.build(&i, &[s], &b, &[], &[], &palette(), &p, &mut out).vertex_count, base);
}
#[test]fn kill_flash_lasts_half_second_is_deduplicated_and_rewind_clears_history() {
    let mut r = RenderHandle::new();
    let mut i = info();
    let mut p = params();
    let e = EventRecord {
        tick: 600,
        x: 200.0,
        y: 200.0,
        kind: 0,
        ..Default::default()
    };
    let mut out = [RenderVertex::default();4096];
    assert_eq!(r.build(&i, &[], &[], &[], &[e], &palette(), &p, &mut out).vertex_count, 72);
    i.tick += 1;
    p.presentation_time = 20.2;
    assert_eq!(r.build(&i, &[], &[], &[], &[e], &palette(), &p, &mut out).vertex_count, 72);
    let alpha = out[0].color.alpha;
    i.tick += 1;
    p.presentation_time = 20.499;
    assert_eq!(r.build(&i, &[], &[], &[], &[], &palette(), &p, &mut out).vertex_count, 72);
    assert!(out[0].color.alpha<alpha);
    p.presentation_time = 20.5;
    assert_eq!(r.build(&i, &[], &[], &[], &[], &palette(), &p, &mut out).vertex_count, 0);
    i.tick = 1;
    p.presentation_time = 0.03;
    assert_eq!(r.build(&i, &[], &[], &[], &[], &palette(), &p, &mut out).vertex_count, 0);
}
#[test]fn corpse_fades_all_primitives() {
    let mut r = RenderHandle::new();
    let mut s = snake();
    s.flags = flags::CORPSE|flags::LEADER;
    s.alive = 0;
    let initial = build(&mut r, s, params());
    assert!(!initial.is_empty());
    let mut p = params();
    p.presentation_time += 0.3;
    let faded = build(&mut r, s, p);
    assert_eq!(initial.len(), faded.len());
    for (a, b) in initial.iter().zip(&faded) {
        assert!(b.color.alpha<a.color.alpha);
        assert_eq!(a.x, b.x);
    }
    p.presentation_time = 20.55;
    assert!(build(&mut r, s, p).iter().all(|v|v.color.alpha==0));
}
#[test]fn render_ffi_validates_and_retry_reports_complete_count() {
    unsafe {
        let r = snakes_core_render_create();
        let mut result = RenderOutput::default();
        let s = snake();
        let b = body();
        let pal = palette();
        let p = params();
        let i = info();
        let call = |s: *const SnakeRecord, out: *mut RenderVertex, n: usize, result: *mut RenderOutput|snakes_core_render_build(r, &i, s, 1, b.as_ptr(), b.len(), std::ptr::null(), 0, std::ptr::null(), 0, pal.as_ptr(), pal.len(), &p, out, n, result);
        assert_eq!(call(&s, std::ptr::null_mut(), 0, &mut result), BUFFER_TOO_SMALL);
        let n = result.vertex_count;
        let mut vertices = vec![RenderVertex::default();n];
        assert_eq!(call(&s, vertices.as_mut_ptr(), n, &mut result), OK);
        assert_eq!(result.vertex_count, n);
        let invalid = SnakeRecord {
            segment_offset: 999,
            ..s
        };
        result.vertex_count = 123;
        let copy = vertices.clone();
        assert_eq!(call(&invalid, vertices.as_mut_ptr(), n, &mut result), INVALID_ARGUMENT);
        assert_eq!(result.vertex_count, 123);
        assert_eq!(vertices, copy);
        assert_eq!(call(std::ptr::null(), vertices.as_mut_ptr(), n, &mut result), INVALID_ARGUMENT);
        assert_eq!(snakes_core_render_reset(r), OK);
        snakes_core_render_destroy(r);
        snakes_core_render_destroy(std::ptr::null_mut());
    }
}
#[test]fn zero_allocations_building_all_r1_effects_and_resetting_history() {
    let mut r = RenderHandle::new();
    let mut i = info();
    let mut p = params();
    let mut s = snake();
    s.flags = flags::BOOSTING|flags::LEADER|flags::HUNTING|flags::TRAPPED;
    let mut b = body();
    let food = [FoodRecord {
        x: 0.1,
        y: 0.1,
        size: 4.0,
        kind: 2,
        attraction: 1.0,
        attraction_x: 10.0,
        attraction_y: 10.0,
        ..Default::default()
    }];
    let pal = palette();
    let mut out = vec![RenderVertex::default();4096];
    COUNT.with(|c|c.set(Some(0)));
    for t in 0..1000 {
        i.tick += 1;
        i.simulation_time += 1.0/30.0;
        p.presentation_time = i.simulation_time;
        p.deadly_walls = (t%2) as u32;
        p.developer_mode = 1;
        b[2].x = (t%20) as f32;
        if t % 100 > 80 {
            s.alive = 0;
            s.flags = flags::CORPSE;
        } else {
            s.alive = 1;
            s.flags = flags::BOOSTING | flags::LEADER | flags::HUNTING | flags::TRAPPED;
        }
        let event = EventRecord {
            tick: i.tick,
            x: 0.1,
            y: 0.1,
            kind: 0,
            ..Default::default()
        };
        r.build(&i, &[s], &b, &food, &[event], &pal, &p, &mut out);
        if t%30==0 {
            r.reset();
        }
    }
    let n = COUNT.with(|c|c.replace(None).unwrap());
    assert_eq!(n, 0);
}
// The constants are measured from the pre-port C++ vertex buffers. Each byte
// (including f32 coordinates and RGBA) participates, so this checks draw order,
// clipping, interpolation, circle tessellation and malformed-point guards.
fn mature_fixture() -> (Vec<SnakeRecord>, Vec<SegmentRecord>, Vec<FoodRecord>) {
    let mut snakes = Vec::new();
    let mut segments = Vec::new();
    let mut food = Vec::new();
    for n in 0..14 {
        snakes.push(SnakeRecord {
            id: n,
            generation: 1,
            alive: 1,
            color_index: 0,
            radius: 8.0,
            angle: 0.0,
            desired_angle: 0.7,
            segment_offset: segments.len() as u32,
            segment_count: 120,
            flags: flags::LEADER,
            ..Default::default()
        });
        for i in 0..120 {
            let x = (250.0-i as f64*4.5+(n%4) as f64*780.0) as f32;
            let y = (120.0+(i as f64*0.08).sin()*18.0+(n/4) as f64*340.0) as f32;
            segments.push(SegmentRecord {
                x: if i==7 {
                    f32::NAN
                } else {
                    x
                },
                y,
                previous_x: x-1.0,
                previous_y: y
            });
        }
    }
    for i in 0..400 {
        let x = (12+(i%32)*9) as f32;
        let y = (12+(i/32)*9) as f32;
        food.push(FoodRecord {
            id: i+1,
            x,
            y,
            size: 4.0,
            attraction_x: x,
            attraction_y: y,
            ..Default::default()
        });
    }
    (snakes, segments, food)
}
fn fingerprint(vertices: &[RenderVertex]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for v in vertices {
        for b in v.x.to_bits().to_le_bytes().into_iter().chain(v.y.to_bits().to_le_bytes()).chain([v.color.red, v.color.green, v.color.blue, v.color.alpha]) {
            hash = (hash^b as u64).wrapping_mul(0x100000001b3);
        }
    }
    hash
}
#[test]fn original_cpp_geometry_fingerprints() {
    let (snakes, segments, food) = mature_fixture();
    let mut out = vec![RenderVertex::default();200000];
    for (walls, developer, alpha, expected) in [(true, false, 0.5, 0x277138f72018d760u64), (false, false, 0.9, 0x9835584fd581399bu64), (true, true, 0.1, 0x97f0972248a549edu64)] {
        let mut p = params();
        p.deadly_walls = walls as u32;
        p.developer_mode = developer as u32;
        p.interpolation = alpha;
        let result = RenderHandle::new().build(&info(), &snakes, &segments, &food, &[], &palette(), &p, &mut out);
        let actual = fingerprint(&out[..result.vertex_count]);
        assert_eq!(actual, expected);
    }
}

#[test]
fn contrail_history_expires_after_skipped_presentation_ticks() {
    let mut r = RenderHandle::new();
    let mut s = snake();
    s.flags = flags::BOOSTING;
    let mut b = body();
    let mut i = info();
    let mut p = params();
    let mut out = [RenderVertex::default();4096];
    let base = r.build(&i,&[s],&b,&[],&[],&palette(),&p,&mut out).vertex_count;
    for _ in 0..20 {
        i.tick += 3;
        i.simulation_time += 0.1;
        p.presentation_time = i.simulation_time;
        b[2].x += 2.0;
        b[2].previous_x += 2.0;
        let count = r.build(&i,&[s],&b,&[],&[],&palette(),&p,&mut out).vertex_count;
        assert!(count <= base + 24);
    }
    i.tick += 16;
    i.simulation_time += 16.0 / 30.0;
    p.presentation_time = i.simulation_time;
    assert_eq!(r.build(&i,&[s],&b,&[],&[],&palette(),&p,&mut out).vertex_count,base);
}


#[test]
fn stationary_snakes_do_not_replay_stale_interpolation() {
    for walls in [0, 1] {
        for stationary in [flags::CORPSE] {
            let mut r = RenderHandle::new();
            let mut s = snake();
            s.flags = stationary | flags::LEADER;
            s.alive = u32::from(stationary != flags::CORPSE);
            let mut i = info();
            let mut p = params();
            p.deadly_walls = walls;
            p.developer_mode = 1;
            let mut b = body();
            // A corpse final move crossing a seam must stay at its endpoint.
            for seg in &mut b {
                seg.x -= 197.0;
                seg.previous_x = i.world_width as f32 - 5.0;
            }
            let mut out = [RenderVertex::default();4096];
            p.interpolation = 1.0;
            let n = r.build(&i, &[s], &b, &[], &[], &palette(), &p, &mut out).vertex_count;
            let reference = out[..n].to_vec();
            for tick in 0..3 {
                i.tick += tick;
                for alpha in [0.0, 0.25, 0.75, 1.0] {
                    p.interpolation = alpha;
                    let n = r.build(&i, &[s], &b, &[], &[], &palette(), &p, &mut out).vertex_count;
                    assert!(&out[..n] == reference, "walls={walls} stationary={stationary} alpha={alpha}");
                }
            }
        }
    }
}

#[test]
fn wrap_copies_cover_full_effect_extents_on_edges_and_corners() {
    let mut p = params();
    let mut i = info();
    i.world_width = 320.0;
    i.world_height = 240.0;
    p.viewport_width = 320.0;
    p.viewport_height = 240.0;
    p.deadly_walls = 0;
    let mut out = [RenderVertex::default();4096];
    for (x, y, copies) in [(2.0,120.0,2), (318.0,120.0,2), (160.0,2.0,2), (160.0,238.0,2), (2.0,2.0,4), (318.0,238.0,4)] {
        let e = EventRecord {tick:i.tick, x, y, kind:0, ..Default::default()};
        let n = RenderHandle::new().build(&i,&[],&[],&[],&[e],&palette(),&p,&mut out).vertex_count;
        assert_eq!(n,72*copies,"flash at {x},{y}");
        for kind in [0,2] {
            // Keep even the small offset highlight straddling the seam.
            let fx = if x<10.0 {0.01} else if x>310.0 {319.99} else {x};
            let fy = if y<10.0 {0.01} else if y>230.0 {239.99} else {y};
            let f = FoodRecord {x:fx,y:fy,size:4.0,kind,..Default::default()};
            let n = RenderHandle::new().build(&i,&[],&[],&[f],&[],&palette(),&p,&mut out).vertex_count;
            assert_eq!(n,if kind==2 {24*copies} else {63*copies},"food kind {kind} at {x},{y}");
        }
    }
    // The expanding outer disc reaches a corner before the inner one does.
    let e = EventRecord {tick:i.tick,x:20.0,y:20.0,kind:0,..Default::default()};
    p.presentation_time += 0.4;
    assert_eq!(RenderHandle::new().build(&i,&[],&[],&[],&[e],&palette(),&p,&mut out).vertex_count,180);
    p.deadly_walls = 1;
    assert_eq!(RenderHandle::new().build(&i,&[],&[],&[],&[e],&palette(),&p,&mut out).vertex_count,72);
    // Oversized transient effects stop after eight visible logical copies.
    // In this ordering only the outer discs of the first eight are visible.
    p.deadly_walls=0; p.viewport_width=10.0; p.viewport_height=10.0;
    i.world_width=10.0; i.world_height=10.0;
    let e=EventRecord{x:5.0,y:5.0,..e};
    assert_eq!(RenderHandle::new().build(&i,&[],&[],&[],&[e],&palette(),&p,&mut out).vertex_count,8*36);
}

#[test]
fn vacuum_streak_wraps_even_when_food_halo_does_not_reach_seam() {
    let mut i = info();
    i.world_width = 320.0;
    i.world_height = 240.0;
    let mut p = params();
    // Anisotropic scaling exercises conversion of the screen-space width.
    p.scale_x = 2.0;
    p.scale_y = 0.5;
    p.viewport_width = 640.0;
    p.viewport_height = 120.0;
    p.deadly_walls = 0;
    let mut out = [RenderVertex::default();4096];
    for (x,y,ax,ay) in [(22.0,120.0,52.0,120.0), (298.0,120.0,268.0,120.0), (160.0,60.0,160.0,90.0), (160.0,180.0,160.0,150.0)] {
        let f = FoodRecord {x,y,attraction_x:ax,attraction_y:ay,size:8.0,phase:-60.0,attraction:1.0,..Default::default()};
        let n = RenderHandle::new().build(&i,&[],&[],&[f],&[],&palette(),&p,&mut out).vertex_count;
        let streak = out[..n].iter().filter(|v|v.color.alpha==150).count();
        assert_eq!(streak,12,"streak at {x},{y}");
        p.deadly_walls = 1;
        let n = RenderHandle::new().build(&i,&[],&[],&[f],&[],&palette(),&p,&mut out).vertex_count;
        assert_eq!(out[..n].iter().filter(|v|v.color.alpha==150).count(),6);
        p.deadly_walls = 0;
    }
    // The halo itself also needs screen-to-world extent conversion.
    let f=FoodRecord{x:160.0,y:35.0,size:8.0,phase:-60.0,..Default::default()};
    assert_eq!(RenderHandle::new().build(&i,&[],&[],&[f],&[],&palette(),&p,&mut out).vertex_count,87);
}

#[test]
fn contrail_draws_both_sides_of_a_seam_and_stops_at_death() {
    for (a,b) in [((1.0,120.0),(319.0,120.0)), ((160.0,1.0),(160.0,239.0)), ((1.0,1.0),(319.0,239.0))] {
        let mut r = RenderHandle::new();
        let mut s = snake();
        s.flags = flags::BOOSTING;
        let mut i = info();
        i.world_width = 320.0;
        i.world_height = 240.0;
        let mut p = params();
        p.viewport_width = 320.0;
        p.viewport_height = 240.0;
        p.deadly_walls = 0;
        let mut body = body();
        let mut out = [RenderVertex::default();4096];
        body[2].x=a.0; body[2].y=a.1;
        r.build(&i,&[s],&body,&[],&[],&palette(),&p,&mut out);
        i.tick+=1;
        i.simulation_time+=1.0/30.0;
        p.presentation_time=i.simulation_time;
        body[2].x=b.0; body[2].y=b.1;
        let n=r.build(&i,&[s],&body,&[],&[],&palette(),&p,&mut out).vertex_count;
        let copies=if a.0==1.0 && a.1==1.0 {4} else {2};
        assert_eq!(out[..n].iter().filter(|v|v.color.alpha==110).count(),6*copies);
        s.alive=0; s.flags=flags::CORPSE|flags::BOOSTING;
        i.tick+=1;
        r.build(&i,&[s],&body,&[],&[],&palette(),&p,&mut out);
        s.alive=1; s.flags=flags::BOOSTING;
        i.tick+=1;
        let n=r.build(&i,&[s],&body,&[],&[],&palette(),&p,&mut out).vertex_count;
        assert!(!out[..n].iter().any(|v|v.color.alpha==110),"death must clear trail history");
    }
}


#[test]
fn eyes_and_crown_pixel_minimums_wrap_at_small_viewport_scales() {
    let mut i=info();
    i.world_width=1000.0; i.world_height=1000.0;
    let mut p=params();
    p.scale_x=0.01; p.scale_y=0.01;
    p.viewport_width=10.0; p.viewport_height=10.0;
    p.deadly_walls=0;
    let mut s=snake(); s.flags=flags::LEADER;
    let b=std::array::from_fn::<_,3,_>(|n|SegmentRecord{x:70.0+n as f32*8.0,y:500.0,previous_x:70.0+n as f32*8.0,previous_y:500.0});
    let mut out=[RenderVertex::default();4096];
    let n=RenderHandle::new().build(&i,&[s],&b,&[],&[],&palette(),&p,&mut out).vertex_count;
    assert!(out[..n].iter().any(|v|v.x>10.0 && v.color==RenderColor{red:255,green:255,blue:255,alpha:255}));
    assert!(out[..n].iter().any(|v|v.x>10.0 && v.color==RenderColor{red:109,green:67,blue:0,alpha:255}));
    for (sx,sy) in [(0.0,0.01),(0.01,0.0),(0.0,0.0)] {
        p.scale_x=sx; p.scale_y=sy;
        assert_eq!(RenderHandle::new().build(&i,&[s],&b,&[],&[],&palette(),&p,&mut out).vertex_count,0);
    }
}

#[test]
fn malformed_render_records_are_skipped_before_bounds_and_history() {
    let mut i=info();i.world_width=320.0;i.world_height=240.0;
    let mut p=params();p.viewport_width=320.0;p.viewport_height=240.0;p.deadly_walls=0;p.developer_mode=1;
    let mut out=[RenderVertex::default();4096];
    let good=FoodRecord{x:160.0,y:120.0,size:4.0,..Default::default()};
    let base=RenderHandle::new().build(&i,&[],&[],&[good],&[],&palette(),&p,&mut out).vertex_count;
    for bad in [f32::NAN,f32::INFINITY,f32::NEG_INFINITY,f32::MAX,-f32::MAX] {
        for field in 0..8 {
            let mut f=good;
            match field {0=>f.x=bad,1=>f.y=bad,2=>f.size=bad,3=>f.phase=bad,
                4=>f.attraction=bad,5=>f.attraction_x=bad,6=>f.attraction_y=bad,_=>f.size=-1.0}
            let n=RenderHandle::new().build(&i,&[],&[],&[good,f],&[],&palette(),&p,&mut out).vertex_count;
            assert_eq!(n,base,"food field {field} value {bad}");
        }
        let event=EventRecord{tick:i.tick,x:bad,y:120.0,kind:0,..Default::default()};
        assert_eq!(RenderHandle::new().build(&i,&[],&[],&[],&[event],&palette(),&p,&mut out).vertex_count,0);
        let event=EventRecord{x:160.0,y:bad,..event};
        assert_eq!(RenderHandle::new().build(&i,&[],&[],&[],&[event],&palette(),&p,&mut out).vertex_count,0);
        for field in 0..4 {
            let mut b=body();
            for seg in &mut b {
                match field {0=>seg.x=bad,1=>seg.y=bad,2=>seg.previous_x=bad,_=>seg.previous_y=bad}
            }
            let mut s=snake();s.flags=flags::BOOSTING|flags::LEADER;
            let mut r=RenderHandle::new();
            for _ in 0..2 {
                assert_eq!(r.build(&i,&[s],&b,&[],&[],&palette(),&p,&mut out).vertex_count,0);
                i.tick+=1;
            }
            // Invalid history must not become a contrail when valid segments resume.
            let n=r.build(&i,&[s],&body(),&[],&[],&palette(),&p,&mut out).vertex_count;
            assert!(n<out.len());
            assert!(out[..n].iter().all(|v|v.x.is_finite() && v.y.is_finite()));
            assert!(!out[..n].iter().any(|v|v.color.alpha==110));
        }
        for field in 0..3 {
            let mut s=snake();
            match field {0=>s.radius=bad as f64,1=>s.angle=bad as f64,_=>s.desired_angle=bad as f64}
            assert_eq!(RenderHandle::new().build(&i,&[s],&body(),&[],&[],&palette(),&p,&mut out).vertex_count,0);
        }
    }
    // A finite but giant halo/streak is rejected by the complete-tiling budget.
    let huge=FoodRecord{size:1e9,attraction:1.0,attraction_x:170.0,attraction_y:130.0,..good};
    let n=RenderHandle::new().build(&i,&[],&[],&[huge],&[],&palette(),&p,&mut out).vertex_count;
    assert!(n<=9*(63+6));
    assert!(out[..n].iter().all(|v|v.x.is_finite() && v.y.is_finite()));
}

#[test]
fn render_frame_numeric_validation_covers_all_projection_inputs() {
    let call=|i:&FrameInfo,p:&RenderParams|unsafe {
        let r=snakes_core_render_create();
        let mut output=RenderOutput{vertex_count:123,..Default::default()};
        let result=snakes_core_render_build(r,i,std::ptr::null(),0,std::ptr::null(),0,
            std::ptr::null(),0,std::ptr::null(),0,std::ptr::null(),0,p,
            std::ptr::null_mut(),0,&mut output);
        snakes_core_render_destroy(r);
        (result,output.vertex_count)
    };
    for bad in [f64::NAN,f64::INFINITY,f64::NEG_INFINITY,f64::MAX,-f64::MAX] {
        for field in 0..11 {
            let mut i=info();let mut p=params();
            match field {0=>i.world_width=bad,1=>i.world_height=bad,2=>i.simulation_time=bad,
                3=>p.viewport_width=bad,4=>p.viewport_height=bad,5=>p.scale_x=bad,6=>p.scale_y=bad,
                7=>p.offset_x=bad,8=>p.offset_y=bad,9=>p.presentation_time=bad,_=>p.interpolation=bad}
            assert_eq!(call(&i,&p),(INVALID_ARGUMENT,123),"field {field} value {bad}");
            assert_eq!(RenderHandle::new().build(&i,&[],&[],&[],&[],&palette(),&p,&mut []).vertex_count,0);
        }
    }
    for (sx,sy) in [(1e-300,1.0),(1.0,1e-300),(1e300,1.0),(1.0,1e300)] {
        let mut p=params();p.scale_x=sx;p.scale_y=sy;
        assert_eq!(call(&info(),&p),(INVALID_ARGUMENT,123));
    }
    for alpha in [-0.01,1.01] {
        let mut p=params();p.interpolation=alpha;
        assert_eq!(call(&info(),&p),(INVALID_ARGUMENT,123));
    }
}

#[test]
fn render_ffi_skips_malformed_exports_without_allocating_or_mutating_records() {
    unsafe {
        let r=snakes_core_render_create();
        let mut i=info();
        let mut p=params();p.deadly_walls=0;p.developer_mode=1;
        let mut s=snake();s.flags=flags::BOOSTING|flags::LEADER;
        let mut b=body();b[2].x=f32::INFINITY;
        let f=[FoodRecord{x:200.0,y:200.0,size:f32::INFINITY,..Default::default()},
            FoodRecord{x:200.0,y:200.0,size:4.0,attraction:1.0,attraction_x:f32::MAX,..Default::default()}];
        let e=[EventRecord{tick:i.tick,x:f32::NEG_INFINITY,y:200.0,kind:0,..Default::default()}];
        let pal=palette();let mut vertices=[RenderVertex::default();4096];let mut output=RenderOutput::default();
        COUNT.with(|c|c.set(Some(0)));
        for _ in 0..10 {
            assert_eq!(snakes_core_render_build(r,&i,&s,1,b.as_ptr(),b.len(),f.as_ptr(),f.len(),e.as_ptr(),e.len(),
                pal.as_ptr(),pal.len(),&p,vertices.as_mut_ptr(),vertices.len(),&mut output),OK);
            assert!(output.vertex_count<=vertices.len());
            assert!(vertices[..output.vertex_count].iter().all(|v|v.x.is_finite() && v.y.is_finite()));
            i.tick+=1;
        }
        assert_eq!(COUNT.with(|c|c.replace(None).unwrap()),0);
        assert!(b[2].x.is_infinite());assert!(f[0].size.is_infinite());
        snakes_core_render_destroy(r);
    }
}

#[test]
fn multi_arena_bodies_keep_every_edge_and_tail_under_viewport_scaling() {
    for (width,height,dx,dy) in [(100.0,200.0,7.0,0.0),
        (200.0,100.0,0.0,7.0),(100.0,100.0,7.0,7.0),
        (100.0,200.0,-7.0,0.0)] {
        for (sx,sy) in [(1.0,1.0),(2.0,0.5),(0.5,2.0)] {
            let mut i=info(); i.world_width=width; i.world_height=height;
            let mut p=params(); p.deadly_walls=0; p.interpolation=1.0;
            p.scale_x=sx; p.scale_y=sy;
            p.viewport_width=width*sx; p.viewport_height=height*sy;
            let s=SnakeRecord{radius:6.0,segment_count:40,..snake()};
            let b:Vec<_>=(0..40).map(|j| {
                let x=(50.0+dx*j as f64).rem_euclid(width) as f32;
                let y=(50.0+dy*j as f64).rem_euclid(height) as f32;
                SegmentRecord{x,y,previous_x:x,previous_y:y}
            }).collect();
            let mut out=vec![RenderVertex::default();20000];
            let n=RenderHandle::new().build(&i,&[s],&b,&[],&[],&palette(),&p,&mut out).vertex_count;
            assert!(n<=out.len());
            let tail=b.last().unwrap();
            let fill=RenderColor{alpha:245,..palette()[0]};
            assert!(out[..n].iter().any(|v|v.color==fill
                && (v.x as f64-tail.x as f64*sx).abs()<0.001
                && (v.y as f64-tail.y as f64*sy).abs()<0.001),
                "missing wrapped tail: arena={width}x{height}, delta={dx},{dy}, scale={sx},{sy}");
            // Independently enumerate all visible copies of each straight edge.
            // This checks the whole body, including intermediate arena crossings.
            let radius=6.0*(sx*sy).sqrt()*0.96;
            let mut edge_copies=0;
            for j in 1..40 {
                let (ax,ay)=(50.0+dx*(j-1) as f64,50.0+dy*(j-1) as f64);
                let (bx,by)=(ax+dx,ay+dy);
                for x in -4..=4 { for y in -4..=4 {
                    let (tx,ty)=(x as f64*width,y as f64*height);
                    if (ax.min(bx)+tx)*sx-radius<p.viewport_width
                        && (ax.max(bx)+tx)*sx+radius>0.0
                        && (ay.min(by)+ty)*sy-radius<p.viewport_height
                        && (ay.max(by)+ty)*sy+radius>0.0 { edge_copies+=1; }
                }}
            }
            // Tail caps have 36 vertices, edge quads have six. These fixtures
            // keep the tail cap away from seams at every tested projection.
            assert_eq!(out[..n].iter().filter(|v|v.color==fill).count(),
                edge_copies*6+36,"incomplete body at {width}x{height}, delta={dx},{dy}, scale={sx},{sy}");
        }
    }
}

#[test]
fn fallback_stump_and_head_decorations_survive_split_sections_without_allocations() {
    for (width,height,dx,dy) in [(100.0,200.0,7.0,0.0),(200.0,100.0,0.0,7.0),
        (100.0,100.0,7.0,7.0),(100.0,200.0,-7.0,0.0),(500.0,200.0,7.0,0.0)] {
        for (sx,sy) in [(1.0,1.0),(2.0,0.5),(0.5,2.0)] {
            let mut i=info();i.world_width=width;i.world_height=height;
            let mut p=params();p.deadly_walls=0;p.interpolation=1.0;p.scale_x=sx;p.scale_y=sy;
            p.viewport_width=width*sx;p.viewport_height=height*sy;
            let s=SnakeRecord {radius:6.0,segment_count:40,stump_ticks:48,flags:flags::LEADER,..snake()};
            let b:Vec<_>=(0..40).map(|j| {
                let x=(50.0+dx*j as f64).rem_euclid(width) as f32;
                let y=(50.0+dy*j as f64).rem_euclid(height) as f32;
                SegmentRecord {x,y,previous_x:x,previous_y:y}
            }).collect();
            let mut out=vec![RenderVertex::default();20000];let mut r=RenderHandle::new();
            COUNT.with(|c|c.set(Some(0)));
            let n=r.build(&i,&[s],&b,&[],&[],&palette(),&p,&mut out).vertex_count;
            assert_eq!(COUNT.with(|c|c.replace(None).unwrap()),0);
            let marker:Vec<_>=out[..n].iter().filter(|v|v.color.alpha==220).collect();
            assert_eq!(marker.len(),24,"one eight-sided stump at {width}x{height}, delta={dx},{dy}, scale={sx},{sy}");
            let tail=b.last().unwrap();
            assert!(marker.iter().any(|v|(v.x as f64-tail.x as f64*sx).abs()<0.001 && (v.y as f64-tail.y as f64*sy).abs()<0.001));
            // Eyes and crown remain head-only; section splitting must not duplicate them.
            assert_eq!(out[..n].iter().filter(|v|v.color==RenderColor {red:255,green:255,blue:255,alpha:255}).count(),48);
            assert_eq!(out[..n].iter().filter(|v|v.color==RenderColor {red:255,green:216,blue:74,alpha:255}).count(),21);
        }
    }
}

#[test]
fn oversized_primitives_match_complete_tiling_in_small_scaled_arenas() {
    for (sx,sy) in [(1.0,1.0),(2.0,0.5)] {
        let mut i=info();i.world_width=10.0;i.world_height=10.0;
        let mut p=params();p.deadly_walls=0;p.scale_x=sx;p.scale_y=sy;
        p.viewport_width=10.0*sx;p.viewport_height=10.0*sy;
        p.presentation_time+=0.4;
        let f=FoodRecord{x:5.0,y:5.0,size:12.0,..Default::default()};
        let e=EventRecord{tick:i.tick,x:5.0,y:5.0,kind:0,..Default::default()};
        let b=[SegmentRecord{x:5.0,y:5.0,previous_x:5.0,previous_y:5.0};3];
        let s=SnakeRecord{radius:0.5,..snake()};
        let mut out=vec![RenderVertex::default();100000];
        // Food halo/highlight, expanding flash, and minimum-length steering
        // arrow each extend beyond the immediately neighboring arena copies.
        for primitive in 0..3 {
            p.developer_mode=u32::from(primitive==2);
            let snakes=if primitive==2 {std::slice::from_ref(&s)} else {&[]};
            let food=if primitive==0 {std::slice::from_ref(&f)} else {&[]};
            let events=if primitive==1 {std::slice::from_ref(&e)} else {&[]};
            let actual=RenderHandle::new().build(&i,snakes,&b,food,events,&palette(),&p,&mut out).vertex_count;
            assert!(actual<=out.len());
            let mut expected=0;let mut effect_copies=0;
            let mut tile=p;tile.deadly_walls=1;
            for x in -10..=10 {for y in -10..=10 {
                tile.offset_x=x as f64*10.0*sx;
                tile.offset_y=y as f64*10.0*sy;
                let n=RenderHandle::new().build(&i,snakes,&b,food,events,&palette(),&tile,&mut out).vertex_count;
                if primitive!=1 || effect_copies<8 {expected+=n;}
                if n>0 {effect_copies+=1;}
            }}
            assert_eq!(actual,expected,"primitive={primitive}, scale={sx},{sy}");
        }
    }
}

#[test]
fn contrails_rebase_samples_outside_the_original_arena() {
    let mut i=info();i.world_width=100.0;i.world_height=100.0;
    let mut p=params();p.deadly_walls=0;p.scale_x=2.0;p.scale_y=0.5;
    p.viewport_width=200.0;p.viewport_height=50.0;
    let s=SnakeRecord{flags:flags::BOOSTING,radius:6.0,..snake()};
    let mut canonical=RenderHandle::new();let mut displaced=RenderHandle::new();
    let mut a=[RenderVertex::default();4096];let mut b=a;
    for tick in 0..3 {
        i.tick+=1;i.simulation_time+=1.0/30.0;p.presentation_time=i.simulation_time;
        let mut segments=[SegmentRecord{x:2.0+tick as f32*2.0,y:50.0,
            previous_x:2.0+tick as f32*2.0,previous_y:50.0};3];
        let n=canonical.build(&i,&[s],&segments,&[],&[],&palette(),&p,&mut a).vertex_count;
        for seg in &mut segments {seg.x+=300.0;seg.previous_x+=300.0;}
        let m=displaced.build(&i,&[s],&segments,&[],&[],&palette(),&p,&mut b).vertex_count;
        assert_eq!(a[..n].iter().filter(|v|v.color.alpha==110).collect::<Vec<_>>(),
            b[..m].iter().filter(|v|v.color.alpha==110).collect::<Vec<_>>());
    }
}

#[test]
fn maximum_length_multi_arena_body_is_complete_and_allocation_free() {
    let mut i=info();i.world_width=100.0;i.world_height=100.0;
    let mut p=params();p.deadly_walls=0;p.viewport_width=100.0;p.viewport_height=100.0;
    p.interpolation=1.0;
    let s=SnakeRecord{radius:6.0,segment_count:1600,..snake()};
    let b:Vec<_>=(0..1600).map(|j| {
        let x=(50.0+7.0*j as f64).rem_euclid(100.0) as f32;
        SegmentRecord{x,y:x,previous_x:x,previous_y:x}
    }).collect();
    let mut out=vec![RenderVertex::default();100000];
    let mut r=RenderHandle::new();
    let fill=RenderColor{alpha:245,..palette()[0]};
    let mut edges=0;
    for j in 1..1600 {
        let a=50.0+7.0*(j-1) as f64;let b=a+7.0;
        let copies=((100.0+5.76-a)/100.0).floor()-((-5.76-b)/100.0).ceil()+1.0;
        edges+=(copies*copies) as usize;
    }
    COUNT.with(|c|c.set(Some(0)));
    for _ in 0..10 {
        let n=r.build(&i,&[s],&b,&[],&[],&palette(),&p,&mut out).vertex_count;
        assert!(n<=out.len());
        assert_eq!(out[..n].iter().filter(|v|v.color==fill).count(),edges*6+36);
        // Every outline precedes every fill in the split path.
        let last_outline=out[..n].iter().rposition(|v|v.color==RenderColor{red:5,green:7,blue:16,alpha:175}).unwrap();
        let first_fill=out[..n].iter().position(|v|v.color==fill).unwrap();
        assert!(last_outline<first_fill);
    }
    assert_eq!(COUNT.with(|c|c.replace(None).unwrap()),0);
    // Extreme finite extents must also terminate with a tiny caller buffer.
    i.world_width=1e-6;i.world_height=1e-6;
    let huge=SnakeRecord{radius:1e9,..s};
    assert_eq!(r.build(&i,&[huge],&b,&[],&[],&palette(),&p,&mut out[..1]).vertex_count,0);
}

#[test]
fn history_only_replay_preserves_corpses_flashes_trails_and_lod_without_allocating() {
    let mut reference = RenderHandle::new();
    let mut replay = RenderHandle::new();
    let mut i = info();
    let mut p = params();
    let mut s = snake();
    let b = body();
    let mut compact = s;
    compact.segment_offset = 0; compact.segment_count = 1;
    let tail = [*b.last().unwrap()];
    let food = vec![FoodRecord { x: 20.0, y: 20.0, size: 4.0, ..FoodRecord::default() }; 350];
    let mut a = vec![RenderVertex::default(); 30000];
    let mut c = vec![RenderVertex::default(); 30000];
    let pal = palette();
    COUNT.with(|count| count.set(Some(0)));
    for tick in 0..10 {
        i.tick = 600 + tick; i.simulation_time = i.tick as f64 / 30.0;
        p.presentation_time = i.simulation_time;
        s.flags = if tick < 4 { flags::BOOSTING | flags::LEADER } else { flags::CORPSE };
        s.alive = u32::from(tick < 4);
        compact.flags = s.flags; compact.alive = s.alive;
        let death = EventRecord { tick: i.tick, x: 200.0, y: 120.0, kind: 0, ..EventRecord::default() };
        let events = if tick == 4 { std::slice::from_ref(&death) } else { &[] };
        let food = if tick < 7 { &food[..] } else { &food[..300] };
        let expected = reference.build(&i, &[s], &b, food, events, &pal, &p, &mut a);
        let mut history = p; history.scale_x = 0.0; history.scale_y = 0.0;
        let result = replay.build(&i, &[compact], &tail, food, events, &pal, &history, &mut []);
        assert_eq!(result.vertex_count, 0);
        if tick == 3 || tick == 9 {
            let actual = replay.build(&i, &[s], &b, food, events, &pal, &p, &mut c);
            assert_eq!(actual.dense_food, expected.dense_food);
            assert_eq!(actual.vertex_count, expected.vertex_count);
            assert_eq!(a[..actual.vertex_count], c[..actual.vertex_count]);
        }
    }
    assert_eq!(COUNT.with(|count| count.replace(None).unwrap()), 0);
}

#[test]
fn shader_layout_and_per_element_budgets() {
    assert_eq!(std::mem::size_of::<ShaderRenderVertex>(),24);
    assert_eq!(std::mem::offset_of!(ShaderRenderVertex,color),16);
    assert_eq!(std::mem::offset_of!(ShaderRenderVertex,params),20);
    let mut out=vec![ShaderRenderVertex::default();200000];
    for length in [2,12,24,99,100,249,250,400] {
        let s=SnakeRecord{segment_count:length,..snake()};
        let b:Vec<_>=(0..length).map(|i|SegmentRecord{x:2000.0-i as f32*4.5,y:200.0,previous_x:2000.0-i as f32*4.5,previous_y:200.0}).collect();
        let result=RenderHandle::new().build_shader(&info(),&[s],&b,&[],&[],&palette(),&params(),&mut out);
        assert_eq!(result.vertex_count,6*(length as usize-1)+6);
        let body=out[..result.vertex_count].iter().filter(|v|v.params[0]==0).count();
        let head=out[..result.vertex_count].iter().filter(|v|v.params[0]==1).count();
        assert_eq!(body,6*(length as usize-1));assert_eq!(head,6);
        assert_eq!(out[0].along,(length-1) as f32);
        assert_eq!(out[body-1].along,0.0);
        // Exact shared neck and tail envelopes; the shader points the last edge.
        assert!(((out[0].y-200.0).abs()-8.0*0.84*2.5).abs()<0.0001);
        assert!(((out[body-1].y-out[body-4].y).abs()-8.0*0.22*2.5*2.0).abs()<0.0001);
        assert_eq!((out[body-1].y+out[body-4].y)*0.5,200.0);
    }
    for kind in 0..=4 {
        let f=FoodRecord{x:200.0,y:200.0,size:4.0,kind,life_fraction:255,..Default::default()};
        let n=RenderHandle::new().build_shader(&info(),&[],&[],&[f],&[],&palette(),&params(),&mut out).vertex_count;
        assert_eq!(n,6);
    }
}

fn shader_fingerprint(vertices:&[ShaderRenderVertex])->u64 {
    let mut hash=0xcbf29ce484222325u64;
    for v in vertices {
        for b in v.x.to_bits().to_le_bytes().into_iter().chain(v.y.to_bits().to_le_bytes())
            .chain(v.across.to_bits().to_le_bytes()).chain(v.along.to_bits().to_le_bytes())
            .chain([v.color.red,v.color.green,v.color.blue,v.color.alpha]).chain(v.params) {
            hash=(hash^b as u64).wrapping_mul(0x100000001b3);
        }
    }
    hash
}
#[test]
fn shader_geometry_fingerprints() {
    let (s,b,f)=mature_fixture();let mut out=vec![ShaderRenderVertex::default();200000];
    // Common-colour waves carry one origin across the entire ribbon; the
    // vertex shader suppresses zero-strength light before interpolation.
    let actual=[(true,0.5),(false,0.9)].map(|(walls,alpha)| {
        let p=RenderParams{deadly_walls:walls as u32,interpolation:alpha,..params()};
        let n=RenderHandle::new().build_shader(&info(),&s,&b,&f,&[],&palette(),&p,&mut out).vertex_count;
        (n,shader_fingerprint(&out[..n]))
    });
    assert_eq!(actual,[(10896,0x0eca5ce6f83aeca6u64),(12804,0x595240754ced00beu64)]);

}
#[test]
fn shader_render_growth_effects_and_ffi_are_allocation_free() {
    let mut r=RenderHandle::new();let mut i=info();let pal=palette();let mut p=params();
    let mut out=vec![ShaderRenderVertex::default();200000];
    let (_,b,f)=mature_fixture();let mut s=snake();
    COUNT.with(|c|c.set(Some(0)));
    for t in 0..100 {
        i.tick+=1;i.simulation_time+=1.0/30.0;p.presentation_time=i.simulation_time;
        p.developer_mode=1;p.deadly_walls=(t%2) as u32;
        s.segment_count=if t%2==0 { 12 } else { 120 };s.flags=if t%20<10 { flags::BOOSTING|flags::LEADER } else { flags::CORPSE };
        let e=EventRecord{tick:i.tick,x:200.0,y:200.0,other_snake_id:0,..Default::default()};
        let result=r.build_shader(&i,&[s],&b,&f,&[e],&pal,&p,&mut out);
        assert!(result.vertex_count<out.len());
        unsafe {
            let mut result=RenderOutput::default();
            assert_eq!(snakes_core_render_build_shader(&mut r,&i,&s,1,b.as_ptr(),b.len(),f.as_ptr(),f.len(),&e,1,pal.as_ptr(),pal.len(),&p,out.as_mut_ptr(),out.len(),&mut result),OK);
        }
        if t%30==0 { r.reset(); }
    }
    let allocations=COUNT.with(|c|c.replace(None).unwrap());assert_eq!(allocations,0);
}
#[test]
fn shader_effect_quad_cap_and_retry_are_stable() {
    let mut r=RenderHandle::new();let mut i=info();let p=params();let pal=palette();
    let events:Vec<_>=(0..32).map(|j|EventRecord{tick:i.tick,x:200.0+j as f32,y:200.0,..Default::default()}).collect();
    let mut out=[ShaderRenderVertex::default();1024];
    let n=r.build_shader(&i,&[],&[],&[],&events,&pal,&p,&mut []).vertex_count;
    assert_eq!(n,8*6);
    assert_eq!(r.build_shader(&i,&[],&[],&[],&events,&pal,&p,&mut out).vertex_count,n);
    i.tick+=1;i.simulation_time+=1.0;
    let expired=RenderParams{presentation_time:i.simulation_time,..p};
    assert_eq!(r.build_shader(&i,&[],&[],&[],&[],&pal,&expired,&mut out).vertex_count,0);
}

#[test]
fn shader_trail_budget_and_generation_reset() {
    let mut r=RenderHandle::new();let mut i=info();let mut p=params();let mut b=body();
    let mut s=SnakeRecord{flags:flags::BOOSTING,..snake()};
    let mut out=[ShaderRenderVertex::default();4096];
    for tick in 0..20 {
        i.tick+=1;i.simulation_time+=1.0/30.0;p.presentation_time=i.simulation_time;
        for segment in &mut b { segment.previous_x=segment.x;segment.x+=2.0; }
        let count=r.build_shader(&i,&[s],&b,&[],&[],&palette(),&p,&mut out).vertex_count;
        let trail=out[..count].iter().filter(|v|v.params[0]==10).count();
        assert!(trail<=84);if tick>15 { assert_eq!(trail,84); }
    }
    s.generation+=1;i.tick+=1;
    let n=r.build_shader(&i,&[s],&b,&[],&[],&palette(),&p,&mut out).vertex_count;
    assert!(!out[..n].iter().any(|v|v.params[0]==10));
}
#[test]
fn shader_history_tail_samples_do_not_place_boost_ring_at_tail() {
    let mut r=RenderHandle::new();let mut i=info();let mut p=params();
    let s=SnakeRecord{flags:flags::BOOSTING,..snake()};let b=body();
    let compact=SnakeRecord{segment_count:1,..s};
    let history=RenderParams{scale_x:0.0,scale_y:0.0,..p};
    r.build_shader(&i,&[compact],&b[2..],&[],&[],&palette(),&history,&mut []);
    i.tick+=1;i.simulation_time+=1.0/30.0;p.presentation_time=i.simulation_time;
    let mut out=[ShaderRenderVertex::default();4096];
    let n=r.build_shader(&i,&[s],&b,&[],&[],&palette(),&p,&mut out).vertex_count;
    let ring:Vec<_>=out[..n].iter().filter(|v|v.params[0]==9).collect();assert_eq!(ring.len(),6);
    let center_x=(ring[0].x+ring[5].x)*0.5;assert_eq!(center_x,b[0].x);
}

#[test]
fn reduced_motion_shortens_discrete_effects_without_changing_classic() {
    let mut normal=RenderHandle::new();let mut reduced=RenderHandle::new();
    unsafe { assert_eq!(snakes_core_render_set_reduced_motion(&mut reduced,1),OK); }
    let i=info();let p=params();let pal=palette();let b=body();
    let s=SnakeRecord{flags:flags::CORPSE,alive:0,..snake()};
    let event=EventRecord{tick:i.tick,x:200.0,y:200.0,..Default::default()};
    let mut a=[ShaderRenderVertex::default();4096];let mut c=a;
    normal.build_shader(&i,&[s],&b,&[],&[event],&pal,&p,&mut a);
    reduced.build_shader(&i,&[s],&b,&[],&[event],&pal,&p,&mut c);
    let aged=RenderParams{presentation_time:i.simulation_time+0.36,..p};
    assert!(normal.build_shader(&i,&[s],&b,&[],&[],&pal,&aged,&mut a).vertex_count>0);
    assert_eq!(reduced.build_shader(&i,&[s],&b,&[],&[],&pal,&aged,&mut c).vertex_count,0);
    let mut x=[RenderVertex::default();4096];let mut y=x;
    normal.reset();reduced.reset();
    let n=normal.build(&i,&[s],&b,&[],&[event],&pal,&aged,&mut x).vertex_count;
    let m=reduced.build(&i,&[s],&b,&[],&[event],&pal,&aged,&mut y).vertex_count;
    assert_eq!(n,m);assert_eq!(x[..n],y[..m]);
    // Kill eye flares use the same shortened death-effect duration.
    normal.reset();reduced.reset();
    let alive=snake();let kill=EventRecord{other_snake_id:0,..event};
    normal.build_shader(&i,&[alive],&b,&[],&[kill],&pal,&p,&mut a);
    reduced.build_shader(&i,&[alive],&b,&[],&[kill],&pal,&p,&mut c);
    let n=normal.build_shader(&i,&[alive],&b,&[],&[],&pal,&aged,&mut a).vertex_count;
    let m=reduced.build_shader(&i,&[alive],&b,&[],&[],&pal,&aged,&mut c).vertex_count;
    assert!(a[..n].iter().any(|v|v.params[0]==1 && v.params[1]&128!=0));
    assert!(!c[..m].iter().any(|v|v.params[0]==1 && v.params[1]&128!=0));
}

#[test]
fn shader_taper_payload_is_physical_and_shared_across_edges() {
    let length=16;
    let s=SnakeRecord{segment_count:length,..snake()};
    let b:Vec<_>=(0..length).map(|j|SegmentRecord{x:400.0-j as f32*9.44,y:200.0,previous_x:400.0-j as f32*9.44,previous_y:200.0}).collect();
    let mut out=[ShaderRenderVertex::default();1024];
    let n=RenderHandle::new().build_shader(&info(),&[s],&b,&[],&[],&palette(),&params(),&mut out).vertex_count;
    let body=&out[..n-6];
    for vertex in body {
        // Across is physical distance, not ±1: no diagonal-dependent UV kink.
        assert!(((vertex.y-200.0).abs()/8.0-vertex.across.abs()*2.5).abs()<1.0e-5);
        assert!((vertex.across.abs()-vertex.color.alpha as f32/255.0).abs()<=0.5/255.0+1.0e-6);
    }
    for pair in body.chunks_exact(6).collect::<Vec<_>>().windows(2) {
        assert_eq!((pair[0][2].x,pair[0][2].y,pair[0][2].across,pair[0][2].color),
                   (pair[1][0].x,pair[1][0].y,pair[1][0].across,pair[1][0].color));
    }
    assert_eq!(body.last().unwrap().along,0.0);
    let head=&out[n-6..n];
    assert_eq!(head[0].color.alpha,body[2].color.alpha);
}

#[test]
fn shader_boost_quad_expands_without_stretching_head_units() {
    let b=body();let p=params();let i=info();let mut out=[ShaderRenderVertex::default();1024];
    for flags in [0,flags::BOOSTING] {
        let s=SnakeRecord{flags,..snake()};
        let n=RenderHandle::new().build_shader(&i,&[s],&b,&[],&[],&palette(),&p,&mut out).vertex_count;
        let head:Vec<_>=out[..n].iter().filter(|v|v.params[0]==1).collect();
        assert_eq!(head.len(),6);
        let hu=8.0*1.24;
        assert!(((head[1].x-head[0].x)/(head[1].across-head[0].across)-hu).abs()<1.0e-4);
        assert!(((head[5].y-head[1].y)/(head[5].along-head[1].along)-hu).abs()<1.0e-4);
        assert_eq!(head[0].along,if flags!=0 {-2.5} else {-1.75});
        assert_eq!(head[0].across,if flags!=0 {-2.3} else {-1.0});
    }
}

#[test]
fn shader_head_angle_interpolates_shortest_arc_and_retries_stably() {
    let mut r=RenderHandle::new();let mut i=info();let mut s=SnakeRecord{angle:3.05,..snake()};
    let b=body();let mut out=[ShaderRenderVertex::default();1024];let mut p=params();
    r.build_shader(&i,&[s],&b,&[],&[],&palette(),&p,&mut out);
    i.tick+=1;i.simulation_time+=1.0/30.0;s.angle=-3.05;
    for alpha in [0.0,0.25,0.5,0.75,1.0] {
        p.interpolation=alpha;
        let n=r.build_shader(&i,&[s],&b,&[],&[],&palette(),&p,&mut out).vertex_count;
        let head=&out[n-6..n];
        let actual=((head[1].y-head[0].y) as f64).atan2((head[1].x-head[0].x) as f64);
        let expected=3.05+(std::f64::consts::TAU-6.10)*alpha;
        let error=(actual-expected).sin().atan2((actual-expected).cos());
        assert!(error.abs()<1.0e-5,"alpha={alpha}, actual={actual}");
        let snapshot=out[..n].to_vec();
        assert_eq!(r.build_shader(&i,&[s],&b,&[],&[],&palette(),&p,&mut out).vertex_count,n);
        assert_eq!(snapshot,out[..n]);
    }
    s.flags=flags::FROZEN;p.interpolation=1.0;
    let n=r.build_shader(&i,&[s],&b,&[],&[],&palette(),&p,&mut out).vertex_count;
    let head=&out[n-6..n];
    let actual=((head[1].y-head[0].y) as f64).atan2((head[1].x-head[0].x) as f64);
    assert!((actual-s.angle).abs()<1.0e-5);
    s.flags=0;s.generation+=1;s.angle=0.7;
    let n=r.build_shader(&i,&[s],&b,&[],&[],&palette(),&p,&mut out).vertex_count;
    // A new generation at the same tick is not a physics update; the host
    // normally sends it on the next tick, as respawns do.
    i.tick+=1;
    let n2=r.build_shader(&i,&[s],&b,&[],&[],&palette(),&p,&mut out).vertex_count;
    assert_eq!(n,n2);
    let head=&out[n2-6..n2];
    let actual=((head[1].y-head[0].y) as f64).atan2((head[1].x-head[0].x) as f64);
    assert!((actual-0.7).abs()<1.0e-5);
}

#[test]
fn shader_crown_white_rule_uses_entire_palette_identity() {
    let palettes:[([u32;6],bool);5]=[
        ([0xfff1a8,0xffc857,0xff7b42,0xef3e36,0x9c1c28,0xffd6a5],true),
        ([0xffffff,0xd9e1e8,0xaeb8c2,0x7f8b96,0xedf2f4,0xbac4ce],true),
        ([0xff477e,0xffbe0b,0x42e2b8,0x3a86ff,0xb967ff,0xfb5607],false),
        ([0xffc8dd,0xbde0fe,0xcaffbf,0xffd6a5,0xe7c6ff,0xa2d2ff],false),
        ([0xd9fbff,0x3dd6e8,0x3a86ff,0x7358d6,0x2aa889,0x9bf6ff],false),
    ];
    let mut out=[ShaderRenderVertex::default();1024];
    for (rgb,white) in palettes {
        let pal:Vec<_>=rgb.iter().map(|c|RenderColor{red:(c>>16) as u8,green:(c>>8) as u8,blue:*c as u8,alpha:255}).collect();
        for color_index in 0..6 {
            let s=SnakeRecord{flags:flags::LEADER,color_index,..snake()};
            let n=RenderHandle::new().build_shader(&info(),&[s],&body(),&[],&[],&pal,&params(),&mut out).vertex_count;
            assert!(out[..n].iter().filter(|v|v.params[0]<=1).all(|v|(v.params[1]&64!=0)==white));
        }
    }
}

#[test]
fn shader_corpse_keeps_width_in_alpha_and_fade_in_separate_byte() {
    let mut r=RenderHandle::new();let mut i=info();let b=body();let mut p=params();
    let mut out=[ShaderRenderVertex::default();1024];let s=SnakeRecord{alive:0,flags:flags::CORPSE,..snake()};
    r.build_shader(&i,&[s],&b,&[],&[],&palette(),&p,&mut out);
    i.tick+=1;i.simulation_time+=0.2;p.presentation_time=i.simulation_time;
    let n=r.build_shader(&i,&[s],&b,&[],&[],&palette(),&p,&mut out).vertex_count;
    assert!(n>0);
    assert!(out[..n].iter().all(|v|v.params[0]==0 && v.params[2]&128!=0 && v.params[3]>0 && v.params[3]<255));
    assert!(out[..n].iter().any(|v|v.color.alpha!=v.params[3]));
    let original=palette()[0];
    assert_eq!(out[0].color.red,(original.red as f64+(255-original.red) as f64*0.35).round() as u8);
}

#[test]
fn shader_contrail_has_shared_width_and_brightness_at_every_join() {
    let mut r=RenderHandle::new();let mut i=info();let mut p=params();let mut b=body();
    let s=SnakeRecord{flags:flags::BOOSTING,..snake()};let mut out=[ShaderRenderVertex::default();4096];
    for _ in 0..16 {
        i.tick+=1;i.simulation_time+=1.0/30.0;p.presentation_time=i.simulation_time;
        for segment in &mut b {segment.previous_x=segment.x;segment.x+=2.0;}
        r.build_shader(&i,&[s],&b,&[],&[],&palette(),&p,&mut out);
    }
    let n=r.build_shader(&i,&[s],&b,&[],&[],&palette(),&p,&mut out).vertex_count;
    let trail:Vec<_>=out[..n].iter().copied().filter(|v|v.params[0]==10).collect();
    assert_eq!(trail.len(),84);
    assert!(trail.iter().all(|v|v.along==0.0));
    assert!(trail.iter().all(|v|v.across.abs()==1.0)); // compact shader support
    for (j,edge) in trail.chunks_exact(6).enumerate() {
        let half_width=|a:ShaderRenderVertex,b:ShaderRenderVertex|
            ((a.x-b.x).powi(2)+(a.y-b.y).powi(2)).sqrt()*0.5;
        assert!((half_width(edge[0],edge[1])-0.465*8.0*(j+1) as f32/15.0).abs()<0.0001);
        assert!((half_width(edge[2],edge[5])-0.465*8.0*(j+2) as f32/15.0).abs()<0.0001);
    }
    for edge in trail.chunks_exact(6).collect::<Vec<_>>().windows(2) {
        assert_eq!(edge[0][2],edge[1][0]);assert_eq!(edge[0][5],edge[1][1]);
    }
    assert!(trail[0].color.alpha<trail.last().unwrap().color.alpha);
    assert_eq!(trail.last().unwrap().color.alpha,89);
}

#[test]
fn shader_food_uses_exported_sizes_and_full_vacuum_streak() {
    let mut out=[ShaderRenderVertex::default();1024];
    for kind in 0..=4 {
        let f=FoodRecord{x:200.0,y:200.0,size:4.0,kind,life_fraction:255,..Default::default()};
        let n=RenderHandle::new().build_shader(&info(),&[],&[],&[f],&[],&palette(),&params(),&mut out).vertex_count;
        let visual_scale=if matches!(kind,3|4) {1.6} else {1.0};
        assert_eq!(n,6);assert!((out[5].x-out[0].x-2.0*4.6*4.0*visual_scale).abs()<1.0e-4);
    }
    let f=FoodRecord{x:200.0,y:200.0,size:4.0,attraction:0.8,attraction_x:300.0,attraction_y:200.0,life_fraction:255,..Default::default()};
    let n=RenderHandle::new().build_shader(&info(),&[],&[],&[f],&[],&palette(),&params(),&mut out).vertex_count;
    assert_eq!(n,12);assert_eq!(out[0].params[0],5);assert_eq!(out[0].color.alpha,191);
    assert!(((out[0].y-out[1].y).abs()-1.3*4.0).abs()<1.0e-4);
    assert!((out[0].x-out[2].x-4.0*(3.0+8.0*0.8)).abs()<1.0e-4);
}

#[test]
fn shader_tight_coil_envelope_stays_inside_curvature_without_triangle_fold() {
    let radius=14.4_f32;
    let b:Vec<_>=(0..24).map(|j|{
        let angle=j as f32*0.6;
        let x=200.0+radius*angle.cos();let y=200.0+radius*angle.sin();
        SegmentRecord{x,y,previous_x:x,previous_y:y}
    }).collect();
    let s=SnakeRecord{segment_count:24,..snake()};let mut out=[ShaderRenderVertex::default();1024];
    let n=RenderHandle::new().build_shader(&info(),&[s],&b,&[],&[],&palette(),&params(),&mut out).vertex_count;
    let edge=&out[5*6..6*6];
    for v in edge {
        let center=b[if v.along==18.0 {5} else {6}];
        let extrusion=((v.x-center.x).powi(2)+(v.y-center.y).powi(2)).sqrt();
        assert!((extrusion-radius*0.95).abs()<1.0e-4);
        assert!(v.across.abs()<v.color.alpha as f32/255.0);
    }
    assert_eq!(n,24*6);
    let area=|a:ShaderRenderVertex,b:ShaderRenderVertex,c:ShaderRenderVertex|(b.x-a.x)*(c.y-a.y)-(b.y-a.y)*(c.x-a.x);
    // Exclude the uncurved endpoint normals; interior edge triangles agree.
    for edge in out[6..(24-2)*6].chunks_exact(6) {
        assert!(area(edge[0],edge[1],edge[2])*area(edge[3],edge[4],edge[5])>0.0);
    }
}


#[test]
fn shader_all_primitive_wrap_bounds_match_independent_tiling() {
    // Count by primitive kind against wall-bounded draws shifted independently
    // across a generous tile grid. This oracle never calls wrap-copy selection.
    let counts = |vertices: &[ShaderRenderVertex]| {
        let mut result = [0usize; 11];
        for v in vertices { result[v.params[0] as usize] += 1; }
        result
    };
    for (width, height, x, y, angle) in [
        (640.0, 360.0, 590.0, 180.0, 0.0),
        (640.0, 360.0, 50.0, 180.0, std::f64::consts::PI),
        (640.0, 360.0, 320.0, 310.0, std::f64::consts::FRAC_PI_2),
        (640.0, 360.0, 320.0, 50.0, -std::f64::consts::FRAC_PI_2),
        (100.0, 100.0, 90.0, 90.0, std::f64::consts::FRAC_PI_4),
        (10.0, 10.0, 5.0, 5.0, std::f64::consts::FRAC_PI_4),
    ] {
        for (sx, sy) in [(1.0, 1.0), (2.0, 0.5), (0.5, 2.0)] {
            let mut i = info(); i.world_width = width; i.world_height = height;
            let mut p = params(); p.deadly_walls = 0; p.developer_mode = 1;
            p.scale_x = sx; p.scale_y = sy; p.interpolation = 1.0;
            p.viewport_width = width * sx; p.viewport_height = height * sy;
            let mut s = SnakeRecord { flags: flags::BOOSTING | flags::LEADER,
                angle, desired_angle: angle, ..snake() };
            let b = std::array::from_fn::<_, 3, _>(|j| SegmentRecord {
                x: x as f32 - j as f32, y: y as f32,
                previous_x: x as f32 - j as f32, previous_y: y as f32 });
            let food = std::array::from_fn::<_, 5, _>(|kind| FoodRecord {
                x: x as f32, y: y as f32, size: 4.0, kind: kind as u8,
                attraction: 1.0, attraction_x: x as f32 + 1.0,
                attraction_y: y as f32 + 1.0, life_fraction: 255, ..Default::default() });
            let events = [EventRecord { tick: i.tick, x: x as f32, y: y as f32,
                kind: 0, ..Default::default() }, EventRecord { tick: i.tick,
                x: x as f32, y: y as f32, kind: 4, ..Default::default() }];
            let mut wrapped = RenderHandle::new(); let mut tiled = RenderHandle::new();
            let mut out = vec![ShaderRenderVertex::default(); 100000];
            let mut history = p; history.scale_x = 0.0; history.scale_y = 0.0;
            for renderer in [&mut wrapped, &mut tiled] {
                renderer.build_shader(&i, &[s], &b, &food, &events, &palette(), &history, &mut []);
            }
            i.tick += 1; i.simulation_time += 1.0 / 30.0; p.presentation_time = i.simulation_time;
            let mut current = b; current[2].y += 1.0; current[2].previous_y += 1.0;
            for corpse in [false, true] {
                if corpse { s.flags = flags::CORPSE; s.alive = 0;
                    i.tick += 1; i.simulation_time += 1.0 / 30.0;
                    p.presentation_time = i.simulation_time + 0.2; }
                let n = wrapped.build_shader(&i, &[s], &current, &food, &[], &palette(), &p, &mut out).vertex_count;
                assert!(n <= out.len()); let actual = counts(&out[..n]);
                let mut expected = [0usize; 11]; let mut tile = p; tile.deadly_walls = 1;
                for tx in -16..=16 { for ty in -16..=16 {
                    tile.offset_x = tx as f64 * width * sx;
                    tile.offset_y = ty as f64 * height * sy;
                    let n = tiled.build_shader(&i, &[s], &current, &food, &[], &palette(), &tile, &mut out).vertex_count;
                    assert!(n <= out.len());
                    for (total, count) in expected.iter_mut().zip(counts(&out[..n])) { *total += count; }
                }}
                // Wrapped copies share the eight-visible-quad budget; newer
                // boost and succession rings have priority over the impact.
                let mut remaining=8*6;
                for kind in [9,7,6] {
                    expected[kind]=expected[kind].min(remaining);
                    remaining-=expected[kind];
                }
                assert_eq!(actual, expected,
                    "arena={width}x{height}, head={x},{y}, angle={angle}, scale={sx},{sy}, corpse={corpse}");
            }
        }
    }
}


#[test]
fn shader_multi_arena_body_bounds_match_independent_tiling() {
    for (dx, dy) in [(7.0, 0.0), (0.0, 7.0), (7.0, 7.0), (-7.0, 0.0)] {
        for (sx, sy) in [(1.0, 1.0), (2.0, 0.5), (0.5, 2.0)] {
            let mut i = info(); i.world_width = 100.0; i.world_height = 100.0;
            let mut p = params(); p.deadly_walls = 0; p.interpolation = 1.0;
            p.scale_x = sx; p.scale_y = sy;
            p.viewport_width = 100.0 * sx; p.viewport_height = 100.0 * sy;
            let mut s = SnakeRecord { radius: 6.0, segment_count: 40, ..snake() };
            let unwrapped: Vec<_> = (0..40).map(|j| {
                let x = (50.0 + dx * j as f64) as f32;
                let y = (50.0 + dy * j as f64) as f32;
                SegmentRecord { x, y, previous_x: x, previous_y: y }
            }).collect();
            let b: Vec<_> = unwrapped.iter().map(|seg| {
                let x = seg.x.rem_euclid(100.0); let y = seg.y.rem_euclid(100.0);
                SegmentRecord { x, y, previous_x: x, previous_y: y }
            }).collect();
            let mut wrapped = RenderHandle::new(); let mut tiled = RenderHandle::new();
            let mut out = vec![ShaderRenderVertex::default(); 10000];
            for corpse in [false, true] {
                if corpse { i.tick += 1; i.simulation_time += 1.0 / 30.0;
                    s.alive = 0; s.flags = flags::CORPSE;
                    p.presentation_time = i.simulation_time + 0.2; }
                let n = wrapped.build_shader(&i, &[s], &b, &[], &[], &palette(), &p, &mut out).vertex_count;
                assert!(n <= out.len());
                let actual = out[..n].iter().filter(|v| v.params[0] == 0).count();
                let mut expected = 0; let mut tile = p; tile.deadly_walls = 1;
                for tx in -5..=5 { for ty in -5..=5 {
                    tile.offset_x = tx as f64 * 100.0 * sx;
                    tile.offset_y = ty as f64 * 100.0 * sy;
                    let n = tiled.build_shader(&i, &[s], &unwrapped, &[], &[], &palette(), &tile, &mut out).vertex_count;
                    assert!(n <= out.len());
                    expected += out[..n].iter().filter(|v| v.params[0] == 0).count();
                }}
                assert_eq!(actual, expected, "delta={dx},{dy}, scale={sx},{sy}, corpse={corpse}");
            }
        }
    }
}

#[test]
fn shader_fragment_extents_fit_emitted_body_head_and_food_quads() {
    let mut out = vec![ShaderRenderVertex::default(); 4096];
    let b: Vec<_> = (0..24).map(|j| SegmentRecord {
        x: 400.0-j as f32*9.44, y: 200.0,
        previous_x: 400.0-j as f32*9.44, previous_y: 200.0 }).collect();
    for flags in [0, flags::HUNTING, flags::LEADER, flags::TRAPPED,
                  flags::BOOSTING | flags::LEADER | flags::HUNTING] {
        let s = SnakeRecord { flags, segment_count: 24, ..snake() };
        let n = RenderHandle::new().build_shader(&info(), &[s], &b, &[], &[], &palette(), &params(), &mut out).vertex_count;
        assert_eq!(n, 24*6+if flags & flags::BOOSTING != 0 {6} else {0});
        for v in out[..n].iter().filter(|v| v.params[0] == 0) {
            let wave_support = 2.4 * 1.028 * v.color.alpha as f64/255.0;
            assert!((v.y as f64-200.0).abs()/8.0+1.0e-5 >= wave_support);
        }
        let h:Vec<_> = out[..n].iter().filter(|v|v.params[0]==1).collect();
        let hr = 8.0*1.14;
        // UV and world extents must agree: expanding vertices alone stretches
        // artwork, and expanding UV alone shrinks its intended physical size.
        for v in &h {
            assert!((v.x as f64-400.0-v.across as f64*hr).abs()<0.00005);
            assert!((v.y as f64-200.0-v.along as f64*hr).abs()<0.00005);
        }
        assert!(h[1].across as f64 >= 1.30+1.32+0.045+0.2); // tongue
        assert!(h[5].along as f64 >= 0.56+1.15); // eye light
        if flags & flags::BOOSTING != 0 {
            let rear_bow = 0.286+2.352*2.4-1.238*2.4*2.4
                -(0.08+0.2)*(1.0_f64+(2.352_f64-2.476*2.4).powi(2)).sqrt();
            assert!(h[0].across as f64 <= rear_bow);
            assert!(h[5].along >= 2.4);
        }
    }
    for (kind, support) in [(0,4.6_f64),(1,4.4),(2,3.0),(3,4.0)] {
        let f = FoodRecord { x:400.0,y:200.0,size:4.0,kind,life_fraction:255,..Default::default() };
        let n = RenderHandle::new().build_shader(&info(), &[], &[], &[f], &[], &palette(), &params(), &mut out).vertex_count;
        assert_eq!(n, 6);
        assert!((out[0].x as f64-400.0).abs()/4.0+0.00001 >= support);
        assert!((out[0].y as f64-200.0).abs()/4.0+0.00001 >= support);
    }
}

#[test]
fn shader_effect_extents_fit_emitted_quads_through_entire_lifetime() {
    let i = info(); let mut out = [ShaderRenderVertex::default(); 1024];
    let events = [EventRecord {tick:i.tick,x:400.0,y:200.0,kind:0,..Default::default()},
                  EventRecord {tick:i.tick,x:400.0,y:200.0,kind:4,..Default::default()}];
    let s = SnakeRecord {flags:flags::BOOSTING,..snake()}; let b = body();
    for kind in [6,7,9] {
        let mut renderer = RenderHandle::new();
        renderer.build_shader(&i, &[s], &b, &[], &events, &palette(), &params(), &mut out);
        let duration = if kind==6 {0.5} else if kind==7 {0.6} else {0.28};
        for byte in 0..=255 {
            let p = RenderParams {presentation_time:i.simulation_time+duration*(byte as f64/255.0).min(0.9999),..params()};
            let n = renderer.build_shader(&i, &[s], &b, &[], &[], &palette(), &p, &mut out).vertex_count;
            let quad:Vec<_> = out[..n].iter().filter(|v|v.params[0]==kind).collect();
            assert_eq!(quad.len(),6);
            let age = quad[0].params[3] as f64/255.0;
            let progress = 2.0*age-age*age;
            let mut support = 1.0+(if kind==9 {2.2} else {5.0})*progress+7.0*(0.012+0.04);
            if kind==6 {
                support = support.max(1.2+8.4*progress+2.2*(1.0-age)+7.0*(0.012+0.04)).max(7.0);
            }
            for v in &quad { assert!(v.across.abs() as f64 >= support && v.along.abs() as f64 >= support); }
            let radius_per_uv = (quad[1].x-quad[0].x)/(quad[1].across-quad[0].across);
            assert!((radius_per_uv-8.0).abs()<0.00001);
        }
    }
}

#[test]
fn shader_new_outer_support_selects_edge_and_corner_copies() {
    // 75px from the seam: old 56px impact radius missed these copies;
    // the current 81.6px radius includes them at all four corners.
    let mut i = info(); i.world_width=640.0;i.world_height=360.0;
    let p = RenderParams {viewport_width:640.0,viewport_height:360.0,deadly_walls:0,..params()};
    let mut out=[ShaderRenderVertex::default();1024];
    for (x,y,expected) in [(565.0,180.0,12),(75.0,180.0,12),
                          (320.0,285.0,12),(320.0,75.0,12),
                          (565.0,285.0,24),(75.0,75.0,24)] {
        let e=EventRecord {tick:i.tick,x,y,kind:0,..Default::default()};
        let n=RenderHandle::new().build_shader(&i,&[],&[],&[],&[e],&palette(),&p,&mut out).vertex_count;
        assert_eq!(n,expected,"impact at {x},{y}");
    }
}

#[test]
fn shader_compact_streak_support_and_minimum_pixel_width_fit_emitted_bounds() {
    let mut out=[ShaderRenderVertex::default();1024];
    let p=RenderParams {developer_mode:1,..params()};
    let n=RenderHandle::new().build_shader(&info(),&[snake()],&body(),&[],&[],&palette(),&p,&mut out).vertex_count;
    let overlay:Vec<_>=out[..n].iter().filter(|v|v.params[0]==5).collect();
    assert_eq!(overlay.len(),6);
    assert!((overlay[0].y-overlay[1].y-2.4).abs()<0.0001);
    assert!((overlay[2].x-overlay[0].x-56.0).abs()<0.0001);
    for v in &overlay { assert_eq!(v.across.abs(),1.0); assert!((0.0..=1.0).contains(&v.along)); }
    // The vacuum's .5px minimum is wider than this food halo. Only the
    // streak crosses the seam: selecting copies with halo bounds loses it.
    let mut i=info();i.world_width=10.0;i.world_height=10.0;
    let p=RenderParams {viewport_width:10.0,viewport_height:10.0,deadly_walls:0,..params()};
    let f=FoodRecord {x:5.0,y:0.49,size:0.1,attraction:0.8,
        attraction_x:6.0,attraction_y:0.49,life_fraction:255,..Default::default()};
    let n=RenderHandle::new().build_shader(&i,&[],&[],&[f],&[],&palette(),&p,&mut out).vertex_count;
    let streaks:Vec<_>=out[..n].iter().filter(|v|v.params[0]==5).collect();
    assert_eq!(streaks.len(),12); assert_eq!(n,18);
    for q in streaks.chunks_exact(6) {
        assert!(((q[0].y-q[1].y).abs()-1.0).abs()<0.0001);
        assert!((q[0].x-q[2].x-0.94).abs()<0.0001);
        for v in q { assert_eq!(v.across.abs(),1.0); assert!((0.0..=1.0).contains(&v.along)); }
    }
}

#[test]
fn items_are_six_vertex_sprites_and_allocation_free_with_bounded_effects() {
    let mut renderer=RenderHandle::new();let info=info();let p=params();let pal=palette();
    let items:[ItemRecord;3]=std::array::from_fn(|i|ItemRecord {id:i as u64+1,x:200.0+i as f32*100.0,y:200.0,
        kind:i as u8+1,age_ticks:0,life_ticks:750,..Default::default()});
    let mut out=[ShaderRenderVertex::default();256];
    COUNT.with(|c|c.set(Some(0)));
    unsafe {assert_eq!(snakes_core_render_set_items(&mut renderer,items.as_ptr(),items.len(),12.0),OK);}
    let n=renderer.build_shader(&info,&[],&[],&[],&[],&pal,&p,&mut out).vertex_count;
    let allocations=COUNT.with(|c|c.replace(None).unwrap());assert_eq!(allocations,0);
    assert_eq!(n,18);assert!(out[..n].iter().all(|v|v.params[0]==11));
    let before=out;
    let invalid=ItemRecord{x:f32::NAN,..items[0]};
    unsafe {assert_eq!(snakes_core_render_set_items(&mut renderer,&invalid,1,12.0),INVALID_ARGUMENT);}
    assert_eq!(renderer.build_shader(&info,&[],&[],&[],&[],&pal,&p,&mut out).vertex_count,n);
    assert_eq!(out,before);
    let events:[EventRecord;32]=std::array::from_fn(|i|EventRecord {tick:info.tick+1,x:200.0,y:200.0,
        snake_id:0,other_snake_id:i as u32%3+1,kind:if i%2==0 {2} else {6},..Default::default()});
    let next=FrameInfo{tick:info.tick+1,simulation_time:info.simulation_time+1.0/30.0,..info};
    let p=RenderParams{presentation_time:next.simulation_time,..p};
    let n=renderer.build_shader(&next,&[],&[],&[],&events,&pal,&p,&mut out).vertex_count;
    assert_eq!(out[..n].iter().filter(|v|v.params[0]>=12).count(),48);
    assert_eq!(n,18+48);
    let mut classic=[RenderVertex::default();4096];
    assert!(renderer.build(&next,&[],&[],&[],&events,&pal,&p,&mut classic).vertex_count>0);
}

#[test]
fn active_body_effects_preserve_vertex_budget_and_expiry_clears_payload() {
    let mut renderer=RenderHandle::new();let i=info();let p=params();let pal=palette();
    let mut s=snake();let b=body();let mut out=[ShaderRenderVertex::default();256];
    let baseline=renderer.build_shader(&i,&[s],&b,&[],&[],&pal,&p,&mut out).vertex_count;
    for kind in 1..=3 {
        s.effect_kind=kind;s.effect_ticks=90;
        let n=renderer.build_shader(&i,&[s],&b,&[],&[],&pal,&p,&mut out).vertex_count;
        assert_eq!(n,baseline+if kind==2 {6} else if kind==1 {18} else {0});
        for v in &out[..n] {
            if v.params[0]==0 {assert_eq!((v.params[1]>>2)&7,kind);}
            if v.params[0]==1 {assert_eq!(v.params[2]&flags::PHASED as u8,if kind==3 {32} else {0});}
        }
        s.effect_ticks=0;
        let n=renderer.build_shader(&i,&[s],&b,&[],&[],&pal,&p,&mut out).vertex_count;
        assert_eq!(n,baseline);
        assert!(out[..n].iter().filter(|v|v.params[0]==0).all(|v|v.params[1]&28==0));
        assert!(out[..n].iter().filter(|v|v.params[0]==1).all(|v|v.params[2]&32==0));
    }
    s.effect_kind=3;s.effect_ticks=90;s.alive=0;s.flags=flags::CORPSE|flags::PHASED;
    let n=renderer.build_shader(&i,&[s],&b,&[],&[],&pal,&p,&mut out).vertex_count;
    assert!(out[..n].iter().all(|v|v.params[0]==0 && v.params[1]&28==0));
}

#[test]
fn active_effect_wrap_bounds_match_independent_tiling_and_share_quad_cap() {
    let counts=|v:&[ShaderRenderVertex]| {
        let mut counts=[0usize;32];for v in v {counts[v.params[0] as usize]+=1;}counts
    };
    for (width,height,x,y) in [(640.0,360.0,630.0,350.0),(100.0,100.0,90.0,90.0),(10.0,10.0,5.0,5.0)] {
        for (sx,sy) in [(1.0,1.0),(2.0,0.5),(0.5,2.0)] {
            let i=FrameInfo{world_width:width,world_height:height,..info()};
            let p=RenderParams{viewport_width:width*sx,viewport_height:height*sy,
                scale_x:sx,scale_y:sy,interpolation:1.0,deadly_walls:0,..params()};
            let b=std::array::from_fn::<_,3,_>(|j|SegmentRecord{x:x as f32-j as f32,y:y as f32,
                previous_x:x as f32-j as f32,previous_y:y as f32});
            let mut renderer=RenderHandle::new();let mut tiled=RenderHandle::new();
            let mut out=vec![ShaderRenderVertex::default();20000];
            for kind in 1..=3 {
                let s=SnakeRecord{effect_kind:kind,effect_ticks:24,..snake()};
                let n=renderer.build_shader(&i,&[s],&b,&[],&[],&palette(),&p,&mut out).vertex_count;
                assert!(n<=out.len());let actual=counts(&out[..n]);
                let mut expected=[0usize;32];let mut tile=RenderParams{deadly_walls:1,..p};
                for tx in -16..=16 {for ty in -16..=16 {
                    tile.offset_x=tx as f64*width*sx;tile.offset_y=ty as f64*height*sy;
                    let n=tiled.build_shader(&i,&[s],&b,&[],&[],&palette(),&tile,&mut out).vertex_count;
                    assert!(n<=out.len());
                    for (total,count) in expected.iter_mut().zip(counts(&out[..n])) {*total+=count;}
                }}
                expected[27]=expected[27].min(84);
                expected[14]=expected[14].min(48);expected[15]=expected[15].min(48-expected[14]);
                assert_eq!(actual,expected,"kind={kind}, arena={width}x{height}, scale={sx},{sy}");
            }
        }
    }
}

#[test]
fn active_effects_are_allocation_free_and_classic_freezes_motion() {
    let mut renderer=RenderHandle::new();let pal=palette();let mut i=info();let mut p=params();
    let mut s=SnakeRecord{effect_ticks:24,..snake()};let b=body();
    let mut shader=[ShaderRenderVertex::default();256];let mut classic=[RenderVertex::default();4096];
    COUNT.with(|c|c.set(Some(0)));
    for tick in 0..1000 {
        i.tick+=1;i.simulation_time+=1.0/30.0;p.presentation_time=i.simulation_time;
        s.effect_kind=1+(tick%3) as u8;
        let n=renderer.build_shader(&i,&[s],&b,&[],&[],&pal,&p,&mut shader).vertex_count;
        assert!(n<=shader.len());assert!(shader[..n].iter().filter(|v|v.params[0]>=12).count()<=48);
        assert!(renderer.build(&i,&[s],&b,&[],&[],&pal,&p,&mut classic).vertex_count<=classic.len());
    }
    assert_eq!(COUNT.with(|c|c.replace(None).unwrap()),0);
    unsafe {assert_eq!(snakes_core_render_set_reduced_motion(&mut renderer,1),OK);}
    for kind in 1..=3 {
        s.effect_kind=kind;s.effect_ticks=90;
        let n=renderer.build(&i,&[s],&b,&[],&[],&pal,&p,&mut classic).vertex_count;
        let before=classic;
        p.presentation_time+=0.1;
        let next=renderer.build(&i,&[s],&b,&[],&[],&pal,&p,&mut classic).vertex_count;
        assert_eq!(n,next);assert_eq!(&before[..n],&classic[..n]);
    }
}

#[test]
fn classic_stroked_capsules_and_transients_match_independent_edge_corner_tiling() {
    // Wall-bounded draws on independently shifted tiles are the oracle. Tiny
    // capsules exercise minimum-pixel strokes; rings exercise the outer stroke.
    for (sx,sy) in [(1.0_f64,1.0_f64),(0.01,0.02),(0.02,0.01)] {
        let scale: f64=(sx*sy).sqrt();
        let i=FrameInfo{world_width:1000.0,world_height:1000.0,..info()};
        let p=RenderParams{scale_x:sx,scale_y:sy,viewport_width:1000.0*sx,
            viewport_height:1000.0*sy,deadly_walls:0,interpolation:1.0,..params()};
        for fixture in 0..4 {
            for corner in [false,true] {
                let radius=if fixture==0 {4.0} else {8.0};
                let screen_x=if fixture==0 {0.45} else {
                    // Between the centreline and its stroked edge.
                    radius*scale*if fixture==1 {1.0} else {2.5}+0.2*(radius*scale*0.1).max(0.5)
                };
                let x=(screen_x/sx) as f32;
                let y=if corner {(screen_x/sy) as f32} else {500.0};
                let item=ItemRecord{id:1,kind:2,x,y,age_ticks:0,life_ticks:750,..Default::default()};
                let event=EventRecord{tick:i.tick,snake_id:u32::MAX,other_snake_id:2,x,y,
                    kind:if fixture==1 {2} else if fixture==2 {6} else {7},..Default::default()};
                let events=if fixture==0 {&[][..]} else {std::slice::from_ref(&event)};
                let mut wrapped=RenderHandle::new();let mut tiled=RenderHandle::new();
                if fixture==0 {for r in [&mut wrapped,&mut tiled] {
                    unsafe {assert_eq!(snakes_core_render_set_items(r,&item,1,radius),OK);}
                }}
                let mut out=vec![RenderVertex::default();20000];
                let actual=wrapped.build(&i,&[],&[],&[],events,&palette(),&p,&mut out).vertex_count;
                let mut expected=0;
                for tx in -3..=3 {for ty in -3..=3 {
                    let tile=RenderParams{deadly_walls:1,offset_x:tx as f64*i.world_width*sx,
                        offset_y:ty as f64*i.world_height*sy,..p};
                    expected+=tiled.build(&i,&[],&[],&[],events,&palette(),&tile,&mut out).vertex_count;
                }}
                assert!(actual>0);
                assert_eq!(actual,expected,"fixture={fixture}, corner={corner}, scale={sx},{sy}");
            }
        }
    }
}

#[test]
fn offscreen_transients_do_not_consume_either_paths_warning_and_magnet_budget() {
    let i=info();let p=RenderParams{viewport_width:400.0,viewport_height:400.0,..params()};
    let s=SnakeRecord{effect_kind:2,effect_ticks:24,..snake()};let b=body();
    for kind in [0,2,6,7] {
        let events:[EventRecord;8]=std::array::from_fn(|_|EventRecord{tick:i.tick,x:1000.0,y:1000.0,
            kind,snake_id:u32::MAX,other_snake_id:u32::MAX,..Default::default()});
        let mut a=RenderHandle::new();let mut z=RenderHandle::new();
        let mut classic=[RenderVertex::default();4096];let mut expected=classic;
        let n=a.build(&i,&[s],&b,&[],&events,&palette(),&p,&mut classic).vertex_count;
        let m=z.build(&i,&[s],&b,&[],&[],&palette(),&p,&mut expected).vertex_count;
        assert_eq!(&classic[..n],&expected[..m],"classic kind={kind}");
        let mut shader=[ShaderRenderVertex::default();256];let mut expected=shader;
        let n=a.build_shader(&i,&[s],&b,&[],&events,&palette(),&p,&mut shader).vertex_count;
        let m=z.build_shader(&i,&[s],&b,&[],&[],&palette(),&p,&mut expected).vertex_count;
        assert_eq!(&shader[..n],&expected[..m],"shader kind={kind}");
        assert_eq!(shader[..n].iter().filter(|v|matches!(v.params[0],14|15)).count(),12);
    }
}

#[test]
fn wrapped_transients_share_eight_visible_copies_newest_first_and_retry_stably() {
    // Three corner events would cost twelve copies. At most the two newest
    // fit, leaving no budget for the live warning/Magnet on either path.
    let i=FrameInfo{world_width:300.0,world_height:300.0,..info()};
    let p=RenderParams{viewport_width:300.0,viewport_height:300.0,deadly_walls:0,
        presentation_time:20.05,..params()};
    let s=SnakeRecord{effect_kind:2,effect_ticks:24,..snake()};let b=body();
    for mixed in [false,true] {
        let events:[EventRecord;3]=std::array::from_fn(|j|EventRecord{tick:i.tick,x:0.0,y:0.0,
            kind:if mixed {match j {1=>0,2=>2,_=>6}} else {6},
            snake_id:u32::MAX,other_snake_id:if mixed && j==1 {u32::MAX} else {j as u32+1},..Default::default()});
        for shader in [false,true] {
            let mut r=RenderHandle::new();let mut reference=RenderHandle::new();
            let newest=[events[1],events[2]];
            if shader {
                let mut out=[ShaderRenderVertex::default();512];let mut expected=out;
                let n=r.build_shader(&i,&[s],&b,&[],&events,&palette(),&p,&mut out).vertex_count;
                let m=reference.build_shader(&i,&[s],&b,&[],&newest,&palette(),&p,&mut expected).vertex_count;
                assert_eq!(&out[..n],&expected[..m]);
                let copies:Vec<_>=out[..n].chunks_exact(6).filter(|q|matches!(q[0].params[0],6|7|9|12..=15)).collect();
                assert_eq!(copies.len(),8);
                assert!(copies[..4].iter().all(|q|q[0].color.blue==255));
                if mixed {
                    assert!(copies[..4].iter().all(|q|q[0].params[0]==12));
                    assert!(copies[4..].iter().all(|q|q[0].params[0]==6));
                } else {assert!(copies[4..].iter().all(|q|q[0].color.green==95));}
                let mut short=[ShaderRenderVertex::default();1];
                assert_eq!(r.build_shader(&i,&[s],&b,&[],&events,&palette(),&p,&mut short).vertex_count,n);
                assert_eq!(short[0],out[0]);
            } else {
                let mut out=[RenderVertex::default();4096];let mut expected=out;
                let n=r.build(&i,&[s],&b,&[],&events,&palette(),&p,&mut out).vertex_count;
                let m=reference.build(&i,&[s],&b,&[],&newest,&palette(),&p,&mut expected).vertex_count;
                assert_eq!(&out[..n],&expected[..m]);
                let mut short=[RenderVertex::default();1];
                assert_eq!(r.build(&i,&[s],&b,&[],&events,&palette(),&p,&mut short).vertex_count,n);
                assert_eq!(short[0],out[0]);
            }
        }
    }
}

#[test]
fn replacement_pickup_waves_retain_each_origin_after_replacement_and_expiry() {
    for walls in [0,1] {
        for reduced in [0,1] {
            for flags in [0,flags::BOOSTING,flags::LEADER] {
                let mut r=RenderHandle::new();
                unsafe {assert_eq!(snakes_core_render_set_reduced_motion(&mut r,reduced),OK);}
                let mut i=info();let mut p=RenderParams{deadly_walls:walls,interpolation:1.0,..params()};
                let mut s=SnakeRecord{flags,segment_count:40,effect_kind:1,effect_ticks:90,..snake()};
                let b:Vec<_>=(0..40).map(|j|SegmentRecord{x:700.0-j as f32*8.0,y:300.0,
                    previous_x:700.0-j as f32*8.0,previous_y:300.0}).collect();
                let mut out=[ShaderRenderVertex::default();512];
                let event=EventRecord{tick:i.tick,kind:2,snake_id:0,other_snake_id:1,x:700.0,y:300.0,..Default::default()};
                COUNT.with(|c|c.set(Some(0)));
                r.build_shader(&i,&[s],&b,&[],&[event],&palette(),&p,&mut out);
                i.tick+=6;i.simulation_time+=0.2;p.presentation_time=i.simulation_time;
                s.effect_kind=2;
                let next=EventRecord{tick:i.tick,other_snake_id:2,..event};
                r.build_shader(&i,&[s],&b,&[],&[next],&palette(),&p,&mut out);
                for ticks in [90,0] {
                    s.effect_ticks=ticks;p.presentation_time=i.simulation_time+0.03;
                    let n=r.build_shader(&i,&[s],&b,&[],&[],&palette(),&p,&mut out).vertex_count;
                    let mut origins=[0usize;8];
                    for v in &out[..n] {if v.params[0]==0 && v.params[3]>0 {
                        let origin=((v.params[2]>>1)&1)|((v.params[2]>>3)&6);
                        origins[origin as usize]+=1;
                        assert_eq!((v.params[1]>>2)&7,if ticks>0 {2} else {0});
                    }}
                    assert!(origins[1]>0 && origins[2]>0,"walls={walls}, reduced={reduced}, ticks={ticks}, {origins:?}");
                    assert_eq!(origins[0],0);
                }
                // Classic also retains both pickup origins, choosing the strongest
                // at each edge rather than replacing the whole wave with the newest.
                let mut classic=[RenderVertex::default();4096];
                let n=r.build(&i,&[s],&b,&[],&[],&palette(),&p,&mut classic).vertex_count;
                assert!(classic[..n].iter().any(|v|v.color.red>180 && v.color.green>200 && v.color.blue<180));
                assert!(classic[..n].iter().any(|v|v.color.red>180 && v.color.green<160 && v.color.blue>180));
                assert_eq!(COUNT.with(|c|c.replace(None).unwrap()),0);
            }
        }
    }
}

#[test]
fn shader_capsules_of_every_kind_and_age_match_independent_tiling() {
    for (sx,sy) in [(1.0,1.0),(0.01,0.02),(2.0,0.5)] {
        let i=FrameInfo{world_width:300.0,world_height:200.0,..info()};
        let p=RenderParams{scale_x:sx,scale_y:sy,viewport_width:300.0*sx,
            viewport_height:200.0*sy,deadly_walls:0,..params()};
        let mut out=[ShaderRenderVertex::default();4096];
        for kind in 1..=5 {for age_ticks in [0,7,15] {for corner in [false,true] {
            let item=ItemRecord{id:1,kind,age_ticks,x:1.0,y:if corner {1.0} else {100.0},life_ticks:750,..Default::default()};
            let mut r=RenderHandle::new();let mut tiled=RenderHandle::new();
            for r in [&mut r,&mut tiled] {
                unsafe {assert_eq!(snakes_core_render_set_items(r,&item,1,12.0),OK);}
            }
            let n=r.build_shader(&i,&[],&[],&[],&[],&palette(),&p,&mut out).vertex_count;
            assert!(out[..n].iter().all(|v|v.params[0]==11));
            let mut expected=0;
            for x in -4..=4 {for y in -4..=4 {
                let tile=RenderParams{deadly_walls:1,offset_x:x as f64*i.world_width*sx,
                    offset_y:y as f64*i.world_height*sy,..p};
                expected+=tiled.build_shader(&i,&[],&[],&[],&[],&palette(),&tile,&mut out).vertex_count;
            }}
            assert_eq!(n,expected,"kind={kind}, age={age_ticks}, corner={corner}, scale={sx},{sy}");
        }}}
    }
}

fn s1_fixture()->(FrameInfo,[SnakeRecord;6],[SegmentRecord;18],[ItemRecord;3]) {
    let mut i=info();i.bubble_count=3;
    i.bubbles=std::array::from_fn(|j|snakes_core::Bubble{snake_id:j as u32,generation:1,age_ticks:12,glyph:j as u8,..Default::default()});
    let snakes=std::array::from_fn(|j|SnakeRecord{id:j as u32,segment_offset:j as u32*3,
        mood:(j+1) as u8,mood_intensity:255,target_item:(j/2) as u8,..snake()});
    let segments=std::array::from_fn(|j|SegmentRecord{x:300.0+(j/3) as f32*100.0-(j%3) as f32*8.0,y:300.0,
        previous_x:300.0+(j/3) as f32*100.0-(j%3) as f32*8.0,previous_y:300.0});
    let items=std::array::from_fn(|j|ItemRecord{id:j as u64+1,x:400.0+j as f32*100.0,y:400.0,kind:1+j as u8,
        age_ticks:40,life_ticks:740,contender_count:2,contender_ids:[2*j as u32,2*j as u32+1],
        contender_etas:[1.0,1.17],leader_snake_id:2*j as u32,..Default::default()});
    (i,snakes,segments,items)
}
#[test]
fn s1_head_moods_pack_without_changing_body_or_vertex_budget() {
    let mut r=RenderHandle::new();let mut out=[ShaderRenderVertex::default();256];let mut baseline=out;
    let mut s=snake();s.flags=flags::HUNTING|flags::TRAPPED;
    let n=r.build_shader(&info(),&[s],&body(),&[],&[],&palette(),&params(),&mut baseline).vertex_count;
    for mood in 0..9 {
        s.mood=mood;s.mood_intensity=255;s.pupil_x=0.18;s.pupil_y=-0.35;s.jaw_ticks=20;s.happy_ticks=45;
        let count=r.build_shader(&info(),&[s],&body(),&[],&[],&palette(),&params(),&mut out).vertex_count;
        assert_eq!(n,count);
        assert_eq!(&out[..12],&baseline[..12],"body flags stay unchanged, mood {mood}");
        let head=&out[12..n];
        // Mood zero with old trapped flags is the compatibility path.
        assert!(head.iter().all(|v|(v.params[2]>>1)&15==if mood==0 {6} else {mood}));
        assert!(head.iter().all(|v|(v.params[1]>>2)&15==15));
        assert_eq!(head[0].params[3]&63,6);
        if matches!(mood,1|5) {assert!((head[0].params[3]>>6)|((head[0].params[2]&128)>>5)>0);}
    }
    s.flags=0;s.mood=3;s.mood_intensity=0;
    let n=r.build_shader(&info(),&[s],&body(),&[],&[],&palette(),&params(),&mut out).vertex_count;
    assert_eq!((out[n-1].params[1]>>2)&15,0);
}
#[test]
fn authoritative_calm_ignores_hunt_boost_and_urgent_flags_in_both_renderers() {
    let mut r=RenderHandle::new();let mut shader=[ShaderRenderVertex::default();256];
    let mut s=snake();s.face_flags=FACE_OBSERVED;s.mood_intensity=255;
    let calm=build(&mut r,s,params());
    for flag in [flags::HUNTING,flags::BOOSTING,flags::TRAPPED,flags::FROZEN] {
        s.flags=flag;
        let n=r.build_shader(&info(),&[s],&body(),&[],&[],&palette(),&params(),&mut shader).vertex_count;
        let head:Vec<_>=shader[..n].iter().filter(|v|v.params[0]==1).collect();
        assert!(!head.is_empty());assert!(head.iter().all(|v|(v.params[2]>>1)&15==0));
        // Frozen/boost flags may change body rendering; hunting only changes
        // legacy eyes, so authoritative Calm must be byte-identical here.
        if flag==flags::HUNTING {assert_eq!(build(&mut r,s,params()),calm);}
    }
    // Explicitly absent faces still provide the old flag-driven expression.
    s.face_flags=0;s.flags=flags::HUNTING;
    let n=r.build_shader(&info(),&[s],&body(),&[],&[],&palette(),&params(),&mut shader).vertex_count;
    assert!(shader[..n].iter().filter(|v|v.params[0]==1).all(|v|(v.params[2]>>1)&15==2 && (v.params[1]>>2)&15==15));
    assert_ne!(build(&mut r,s,params()),calm);
}
#[test]
fn authoritative_hunt_onset_ramps_shader_intensity_and_classic_eyes() {
    let mut r=RenderHandle::new();let mut shader=[ShaderRenderVertex::default();256];
    let mut s=snake();s.face_flags=FACE_OBSERVED;s.flags=flags::HUNTING;s.mood_intensity=0;
    let calm=build(&mut r,s,params());s.mood=2;
    assert_eq!(build(&mut r,s,params()),calm,"zero onset blends to Calm");
    let mut last_blue=255;
    for age in 0u16..=6 {
        s.mood_intensity=(age*255/6) as u8;
        let n=r.build_shader(&info(),&[s],&body(),&[],&[],&palette(),&params(),&mut shader).vertex_count;
        assert!(shader[..n].iter().filter(|v|v.params[0]==1).all(|v|(v.params[2]>>1)&15==2 && (v.params[1]>>2)&15==(age*15/6) as u8));
        let vertices=build(&mut r,s,params());
        let eye=vertices.iter().find(|v|v.color.red==255 && v.color.green>=190 && v.color.blue>=80).unwrap().color;
        assert!(eye.blue<=last_blue);assert!(last_blue-eye.blue<=30);last_blue=eye.blue;
    }
    assert_eq!(last_blue,80);
}
#[test]
fn s1_bubble_cap_clock_exclusion_identity_and_calm_life() {
    let (mut i,s,b,items)=s1_fixture();let mut r=RenderHandle::new();r.set_items(&items,12.0);
    let mut out=[ShaderRenderVertex::default();1024];let p=params();
    for clock in [[0.0;4],[280.0,250.0,280.0,90.0],[0.0,0.0,3440.0,1440.0]] {
        r.set_clock_rect(clock);i.bubble_count=u32::MAX;
        let n=r.build_shader(&i,&s,&b,&[],&[],&palette(),&p,&mut out).vertex_count;
        let bubble:Vec<_>=out[..n].iter().filter(|v|v.params[0]==16).collect();
        assert_eq!(bubble.len(),if clock[2]>0.0 {0} else {18});
        for quad in bubble.chunks_exact(6) {
            let (minx,maxx,miny,maxy)=quad.iter().fold((f32::INFINITY,f32::NEG_INFINITY,f32::INFINITY,f32::NEG_INFINITY),|(a,b,c,d),v|(a.min(v.x),b.max(v.x),c.min(v.y),d.max(v.y)));
            assert!(minx>=0.0 && maxx<=3440.0 && miny>=0.0 && maxy<=1440.0);
            assert!(clock[2]==0.0 || maxx<=clock[0] as f32 || minx>=(clock[0]+clock[2]) as f32 || maxy<=clock[1] as f32 || miny>=(clock[1]+clock[3]) as f32);
        }
    }
    r.set_clock_rect([0.0;4]);i.bubbles[0].generation=2;i.bubbles[1]=i.bubbles[2];
    let n=r.build_shader(&i,&s,&b,&[],&[],&palette(),&p,&mut out).vertex_count;
    assert_eq!(out[..n].iter().filter(|v|v.params[0]==16).count(),6);
    i.bubbles[2].age_ticks=28;r.reduced_motion=true;
    let n=r.build_shader(&i,&s,&b,&[],&[],&palette(),&p,&mut out).vertex_count;
    // Duplicate age-12 copy still renders once.
    assert_eq!(out[..n].iter().filter(|v|v.params[0]==16).count(),6);
    i.bubbles[1].age_ticks=28;
    let n=r.build_shader(&i,&s,&b,&[],&[],&palette(),&p,&mut out).vertex_count;
    assert_eq!(out[..n].iter().filter(|v|v.params[0]==16).count(),0);
    unsafe {assert_eq!(snakes_core_render_set_clock_rect(&mut r,f64::NAN,0.0,2.0,3.0),INVALID_ARGUMENT);}
}
#[test]
fn s1_races_landing_and_zero_steady_state_allocations() {
    let (mut i,mut s,b,mut items)=s1_fixture();let mut r=RenderHandle::new();let mut p=params();
    let mut shader=[ShaderRenderVertex::default();4096];let mut classic=[RenderVertex::default();8192];
    r.set_items(&items,12.0);
    let n=r.build_shader(&i,&s,&b,&[],&[],&palette(),&p,&mut shader).vertex_count;
    assert_eq!(shader[..n].iter().filter(|v|v.params[0]==16).count(),18);
    assert_eq!(shader[..n].iter().filter(|v|v.params[0]==17).count(),36);
    for v in shader[..n].iter().filter(|v|v.params[0]==17) {assert_eq!(v.params[3]&2,2);}
    COUNT.with(|c|c.set(Some(0)));
    for t in 0..1000 {
        i.tick+=1;i.simulation_time+=1.0/30.0;p.presentation_time=i.simulation_time;
        r.reduced_motion=t%2==0;r.set_clock_rect([300.0,260.0,100.0,80.0]);
        for snake in &mut s {snake.mood=(t%9) as u8;snake.pupil_x=0.12;snake.pupil_y=-0.2;}
        items[0].landing_ticks=30-(t%30) as u16;r.set_items(&items,12.0);
        let n=r.build_shader(&i,&s,&b,&[],&[],&palette(),&p,&mut shader).vertex_count;
        assert!(n<shader.len());
        assert!(shader[..n].iter().filter(|v|v.params[0]==11 && v.params[1]==1).all(|v|v.params[2]>=128));
        assert!(r.build(&i,&s,&b,&[],&[],&palette(),&p,&mut classic).vertex_count<classic.len());
    }
    assert_eq!(COUNT.with(|c|c.replace(None).unwrap()),0);
}
#[test]
fn s1_contest_arcs_match_independent_wrap_tiles_including_pixel_stroke() {
    let (_,s,b,mut items)=s1_fixture();
    for (sx,sy) in [(1.0_f64,1.0_f64),(0.01,0.02),(0.02,0.01)] {
        let i=FrameInfo{world_width:1000.0,world_height:1000.0,..info()};
        let p=RenderParams{scale_x:sx,scale_y:sy,viewport_width:1000.0*sx,viewport_height:1000.0*sy,deadly_walls:0,..params()};
        items[0].x=1.0;items[0].y=1.0;
        let mut wrapped=RenderHandle::new();wrapped.set_items(&items[..1],4.0);
        let mut out=[ShaderRenderVertex::default();4096];
        let n=wrapped.build_shader(&i,&s,&b,&[],&[],&palette(),&p,&mut out).vertex_count;
        let actual=out[..n].iter().filter(|v|v.params[0]==17).count();
        let mut expected=0;
        for x in -4..=4 {for y in -4..=4 {
            let mut tiled=RenderHandle::new();tiled.set_items(&items[..1],4.0);
            let p=RenderParams{deadly_walls:1,offset_x:x as f64*1000.0*sx,offset_y:y as f64*1000.0*sy,..p};
            let n=tiled.build_shader(&i,&s,&b,&[],&[],&palette(),&p,&mut out).vertex_count;
            expected+=out[..n].iter().filter(|v|v.params[0]==17).count();
        }}
        assert_eq!(actual,expected);assert!(actual>=48);
    }
}

#[test]
fn s1_calm_authoritative_ages_advance_while_shader_clock_is_frozen() {
    let (mut i,s,b,mut items)=s1_fixture();
    let mut r=RenderHandle::new();r.reduced_motion=true;
    i.tick+=90;i.simulation_time+=3.0; // shader presentation clock remains at 20
    items[0].landing_ticks=15;r.set_items(&items,12.0);
    let mut out=[ShaderRenderVertex::default();4096];
    let n=r.build_shader(&i,&s,&b,&[],&[],&palette(),&params(),&mut out).vertex_count;
    assert_eq!(out[..n].iter().filter(|v|v.params[0]==16).count(),18);
    assert!(out[..n].iter().filter(|v|v.params[0]==16).all(|v|v.color.alpha==255));
    assert!(out[..n].iter().filter(|v|v.params[0]==11 && v.params[1]==items[0].kind).all(|v|v.params[2]==192));
    for bubble in &mut i.bubbles {bubble.age_ticks=28;}
    let n=r.build_shader(&i,&s,&b,&[],&[],&palette(),&params(),&mut out).vertex_count;
    assert_eq!(out[..n].iter().filter(|v|v.params[0]==16).count(),0);
}

#[test]
fn s1_track_is_once_per_item_and_halo_quad_has_pixel_margin() {
    let (i,s,b,mut items)=s1_fixture();
    let mut r=RenderHandle::new();let mut out=[ShaderRenderVertex::default();1024];
    for leader in [0,1,u32::MAX] {
        items[0].leader_snake_id=leader;r.set_items(&items[..1],0.5);
        let n=r.build_shader(&i,&s,&b,&[],&[],&palette(),&params(),&mut out).vertex_count;
        let arcs:Vec<_>=out[..n].iter().filter(|v|v.params[0]==17).collect();
        assert_eq!(arcs.len(),12);
        assert_eq!(arcs.iter().filter(|v|v.params[3]&4!=0).count(),6);
        assert!(arcs[if leader==1 {6} else {0}].params[3]&4!=0);
        let extent=0.5*1.62+12.0;
        assert!((arcs[0].x as f64-(items[0].x as f64-extent)).abs()<0.001);
        assert!((arcs[0].across as f64+extent/0.5).abs()<0.001);
    }
}
#[test]
fn s1_hatchling_bubble_has_minimum_size_and_yawn_has_seven_steps() {
    let (mut i,mut s,b,_)=s1_fixture();i.bubble_count=1;s[0].radius=2.0;
    let mut r=RenderHandle::new();let mut out=[ShaderRenderVertex::default();1024];
    r.reduced_motion=true;
    let n=r.build_shader(&i,&s,&b,&[],&[],&palette(),&params(),&mut out).vertex_count;
    let bubble:Vec<_>=out[..n].iter().filter(|v|v.params[0]==16).collect();
    assert_eq!(bubble.len(),6);assert_eq!(bubble[1].x-bubble[0].x,25.0);
    r.reduced_motion=false;i.bubble_count=0;s[0].mood=1;
    let mut steps=0u8;
    for ticks in 1..=39 {
        s[0].jaw_ticks=ticks;
        let n=r.build_shader(&i,&s,&b,&[],&[],&palette(),&params(),&mut out).vertex_count;
        let h=out[..n].iter().find(|v|v.params[0]==1).unwrap();
        let jaw=(h.params[3]>>6)|((h.params[2]&128)>>5);
        steps|=1<<jaw;
    }
    assert_eq!(steps,255);
}


#[test]
fn classic_pupils_remain_inside_iris_for_all_looks_moods_and_scales() {
    for scale in [0.08,0.4,1.0,2.0] {for angle in [0.0,1.2,3.4] {for mood in 0..9 {for intensity in [0,128,255] {
        for (x,y) in [(0.0,0.0),(0.18,0.0),(-0.18,0.35),(0.18,-0.35)] {
            let mut s=snake();s.angle=angle;s.face_flags=FACE_OBSERVED;s.mood=mood;s.mood_intensity=intensity;s.pupil_x=x;s.pupil_y=y;
            let mut p=params();p.scale_x=scale;p.scale_y=scale;p.interpolation=1.0;
            let r=s.radius*scale;let iris=(r*0.31).max(1.7);
            let head=(200.0*scale,200.0*scale);
            let eyes=[-1.0,1.0].map(|side|(head.0+angle.cos()*r*0.48-angle.sin()*r*0.46*side,
                head.1+angle.sin()*r*0.48+angle.cos()*r*0.46*side));
            let out=build(&mut RenderHandle::new(),s,p);
            let mut pupils=0;
            for v in out.iter().filter(|v|v.color==RenderColor {red:17,green:19,blue:26,alpha:255}) {
                pupils+=1;
                assert!(eyes.iter().any(|&(ex,ey)|(0..8).all(|edge| {
                    let normal=(edge as f64+0.5)*std::f64::consts::FRAC_PI_4;
                    (v.x as f64-ex)*normal.cos()+(v.y as f64-ey)*normal.sin()
                        <=iris*(std::f64::consts::PI/8.0).cos()+0.0001
                })),
                    "mood={mood} intensity={intensity} look={x},{y} scale={scale} angle={angle}");
            }
            assert_eq!(pupils,42);
        }
    }}}}
}

#[test]
fn prism_gulp_rainbow_change_no_vertex_counts_or_allocations() {
    let mut r=RenderHandle::new();let mut s=snake();s.segment_count=48;
    let b:Vec<_>=(0..48).map(|i|SegmentRecord{x:600.0-i as f32*7.0,y:300.0,previous_x:600.0-i as f32*7.0,previous_y:300.0}).collect();
    let mut out=vec![ShaderRenderVertex::default();8192];let mut fallback=vec![RenderVertex::default();8192];
    let base=r.build_shader(&info(),&[s],&b,&[],&[],&palette(),&params(),&mut out).vertex_count;
    let old=r.build(&info(),&[s],&b,&[],&[],&palette(),&params(),&mut fallback).vertex_count;
    s.bulges=[snakes_core::Bulge {start_tick:585,duration_ticks:55,strength:0.35,..Default::default()},snakes_core::Bulge {start_tick:580,duration_ticks:55,strength:0.35,..Default::default()}];
    let feast=EventRecord {tick:585,snake_id:0,generation:1,kind:10,duration_ticks:55,..Default::default()};
    r.reset();
    COUNT.with(|c|c.set(Some(0)));
    let new=r.build_shader(&info(),&[s],&b,&[],&[feast],&palette(),&params(),&mut out).vertex_count;
    let classic=r.build(&info(),&[s],&b,&[],&[feast],&palette(),&params(),&mut fallback).vertex_count;
    let allocs=COUNT.with(|c|c.replace(None).unwrap());assert_eq!(allocs,0);assert_eq!(base,new);assert_eq!(old,classic);
    assert!(out[..new].iter().any(|v|v.params[0]==0 && v.color.red>77));
    assert!(out[..new].iter().any(|v|v.params[0]==0 && v.params[3]>=150 && v.params[2]&50==0));
    let seed=FoodRecord {id:88,x:300.0,y:300.0,size:10.0,kind:4,life_fraction:128,..Default::default()};
    let n=r.build_shader(&info(),&[],&[],&[seed],&[],&palette(),&params(),&mut out).vertex_count;
    assert_eq!(n,6);assert_eq!(out[0].params,[8,0,128,255]);
    assert!((out[0].x as f64-(seed.x as f64-10.0*1.6*4.6)).abs()<0.001);
}

#[test]
fn prism_age_packing_visual_scale_and_vacuum_stay_independent() {
    let mut r=RenderHandle::new();let mut out=[ShaderRenderVertex::default();128];
    let mut p=params();
    let mut f=FoodRecord {id:88,x:300.0,y:300.0,size:10.0,kind:3,life_fraction:200,ripe_tick:600,..Default::default()};
    for (calm,age,expected) in [(false,0.0,0),(false,0.5,127),(false,1.0,254),
        (true,0.3,127),(true,0.6,254),(false,2.0,254)] {
        r.reduced_motion=calm;p.presentation_time=20.0+age;
        let n=r.build_shader(&info(),&[],&[],&[f],&[],&palette(),&p,&mut out).vertex_count;
        assert_eq!(n,6);assert_eq!(out[0].params,[8,0,200,expected]);
        assert!((out[0].x as f64-(300.0-10.0*1.6*4.6)).abs()<0.001);
    }
    p.presentation_time=20.0;r.reduced_motion=false;
    f.ripe_tick=585;
    r.build_shader(&info(),&[],&[],&[f],&[],&palette(),&p,&mut out);
    assert_eq!(out[0].params[3],127);
    f.ripe_tick=0;
    r.build_shader(&info(),&[],&[],&[f],&[],&palette(),&p,&mut out);
    assert_eq!(out[0].params[3],254);
    f.ripe_tick=601;
    r.build_shader(&info(),&[],&[],&[f],&[],&palette(),&p,&mut out);
    assert_eq!(out[0].params[3],0);
    f.kind=4;
    r.build_shader(&info(),&[],&[],&[f],&[],&palette(),&p,&mut out);
    assert_eq!(out[0].params[3],255);
    // Even malformed attracted prize records keep the ordinary streak dimensions.
    f.attraction=1.0;f.attraction_x=400.0;f.attraction_y=300.0;
    let mut baseline=[ShaderRenderVertex::default();6];
    for kind in 0..=4 {
        f.kind=kind;
        let n=r.build_shader(&info(),&[],&[],&[f],&[],&palette(),&p,&mut out).vertex_count;
        assert_eq!(n,12);
        if kind==0 {baseline.copy_from_slice(&out[..6]);}
        else {assert_eq!(out[..6],baseline);}
        if kind<=2 {assert!((out[6].x as f64-(300.0-10.0*4.6)).abs()<0.001);assert_eq!(out[6].params[3],0);}
    }
}

#[test]
fn prism_rainbow_and_boost_keep_the_strongest_light_origin_without_allocating() {
    let mut s=snake();s.segment_count=80;s.flags=flags::BOOSTING;
    let b:Vec<_>=(0..80).map(|j|SegmentRecord {x:900.0-j as f32*7.0,y:300.0,
        previous_x:900.0-j as f32*7.0,previous_y:300.0}).collect();
    let feast=EventRecord {tick:570,snake_id:0,generation:1,kind:10,duration_ticks:92,..Default::default()};
    let mut out=vec![ShaderRenderVertex::default();8192];
    for walls in [0,1] {for calm in [false,true] {
        let mut r=RenderHandle::new();r.reduced_motion=calm;
        let p=RenderParams {deadly_walls:walls,..params()};
        COUNT.with(|c|c.set(Some(0)));
        let n=r.build_shader(&info(),&[s],&b,&[],&[feast],&palette(),&p,&mut out).vertex_count;
        let allocations=COUNT.with(|c|c.replace(None).unwrap());assert_eq!(allocations,0);
        let mut origins=[0usize;8];
        for v in out[..n].iter().filter(|v|v.params[0]==0 && v.params[3]>0) {
            let origin=((v.params[2]>>1)&1)|((v.params[2]>>3)&6);origins[origin as usize]+=1;
            if origin==0 {assert!(v.params[3]<=153);}
        }
        assert!(origins[0]>0 && origins[6]>0,"walls={walls}, calm={calm}, origins={origins:?}");
        assert_eq!(origins.iter().sum::<usize>(),origins[0]+origins[6]);
    }}
}

#[test]
fn classic_prism_seed_ring_uses_spectrum_and_scales_without_more_vertices() {
    let mut r=RenderHandle::new();let mut out=[RenderVertex::default();512];
    let f=FoodRecord {id:88,x:300.0,y:300.0,size:10.0,kind:4,life_fraction:255,..Default::default()};
    let pal=[RenderColor {red:12,green:32,blue:62,alpha:255}];
    let n=r.build(&info(),&[],&[],&[f],&[],&pal,&params(),&mut out).vertex_count;
    assert_eq!(n,120); // unchanged two 8-sided discs and twelve 6-vertex ring segments
    let hues=[[255,100,120],[255,180,90],[245,240,120],[120,245,170],[100,205,255],[195,145,255]];
    for (j,segment) in out[48..n].chunks_exact(6).enumerate() {
        let expected=hues[j/2];
        assert!(segment.iter().all(|v|[v.color.red,v.color.green,v.color.blue]==expected && v.color.alpha==210));
    }
    // The pulsed classic halo uses the same 1.6 display factor; sim size is borrowed unchanged.
    let pulse=0.82+(params().presentation_time*3.0).sin()*0.18;
    assert!((out[1].x as f64-(300.0+10.0*pulse*1.6*3.2)).abs()<0.001);
    assert_eq!(f.size,10.0);
}

#[test]
fn prism_contested_second_leader_keeps_its_bright_arc() {
    let mut r=RenderHandle::new();let mut a=snake();a.face_flags=4;
    let mut b=a;b.id=1;b.segment_offset=3;b.color_index=1;
    let mut segments=body().to_vec();segments.extend_from_slice(&body());
    let pal=[palette()[0],RenderColor {red:255,green:150,blue:100,alpha:255}];
    let f=FoodRecord {id:88,x:250.0,y:200.0,size:8.0,kind:4,life_fraction:128,
        food_flags:1 | (2<<4) | 256 | 512,..Default::default()};
    let mut out=vec![ShaderRenderVertex::default();8192];
    let n=r.build_shader(&info(),&[a,b],&segments,&[f],&[],&pal,&params(),&mut out).vertex_count;
    let arcs:Vec<_>=out[..n].iter().filter(|v|v.params[0]==17).collect();assert_eq!(arcs.len(),12);
    let leader:Vec<_>=arcs.iter().filter(|v|v.params[3]&1!=0).collect();assert_eq!(leader.len(),6);
    assert!(leader.iter().all(|v|v.params[3]&2!=0 && v.color==pal[1]));
    let radius=f.size as f64*2.04*1.6;
    let extent=(radius*3.4).max(radius*1.62+12.0);
    assert!((arcs[0].x as f64-(f.x as f64-extent)).abs()<0.001);
}


#[test]
fn reduced_motion_retained_events_after_freeze_advance_and_expire_without_allocating() {
    let mut i=info();let p=params();let s=snake();let b=body();let pal=palette();
    let mut shader=[ShaderRenderVertex::default();1024];let mut classic=[RenderVertex::default();4096];
    // Exercise every retained transient family, including pickup head flare.
    for (event_kind,sprite_kind) in [(0,6),(4,7),(2,12),(6,13),(7,13)] {
        for use_shader in [false,true] {
            let mut r=RenderHandle::new();r.reduced_motion=true;
            r.build_shader(&i,&[s],&b,&[],&[],&pal,&p,&mut shader);
            i.tick=690;i.simulation_time=23.0;
            let e=EventRecord {tick:690,kind:event_kind,other_snake_id:1,x:200.0,y:200.0,..Default::default()};
            if use_shader {
                let n=r.build_shader(&i,&[s],&b,&[],&[e],&pal,&p,&mut shader).vertex_count;
                assert_eq!(shader[..n].iter().filter(|v|v.params[0]==sprite_kind).count(),6);
                if event_kind==2 {assert!(shader[..n].iter().any(|v|v.params[0]==1 && v.params[1]&128!=0));}
                i.tick=699;i.simulation_time=23.3;
                let n=r.build_shader(&i,&[s],&b,&[],&[],&pal,&p,&mut shader).vertex_count;
                if event_kind==2 {assert!(shader[..n].iter().filter(|v|v.params[0]==1).all(|v|v.params[1]&128==0));}
                i.tick=708;i.simulation_time=23.6;
                COUNT.with(|c|c.set(Some(0)));
                let n=r.build_shader(&i,&[s],&b,&[],&[],&pal,&p,&mut shader).vertex_count;
                assert_eq!(COUNT.with(|c|c.replace(None).unwrap()),0);
                assert_eq!(shader[..n].iter().filter(|v|v.params[0]==sprite_kind).count(),0);
            } else {
                let n=r.build(&i,&[s],&b,&[],&[e],&pal,&p,&mut classic).vertex_count;
                i.tick=708;i.simulation_time=23.6;
                COUNT.with(|c|c.set(Some(0)));
                let expired=r.build(&i,&[s],&b,&[],&[],&pal,&p,&mut classic).vertex_count;
                assert_eq!(COUNT.with(|c|c.replace(None).unwrap()),0);
                if event_kind!=4 {assert!(n>expired,"classic event {event_kind} must expire");}
            }
            i=info();
        }
    }
}

#[test]
fn reduced_motion_feast_after_freeze_reaches_both_render_formats_and_expires() {
    let mut i=info();let mut r=RenderHandle::new();r.reduced_motion=true;
    let mut s=snake();s.segment_count=48;let p=params();let pal=palette();
    let b:Vec<_>=(0..48).map(|j|SegmentRecord {x:600.0-j as f32*7.0,y:300.0,
        previous_x:600.0-j as f32*7.0,previous_y:300.0}).collect();
    let mut out=vec![ShaderRenderVertex::default();8192];let mut fallback=vec![RenderVertex::default();8192];
    r.build_shader(&i,&[s],&b,&[],&[],&pal,&p,&mut out);
    i.tick=705;i.simulation_time=23.5;
    s.bulges[0]=snakes_core::Bulge {start_tick:690,duration_ticks:55,strength:0.35,..Default::default()};
    let feast=EventRecord {tick:690,snake_id:0,generation:1,kind:10,duration_ticks:55,..Default::default()};
    COUNT.with(|c|c.set(Some(0)));
    let n=r.build_shader(&i,&[s],&b,&[],&[feast],&pal,&p,&mut out).vertex_count;
    let m=r.build(&i,&[s],&b,&[],&[feast],&pal,&p,&mut fallback).vertex_count;
    assert_eq!(COUNT.with(|c|c.replace(None).unwrap()),0);
    assert!(out[..n].iter().any(|v|v.params[0]==0 && v.params[3]>=150 && v.color.red>pal[0].red));
    assert!(fallback[..m].iter().any(|v|v.color.red>pal[0].red && v.color.green<pal[0].green));
    i.tick=735;i.simulation_time=24.5;
    let n=r.build_shader(&i,&[s],&b,&[],&[],&pal,&p,&mut out).vertex_count;
    assert!(out[..n].iter().filter(|v|v.params[0]==0).all(|v|v.params[3]==0 && v.color.red==pal[0].red));
    let m=r.build(&i,&[s],&b,&[],&[],&pal,&p,&mut fallback).vertex_count;
    let mut plain=RenderHandle::new();plain.reduced_motion=true;
    let mut reference=vec![RenderVertex::default();8192];s.bulges=[Default::default();2];
    let expected=plain.build(&i,&[s],&b,&[],&[],&pal,&p,&mut reference).vertex_count;
    assert_eq!(&fallback[..m],&reference[..expected]);
}


#[test]
fn reduced_motion_snapshot_effects_keep_advancing_after_clock_freeze() {
    let (mut i,mut s,b,mut items)=s1_fixture();
    let p=params();let pal=palette();let mut r=RenderHandle::new();r.reduced_motion=true;
    let mut out=[ShaderRenderVertex::default();4096];
    i.tick=690;i.simulation_time=23.0;i.bubble_count=1;
    let mut f=FoodRecord {id:88,x:300.0,y:300.0,size:10.0,kind:3,life_fraction:200,ripe_tick:690,..Default::default()};
    // Frozen procedural time is older than all these newly started effects.
    for age in [0u16,3,9,15,18,25,27,30] {
        i.tick=690+age as u64;i.simulation_time=23.0+age as f64/30.0;
        i.bubbles[0].age_ticks=age;
        s[0].mood=5;s[0].happy_ticks=45-age;s[0].effect_kind=1;s[0].effect_ticks=36-age;
        s[1].mood=1;s[1].jaw_ticks=39-age;
        items[0].landing_ticks=30-age;items[0].age_ticks=age;items[0].life_ticks=750-age;
        r.set_items(&items[..1],12.0);f.life_fraction=200-age as u8;
        COUNT.with(|c|c.set(Some(0)));
        let n=r.build_shader(&i,&s,&b,&[f],&[],&pal,&p,&mut out).vertex_count;
        assert_eq!(COUNT.with(|c|c.replace(None).unwrap()),0);
        let vertices=&out[..n];
        let bubble=vertices.iter().find(|v|v.params[0]==16);
        if age<27 {
            let bubble=bubble.unwrap();
            let fade=(age as f64/3.24).min(1.0)*((27-age) as f64/5.4).min(1.0);
            assert_eq!(bubble.color.alpha,(fade*255.0).round() as u8);
            let quad:Vec<_>=vertices.iter().filter(|v|v.params[0]==16).collect();
            assert!((quad[1].x-quad[0].x-2.0*(s[0].radius as f32*1.3).max(12.5)).abs()<0.001);
        } else {assert!(bubble.is_none());}
        let heads:Vec<_>=vertices.iter().filter(|v|matches!(v.params[0],1|28)).step_by(6).collect();
        let jaw=|v:&ShaderRenderVertex|(v.params[3]>>6)|((v.params[2]&128)>>5);
        assert_eq!(jaw(heads[0]),if age<17 {7} else {0},"blep age {age}");
        let yawn=(std::f64::consts::PI*(age as f64/23.4).min(1.0)).sin();
        assert_eq!(jaw(heads[1]),(yawn*7.0).round() as u8,"yawn age {age}");
        let prism=vertices.iter().find(|v|v.params[0]==8).unwrap();
        assert_eq!(prism.params[3],(age as f64*254.0/18.0).min(254.0) as u8);
        assert_eq!(prism.params[2],f.life_fraction,"expiry is authoritative");
        let item=vertices.iter().find(|v|v.params[0]==11).unwrap();
        assert_eq!(item.params[2],if age<30 {128+(age as f64/30.0*127.0).round() as u8} else {127});
        let warning=vertices.iter().find(|v|v.params[0]==14).unwrap();
        assert_eq!(warning.params[3],((age as f64/36.0)*255.0).round() as u8);
        // Contest state uses current ETAs; its pulse is disabled by motionScale.
        assert!(vertices.iter().filter(|v|v.params[0]==17).any(|v|v.params[3]&2!=0));
    }
}

#[test]
fn reduced_motion_corpse_and_contrail_lifetimes_advance_after_freeze() {
    let p=params();let pal=palette();let b=body();let mut out=[ShaderRenderVertex::default();1024];
    let mut fallback=[RenderVertex::default();4096];
    for shader in [false,true] {
        let mut r=RenderHandle::new();r.reduced_motion=true;let mut i=info();let mut s=snake();
        r.build_shader(&i,&[s],&b,&[],&[],&pal,&p,&mut out);
        i.tick=690;i.simulation_time=23.0;s.alive=0;s.flags=flags::CORPSE;
        if shader {
            let n=r.build_shader(&i,&[s],&b,&[],&[],&pal,&p,&mut out).vertex_count;
            assert!(out[..n].iter().any(|v|v.params[0]==0 && v.color.alpha>0));
        } else {
            let n=r.build(&i,&[s],&b,&[],&[],&pal,&p,&mut fallback).vertex_count;
            assert!(fallback[..n].iter().any(|v|v.color.alpha>0));
        }
        i.tick=711;i.simulation_time=23.7;
        if shader {
            let n=r.build_shader(&i,&[s],&b,&[],&[],&pal,&p,&mut out).vertex_count;
            assert!(out[..n].iter().all(|v|v.color.alpha==0));
        } else {
            let n=r.build(&i,&[s],&b,&[],&[],&pal,&p,&mut fallback).vertex_count;
            assert!(fallback[..n].iter().all(|v|v.color.alpha==0));
        }
    }
    let mut r=RenderHandle::new();r.reduced_motion=true;let mut s=snake();s.flags=flags::BOOSTING;
    let mut i=FrameInfo {tick:690,simulation_time:23.0,..info()};
    r.build_shader(&i,&[s],&b,&[],&[],&pal,&p,&mut out);
    let moved=b.map(|v|SegmentRecord {x:v.x+8.0,previous_x:v.x+8.0,..v});
    i.tick+=1;i.simulation_time+=1.0/30.0;
    let n=r.build_shader(&i,&[s],&moved,&[],&[],&pal,&p,&mut out).vertex_count;
    assert!(out[..n].iter().any(|v|v.params[0]==10));
    i.tick=711;i.simulation_time=23.7;
    let n=r.build_shader(&i,&[s],&moved,&[],&[],&pal,&p,&mut out).vertex_count;
    assert!(out[..n].iter().all(|v|v.params[0]!=10 && v.params[0]!=9));
}

#[test]
fn venom_titan_orphan_is_bounded_allocation_free_and_ages_in_calm() {
    use snakes_core::ffi::ShaderRenderVertex as ShaderVertex;
    for calm in [false,true] {
        let mut r=RenderHandle::new();r.reduced_motion=calm;
        let mut f=info();f.world_width=16000.0;f.world_height=4000.0;
        let mut p=params();p.viewport_width=16000.0;p.viewport_height=4000.0;p.interpolation=1.0;
        let mut s=snake();s.segment_count=1600;s.radius=6.0;
        let body:Vec<_>=(0..1600).map(|i|SegmentRecord {x:11500.0-i as f32*7.08,y:1000.0,previous_x:11500.0-i as f32*7.08,previous_y:1000.0}).collect();
        let mut output=vec![ShaderVertex::default();12000];
        r.build_shader(&f,&[s],&body,&[],&[],&palette(),&p,&mut output);
        f.tick+=1;f.simulation_time+=1.0/30.0;p.presentation_time=f.simulation_time;
        // No live stump sprite: every kind-25 vertex belongs to this orphan.
        s.segment_count=1248;s.stump_ticks=0;
        let event=EventRecord {tick:f.tick,kind:1,snake_id:0,other_snake_id:1,generation:1,other_generation:1,
            cut_index:1248,duration_ticks:33,release_tick:f.tick+33,..Default::default()};
        COUNT.with(|c|c.set(Some(0)));
        let fresh=r.build_shader(&f,&[s],&body,&[],&[event],&palette(),&p,&mut output);
        let count=COUNT.with(|c|c.replace(None)).unwrap();assert_eq!(count,0);
        let orphan=output[..fresh.vertex_count].iter().filter(|v|v.params[2]&128!=0 || v.params[0]==25).count();
        assert!(orphan>0 && orphan<=1200);
        assert_eq!(output[..fresh.vertex_count].iter().filter(|v|v.params[0]==25).count(),6);
        let first=output[..fresh.vertex_count].iter().position(|v|v.params[2]&128!=0).unwrap();
        let edge=&output[first..first+6];
        let width=snakes_core::shape::taper(1248.0/1599.0);
        assert!((edge[0].across as f64-width).abs()<=1.0/255.0);
        let extrusion=((edge[0].x-edge[1].x) as f64).hypot((edge[0].y-edge[1].y) as f64)/2.0;
        assert!((extrusion-s.radius*2.5*edge[0].across as f64).abs()<0.001);
        assert_eq!(edge[0].params[1],3); // shortened titan's pattern tier
        assert_eq!(edge[0].along,351.0); // source segments, not 100-edge LOD indices
        assert_eq!(edge[2].along,348.0);
        if calm {
            let sever_time=f.simulation_time;let sever_tick=f.tick;
            f.tick=sever_tick+32;f.simulation_time=sever_time+32.0/30.0;
            let held=r.build_shader(&f,&[s],&body,&[],&[event],&palette(),&p,&mut output);
            let edge=output[..held.vertex_count].iter().find(|v|v.params[2]&128!=0).unwrap();
            assert_eq!(edge.params[3],255,"Calm holds until the sim's shard release at 33 ticks");
            f.tick=sever_tick+34;f.simulation_time=sever_time+34.0/30.0;
            let dissolving=r.build_shader(&f,&[s],&body,&[],&[event],&palette(),&p,&mut output);
            let edge=output[..dissolving.vertex_count].iter().find(|v|v.params[2]&128!=0).unwrap();
            assert!(edge.params[3]<255,"Calm dissolves after the sim's shard release");
            f.tick=sever_tick;f.simulation_time=sever_time;
        }
        // Stale events cannot restart orphan life; the frozen shader clock is
        // deliberately held at its pre-sever value for the Calm path.
        f.tick+=60;f.simulation_time+=2.0;if !calm {p.presentation_time=f.simulation_time;}
        let expired=r.build_shader(&f,&[s],&body,&[],&[event],&palette(),&p,&mut output);
        assert!(output[..expired.vertex_count].iter().all(|v|v.params[2]&128==0));
    }
}

#[test]
fn venom_two_titan_orphans_reserve_both_cut_glows_inside_vertex_cap() {
    let mut r=RenderHandle::new();let mut f=info();f.world_width=16000.0;f.world_height=4000.0;
    let mut p=params();p.viewport_width=16000.0;p.viewport_height=4000.0;p.interpolation=1.0;
    let mut records:[SnakeRecord;2]=std::array::from_fn(|id|SnakeRecord {id:id as u32,
        segment_offset:(id*1600) as u32,segment_count:1600,radius:6.0,..snake()});
    let body:Vec<_>=(0..3200).map(|i|SegmentRecord {x:11500.0-(i%1600) as f32*7.08,y:1000.0+(i/1600) as f32*1000.0,
        previous_x:11500.0-(i%1600) as f32*7.08,previous_y:1000.0+(i/1600) as f32*1000.0}).collect();
    let mut output=vec![ShaderRenderVertex::default();24000];
    r.build_shader(&f,&records,&body,&[],&[],&palette(),&p,&mut output);
    f.tick+=1;f.simulation_time+=1.0/30.0;p.presentation_time=f.simulation_time;
    for s in &mut records {s.segment_count=1248;}
    let events:[EventRecord;2]=std::array::from_fn(|id|EventRecord {tick:f.tick,kind:1,snake_id:id as u32,
        other_snake_id:2,generation:1,cut_index:1248,duration_ticks:33,..Default::default()});
    COUNT.with(|c|c.set(Some(0)));
    let n=r.build_shader(&f,&records,&body,&[],&events,&palette(),&p,&mut output).vertex_count;
    assert_eq!(COUNT.with(|c|c.replace(None)).unwrap(),0);
    assert_eq!(output[..n].iter().filter(|v|v.params[0]==25).count(),12);
    assert_eq!(output[..n].iter().filter(|v|v.params[2]&128!=0 || v.params[0]==25).count(),1200);
}

#[test]
fn venom_classic_orphan_keeps_original_full_taper_width() {
    let mut r=RenderHandle::new();r.reduced_motion=true;
    let mut f=info();let mut p=params();p.interpolation=1.0;
    let mut s=snake();s.segment_count=80;s.radius=6.0;
    let body:Vec<_>=(0..80).map(|i|SegmentRecord {x:1200.0-i as f32*7.08,y:500.0,
        previous_x:1200.0-i as f32*7.08,previous_y:500.0}).collect();
    let mut output=vec![RenderVertex::default();12000];
    r.build(&f,&[s],&body,&[],&[],&palette(),&p,&mut output);
    f.tick+=1;f.simulation_time+=1.0/30.0;p.presentation_time=f.simulation_time;
    let event=EventRecord {tick:f.tick,kind:1,snake_id:0,other_snake_id:1,generation:1,
        cut_index:62,duration_ticks:33,..Default::default()};
    COUNT.with(|c|c.set(Some(0)));
    // With no live bodies/items, the retained orphan is emitted first, ahead of impact sprites.
    let n=r.build(&f,&[],&[],&[],&[event],&palette(),&p,&mut output).vertex_count;
    assert_eq!(COUNT.with(|c|c.replace(None)).unwrap(),0);
    assert!(n>=17*6);
    let edge=&output[..6];
    let extrusion=((edge[0].x-edge[1].x) as f64).hypot((edge[0].y-edge[1].y) as f64)/2.0;
    assert!((extrusion-s.radius*snakes_core::shape::taper(62.0/79.0)).abs()<=s.radius/255.0);
}

#[test]
fn venom_compact_sever_history_retries_and_two_orphan_cap() {
    let mut r=RenderHandle::new();let mut f=info();let mut p=params();p.interpolation=1.0;
    let records:[SnakeRecord;3]=std::array::from_fn(|id|SnakeRecord {id:id as u32,generation:1,alive:1,radius:8.0,
        segment_offset:(id*80) as u32,segment_count:80,..Default::default()});
    let body:Vec<_>=(0..240).map(|i|SegmentRecord {x:1200.0-(i%80) as f32*9.44,y:300.0+(i/80) as f32*200.0,
        previous_x:1200.0-(i%80) as f32*9.44,previous_y:300.0+(i/80) as f32*200.0}).collect();
    let mut output=vec![ShaderRenderVertex::default();6000];
    r.build_shader(&f,&records,&body,&[],&[],&palette(),&p,&mut output);
    let compact:[SnakeRecord;3]=std::array::from_fn(|id|SnakeRecord {segment_offset:id as u32,segment_count:1,..records[id]});
    let tails:[SegmentRecord;3]=std::array::from_fn(|id|body[id*80+79]);
    let events:[EventRecord;3]=std::array::from_fn(|id|EventRecord {tick:601,kind:1,snake_id:id as u32,other_snake_id:3,
        generation:1,cut_index:62,duration_ticks:33,..Default::default()});
    f.tick=601;f.simulation_time=601.0/30.0;p.presentation_time=f.simulation_time;
    let mut history=p;history.scale_x=0.0;history.scale_y=0.0;
    COUNT.with(|c|c.set(Some(0)));
    r.build_shader(&f,&compact,&tails,&[],&events,&palette(),&history,&mut []);
    let first=r.build_shader(&f,&[],&[],&[],&events,&palette(),&p,&mut output).vertex_count;
    let second=r.build_shader(&f,&[],&[],&[],&events,&palette(),&p,&mut output).vertex_count;
    assert_eq!(COUNT.with(|c|c.replace(None)).unwrap(),0);
    assert_eq!(first,second);
    assert_eq!(output[..first].iter().filter(|v|v.params[0]==0).count(),2*17*6);
}

#[test]
fn happy_blep_lasts_twenty_seven_ticks_and_does_not_create_a_heart() {
    let mut r=RenderHandle::new();let mut out=[ShaderRenderVertex::default();256];
    let mut s=snake();s.face_flags=FACE_OBSERVED;s.mood=5;s.mood_intensity=255;
    for kind in [1,23,24] {
        s.effect_kind=if kind==1 {0} else {4};
        s.effect_ticks=if kind==1 {0} else {240};
        s.flags=if kind==24 {flags::STRIKE} else {0};
        for age in [0,16,17,26,27,44] {
            s.happy_ticks=45-age;
            let n=r.build_shader(&info(),&[s],&body(),&[],&[],&palette(),&params(),&mut out).vertex_count;
            let head=out[..n].iter().find(|v|v.params[0]==kind).unwrap();
            let jaw=(head.params[3]>>6)|((head.params[2]&128)>>5);
            assert_eq!((head.params[2]>>1)&15,5);
            assert_eq!(jaw,if age<27 {7} else {0},"kind {kind}, blep age {age}");
            assert!(out[..n].iter().all(|v|v.params[0]!=16),"Happy alone emits no bubble");
        }
    }
}

#[test]
fn venom_terminal_cut_retains_one_point_and_cut_glow_without_allocations() {
    for shader in [false,true] {
        let mut r=RenderHandle::new();r.reduced_motion=true;let mut f=info();let mut p=params();p.interpolation=1.0;
        let s=SnakeRecord {segment_count:80,radius:6.0,..snake()};
        let body:Vec<_>=(0..80).map(|j|SegmentRecord {x:1200.0-j as f32*7.08,y:500.0,
            previous_x:1200.0-j as f32*7.08,previous_y:500.0}).collect();
        let mut vertices=vec![RenderVertex::default();12000];let mut sprites=vec![ShaderRenderVertex::default();12000];
        if shader {r.build_shader(&f,&[s],&body,&[],&[],&palette(),&p,&mut sprites);}
        else {r.build(&f,&[s],&body,&[],&[],&palette(),&p,&mut vertices);}
        f.tick+=1;f.simulation_time+=1.0/30.0;
        let event=EventRecord {tick:f.tick,kind:1,snake_id:0,generation:1,cut_index:79,duration_ticks:33,..Default::default()};
        COUNT.with(|c|c.set(Some(0)));
        if shader {
            let n=r.build_shader(&f,&[],&[],&[],&[event],&palette(),&p,&mut sprites).vertex_count;
            assert!(sprites[..n].iter().any(|v|v.params[2]&128!=0));
            assert_eq!(sprites[..n].iter().filter(|v|v.params[0]==25).count(),6);
        } else {
            let n=r.build(&f,&[],&[],&[],&[event],&palette(),&p,&mut vertices).vertex_count;
            assert!(vertices[..n].iter().any(|v|v.color.alpha==220));
        }
        assert_eq!(COUNT.with(|c|c.replace(None)).unwrap(),0);
    }
}
#[test]
fn fallback_corpse_never_keeps_a_bright_bite_stump() {
    let mut r=RenderHandle::new();let mut f=info();let mut p=params();p.interpolation=1.0;
    let s=SnakeRecord {stump_ticks:48,..snake()};let mut out=[RenderVertex::default();4096];
    let alive=r.build(&f,&[s],&body(),&[],&[],&palette(),&p,&mut out).vertex_count;
    assert!(out[..alive].iter().any(|v|v.color.alpha==220));
    let corpse=SnakeRecord {alive:0,flags:flags::CORPSE,..s};
    f.tick+=1;f.simulation_time+=1.0/30.0;p.presentation_time=f.simulation_time;
    let fresh=r.build(&f,&[corpse],&body(),&[],&[],&palette(),&p,&mut out).vertex_count;
    assert!(out[..fresh].iter().all(|v|v.color.alpha!=220));
    f.tick+=15;f.simulation_time+=0.5;p.presentation_time=f.simulation_time;
    COUNT.with(|c|c.set(Some(0)));
    let fading=r.build(&f,&[corpse],&body(),&[],&[],&palette(),&p,&mut out).vertex_count;
    assert_eq!(COUNT.with(|c|c.replace(None)).unwrap(),0);
    assert!(out[..fading].iter().all(|v|v.color.alpha!=220));
}

#[path="support/giant_render_fixture.rs"] mod giant_render_fixture;
#[test]
fn giant_6000_visible_both_paths_and_growth_are_allocation_free() {
    let (mut f,mut s,b)=giant_render_fixture::fixture(false);
    let pal=giant_render_fixture::palette();let p=RenderParams{interpolation:1.0,..params()};
    assert_eq!(b[..6000].iter().filter(|v|v.x>=0.0 && v.x<3440.0 && v.y>=0.0 && v.y<1440.0).count(),6000);
    let mut shader=RenderHandle::new();let mut classic=RenderHandle::new();
    let mut gpu=vec![ShaderRenderVertex::default();150000];let mut out=vec![RenderVertex::default();500000];
    COUNT.with(|c|c.set(Some(0)));
    let mut last=(0,0);
    for count in (1600..6000).step_by(64).chain([6000]) {
        s[0].segment_count=count;f.tick+=1;
        last=(shader.build_shader(&f,&s,&b,&[],&[],&pal,&p,&mut gpu).vertex_count,
              classic.build(&f,&s,&b,&[],&[],&pal,&p,&mut out).vertex_count);
    }
    assert_eq!(COUNT.with(|c|c.replace(None)).unwrap(),0);
    assert_eq!(last,(41280,105366));
    assert!(gpu[..last.0].iter().all(|v|v.x.is_finite() && v.y.is_finite()));
    assert!(out[..last.1].iter().all(|v|v.x.is_finite() && v.y.is_finite()));
    assert_eq!(shader_fingerprint(&gpu[..last.0]),0xf131030cb72994b0);
    assert_eq!(fingerprint(&out[..last.1]),0x58000893da28c848);
}

#[test]
fn giant_2999_segment_venom_orphan_both_paths_keep_cap_and_source_indices() {
    for calm in [false,true] {
        let (mut f,mut s,b)=giant_render_fixture::fixture(false);s.truncate(1);
        let pal=giant_render_fixture::palette();let mut p=RenderParams{interpolation:1.0,..params()};
        let mut shader=RenderHandle::new();let mut classic=RenderHandle::new();
        shader.reduced_motion=calm;classic.reduced_motion=calm;
        let mut gpu=vec![ShaderRenderVertex::default();150000];let mut out=vec![RenderVertex::default();500000];
        shader.build_shader(&f,&s,&b,&[],&[],&pal,&p,&mut gpu);
        classic.build(&f,&s,&b,&[],&[],&pal,&p,&mut out);
        s[0].segment_count=3001;s[0].alive=0; // Isolate the orphan geometry after observing its full trail.
        f.tick+=1;f.simulation_time+=1.0/30.0;p.presentation_time=f.simulation_time;
        let event=EventRecord{tick:f.tick,kind:1,snake_id:0,generation:1,cut_index:3001,duration_ticks:33,
            release_tick:f.tick+33,..Default::default()};
        COUNT.with(|c|c.set(Some(0)));
        let a=shader.build_shader(&f,&s,&b,&[],&[event],&pal,&p,&mut gpu).vertex_count;
        let c=classic.build(&f,&s,&b,&[],&[event],&pal,&p,&mut out).vertex_count;
        assert_eq!(COUNT.with(|c|c.replace(None)).unwrap(),0);
        let orphan:Vec<_>=gpu[..a].iter().filter(|v|v.params[2]&128!=0 || v.params[0]==25).collect();
        assert!(!orphan.is_empty() && orphan.len()<=1200);
        assert_eq!(orphan.iter().filter(|v|v.params[0]==25).count(),6);
        assert!(orphan.iter().any(|v|v.params[2]&128!=0 && v.along==2998.0));
        assert!(orphan.iter().any(|v|v.params[2]&128!=0 && v.along==0.0));
        assert!(c>0 && c<=1200,"classic orphan: {c}");
        if calm {
            let mut error=0.0_f64;
            for edge in gpu[..a].chunks_exact(6).filter(|e|e[0].params[2]&128!=0) {
                let from=2998-edge[0].along as usize;let to=2998-edge[2].along as usize;
                let start=b[3001+from];let end=b[3001+to];
                let dx=(end.x-start.x) as f64;let dy=(end.y-start.y) as f64;let length=dx*dx+dy*dy;
                for point in &b[3001+from..=3001+to] {
                    let x=(point.x-start.x) as f64;let y=(point.y-start.y) as f64;
                    let t=if length>0.0 {((x*dx+y*dy)/length).clamp(0.0,1.0)} else {0.0};
                    error=error.max((x-t*dx).hypot(y-t*dy));
                }
            }
            let mut uniform_error=0.0_f64;
            for i in 0..100 {
                let from=i*2998/100;let to=(i+1)*2998/100;
                let start=b[3001+from];let end=b[3001+to];
                let dx=(end.x-start.x) as f64;let dy=(end.y-start.y) as f64;let length=dx*dx+dy*dy;
                for point in &b[3001+from..=3001+to] {
                    let x=(point.x-start.x) as f64;let y=(point.y-start.y) as f64;
                    let t=if length>0.0 {((x*dx+y*dy)/length).clamp(0.0,1.0)} else {0.0};
                    uniform_error=uniform_error.max((x-t*dx).hypot(y-t*dy));
                }
            }
            println!("2999-segment orphan: shader={a}, classic={c}, chord_error_px={uniform_error:.3}->{error:.3}");
            assert!(error<=12.0 && error<uniform_error*0.1,
                "Long orphan must preserve the tight spiral turns within its existing vertex cap");
        }
        COUNT.with(|c|c.set(Some(0)));
        for sample in 0..32 {
            p.scale_x=if sample%2==0 {1.0} else {0.75};
            let a=shader.build_shader(&f,&s,&b,&[],&[event],&pal,&p,&mut gpu).vertex_count;
            let c=classic.build(&f,&s,&b,&[],&[event],&pal,&p,&mut out).vertex_count;
            assert!(a<=1200 && c<=1200);
        }
        assert_eq!(COUNT.with(|c|c.replace(None)).unwrap(),0);
        f.tick+=60;f.simulation_time+=2.0;
        if !calm {p.presentation_time=f.simulation_time;}
        assert_eq!(shader.build_shader(&f,&s,&b,&[],&[event],&pal,&p,&mut gpu).vertex_count,0);
        assert_eq!(classic.build(&f,&s,&b,&[],&[event],&pal,&p,&mut out).vertex_count,0);
    }
}

#[test]
fn frost_packing_ice_tint_interpolation_and_palette_modes() {
    let b=body();let mut output=vec![ShaderRenderVertex::default();4096];
    for pal in [palette(),[RenderColor {red:255,green:255,blue:255,alpha:255}],[RenderColor {red:255,green:200,blue:221,alpha:255}]] {
        let mut s=snake();s.flags=flags::FROZEN;s.frozen_ticks=75;s.mood=8;s.face_flags=2;
        let n=RenderHandle::new().build_shader(&info(),&[s],&b,&[],&[],&pal,&params(),&mut output).vertex_count;
        let bodies:Vec<_>=output[..n].iter().filter(|v|v.params[0]==0).copied().collect();
        assert!(!bodies.is_empty());assert!(bodies.iter().all(|v|v.params[1]&128!=0));
        let expected=if pal[0].red==77 {(151,235,255)} else if pal[0].green==255 {(238,241,242)} else {(232,226,241)};
        assert_eq!((bodies[0].color.red,bodies[0].color.green,bodies[0].color.blue),expected);
        assert!(output[..n].iter().filter(|v|v.params[0]==1).all(|v|(v.params[2]>>1)&15==8));
        s.flags=0;s.frozen_ticks=0;s.mood=0;
        let m=RenderHandle::new().build_shader(&info(),&[s],&b,&[],&[],&pal,&params(),&mut output).vertex_count;
        assert_eq!(n,m,"ice adds no body geometry");
        let ordinary=output[..m].iter().find(|v|v.params[0]==0).unwrap();
        assert_eq!(bodies[0].x,ordinary.x,"frozen bodies still interpolate half-speed motion");
        assert_ne!(bodies[0].color,ordinary.color);
    }
}
#[test]
fn frost_nova_crack_lifetimes_and_both_formats_are_allocation_free_in_calm() {
    for calm in [false,true] {for shader in [false,true] {
        let mut r=RenderHandle::new();r.reduced_motion=calm;
        let mut i=info();let mut p=params();let pal=palette();
        let nova=EventRecord {tick:i.tick,x:700.0,y:500.0,kind:3,value:288.0,duration_ticks:21,..Default::default()};
        let crack=EventRecord {tick:i.tick,x:200.0,y:200.0,kind:7,other_snake_id:5,duration_ticks:15,..Default::default()};
        let mut classic=vec![RenderVertex::default();4096];let mut out=vec![ShaderRenderVertex::default();4096];
        let mut previous=0;
        COUNT.with(|c|c.set(Some(0)));
        for age in [0u64,14,15,20,21,45] {
            i.tick=600+age;i.simulation_time=20.0+age as f64/30.0;
            // Frozen procedural time does not hold transient simulation lifetimes.
            if !calm {p.presentation_time=i.simulation_time;}
            let events=if age==0 {&[nova,crack][..]} else {&[][..]};
            let n=if shader {r.build_shader(&i,&[snake()],&body(),&[],events,&pal,&p,&mut out).vertex_count}
                else {r.build(&i,&[snake()],&body(),&[],events,&pal,&p,&mut classic).vertex_count};
            if shader {
                assert_eq!(out[..n].iter().filter(|v|v.params[0]==18).count(),if age<21 {6} else {0});
                assert_eq!(out[..n].iter().filter(|v|v.params[0]==6 && v.params[2]==1).count(),if age<15 {6} else {0});
            } else if age==0 {previous=n;} else if age>=21 {assert!(n<previous);}
        }
        assert_eq!(COUNT.with(|c|c.replace(None).unwrap()),0);
    }}
}

#[test]
fn event_sprites_are_bounded_allocation_free_wrap_and_calm_keeps_landing_lifetimes() {
    use snakes_core::{FoodKind,WorldEventState};
    let mut r=RenderHandle::new();let mut f=info();let mut p=params();p.presentation_time=f.simulation_time;
    f.world_event=WorldEventState {kind:1,phase:1,x:700.0,y:500.0,radius:216.0,start_tick:555,end_tick:674,..Default::default()};
    let meteors:[FoodRecord;24]=std::array::from_fn(|i|FoodRecord {id:i as u64+1,kind:FoodKind::Meteor as u8,
        x:400.0+i as f32*20.0,y:400.0,size:6.48,attraction_x:600.0+i as f32*20.0,attraction_y:600.0,
        motion_origin_x:200.0+i as f32*20.0,motion_origin_y:200.0,motion_ticks:7,life_fraction:255,..Default::default()});
    let mut output=vec![ShaderRenderVertex::default();4096];
    COUNT.with(|c|c.set(Some(0)));
    let a=r.build_shader(&f,&[],&[],&meteors,&[],&palette(),&p,&mut output);
    let allocations=COUNT.with(|c|c.replace(None)).unwrap();assert_eq!(allocations,0);
    assert_eq!(a.vertex_count,6+24*12);assert_eq!(output[..a.vertex_count].iter().filter(|v|v.params[0]==20).count(),6);
    r.reduced_motion=true;
    let calm=r.build_shader(&f,&[],&[],&meteors,&[],&palette(),&p,&mut output);assert_eq!(calm.vertex_count,6+24*6);
    let head=&output[6..12];let cx=head.iter().map(|v|v.x as f64).sum::<f64>()/6.0;
    // Triangle list's center has a diagonal bias; bounds prove landing placement.
    assert!(cx.is_finite());assert!((head.iter().map(|v|v.x).fold(f32::INFINITY,f32::min)+head.iter().map(|v|v.x).fold(f32::NEG_INFINITY,f32::max)-1200.0).abs()<0.01);
    let star=FoodRecord {id:99,kind:FoodKind::Star as u8,x:600.0,y:600.0,size:6.48,life_fraction:255,ripe_tick:600,..Default::default()};
    r.build_shader(&f,&[],&[],&[star],&[],&palette(),&p,&mut output);assert_eq!(output[6].params[0],21);assert_eq!(output[6].params[3],0);
    f.tick+=18;f.simulation_time+=0.6;p.presentation_time=f.simulation_time;
    r.build_shader(&f,&[],&[],&[star],&[],&palette(),&p,&mut output);assert_eq!(output[6].params[3],255,"birth ages on simulation time while procedural time is frozen");
    // Both wrap copies contain the full streak/head envelope.
    p.deadly_walls=0;f.world_event.kind=0;let seam=FoodRecord {x:2.0,attraction_x:202.0,motion_origin_x:-198.0,..meteors[0]};
    r.reduced_motion=false;let wrapped=r.build_shader(&f,&[],&[],&[seam],&[],&palette(),&p,&mut output);assert!(wrapped.vertex_count>=18);
}

#[test]
fn inventory_pips_follow_curves_keep_icons_upright_and_opaque_in_phase_night() {
    let mut s=SnakeRecord {radius:18.0,segment_count:20,inv_kind:[2,4,5],inv_count:3,inv_life:[255,128,2],effect_kind:3,effect_ticks:90,..snake()};
    let mut b=[SegmentRecord::default();20];
    for (i,v) in b.iter_mut().enumerate() {let a=i as f64*0.14;*v=SegmentRecord{x:(700.0-150.0*a.sin()) as f32,y:(500.0+150.0*(1.0-a.cos())) as f32,previous_x:(700.0-150.0*a.sin()) as f32,previous_y:(500.0+150.0*(1.0-a.cos())) as f32};}
    let mut i=info();i.ambient=0.28;let mut out=[ShaderRenderVertex::default();1024];
    let mut r=RenderHandle::new();let n=r.build_shader(&i,&[s],&b,&[],&[],&palette(),&params(),&mut out).vertex_count;
    let pips:Vec<_>=out[..n].chunks_exact(6).filter(|v|v[0].params[0]==26).collect();assert_eq!(pips.len(),3);
    let head=out[..n].iter().rposition(|v|v.params[0]==1).unwrap();assert!(out[head+1].params[0]==26);
    for (slot,pip) in pips.iter().enumerate() {
        assert_eq!(pip[0].params,[26,s.inv_kind[slot],s.inv_life[slot],0]);assert_eq!(pip[0].color.alpha,255);
        assert_eq!(pip[0].y,pip[1].y);assert_eq!(pip[0].x,pip[2].x);
        let mut left=[2.2,4.3,6.4][slot]*18.0;let mut expected=(0.0,0.0);
        for edge in b.windows(2) {let dx=(edge[1].x-edge[0].x) as f64;let dy=(edge[1].y-edge[0].y) as f64;let length=dx.hypot(dy);if left<=length {expected=(edge[0].x as f64+dx*left/length,edge[0].y as f64+dy*left/length);break;}left-=length;}
        assert!(((pip[0].x+pip[1].x) as f64*0.5-expected.0).abs()<0.001);
        assert!(((pip[0].y+pip[2].y) as f64*0.5-expected.1).abs()<0.001);
    }
    s.flags=flags::FROZEN;s.effect_kind=0;let n=r.build_shader(&i,&[s],&b,&[],&[],&palette(),&params(),&mut out).vertex_count;
    assert!(out[..n].iter().filter(|v|v.params[0]==26).all(|v|v.color.alpha==255));
}

#[test]
fn inventory_stash_use_compaction_fizzle_retry_and_calm_are_allocation_free() {
    for calm in [false,true] {
        let mut r=RenderHandle::new();r.reduced_motion=calm;let mut i=info();let mut p=params();let pal=palette();
        let mut s=SnakeRecord {radius:18.0,segment_count:12,inv_kind:[3,1,5],inv_count:3,inv_life:[255;3],..snake()};
        let b: [SegmentRecord;12]=std::array::from_fn(|j|SegmentRecord{x:700.0-j as f32*21.24,y:500.0,previous_x:700.0-j as f32*21.24,previous_y:500.0});
        let mut out=[ShaderRenderVertex::default();1024];let mut retry=out;let mut fallback=[RenderVertex::default();8192];
        let stash=EventRecord {tick:600,snake_id:0,generation:1,kind:14,other_snake_id:5,cut_index:2,x:740.0,y:500.0,..Default::default()};
        COUNT.with(|c|c.set(Some(0)));
        let n=r.build_shader(&i,&[s],&b,&[],&[stash],&pal,&p,&mut out).vertex_count;
        let m=r.build_shader(&i,&[s],&b,&[],&[stash],&pal,&p,&mut retry).vertex_count;
        assert_eq!(n,m);assert_eq!(&out[..n],&retry[..m]);assert!(!out[..n].iter().any(|v|v.params[0]==12));
        assert!(out[..n].iter().any(|v|v.params[0]==13));
        i.tick=615;i.simulation_time=20.5;p.presentation_time=i.simulation_time;
        s.inv_windup=2;let use_event=EventRecord {tick:615,kind:13,other_snake_id:1,cut_index:1,duration_ticks:4,snake_id:0,generation:1,x:700.0,y:500.0,..Default::default()};
        r.build_shader(&i,&[s],&b,&[],&[use_event],&pal,&p,&mut out);
        i.tick=617;i.simulation_time=617.0/30.0;p.presentation_time=i.simulation_time;
        let n=r.build_shader(&i,&[s],&b,&[],&[],&pal,&p,&mut out).vertex_count;
        let flight=out[..n].iter().filter(|v|v.params[0]==26 && v.params[3]>=128).count();assert_eq!(flight,6);
        let last=out[..n].chunks_exact(6).filter(|v|v[0].params[0]==26).last().unwrap();assert_eq!(last[0].params[1],1);
        i.tick=619;i.simulation_time=619.0/30.0;p.presentation_time=i.simulation_time;
        s.inv_windup=0;s.inv_count=2;s.inv_kind=[3,5,0];s.inv_life=[255,255,0];s.effect_kind=1;s.effect_ticks=180;
        let pickup=EventRecord {tick:619,kind:2,flags:event_flags::HELD_ACTIVATION,other_snake_id:1,snake_id:0,generation:1,x:700.0,y:500.0,..Default::default()};
        let n=r.build_shader(&i,&[s],&b,&[],&[pickup],&pal,&p,&mut out).vertex_count;
        assert_eq!(out[..n].iter().filter(|v|v.params[0]==26).count(),12);assert!(out[..n].iter().any(|v|v.params[0]==12));
        assert!(out[..n].iter().any(|v|v.params[0]==0 && v.params[3]>0));
        i.tick=630;i.simulation_time=21.0;p.presentation_time=i.simulation_time;
        s.inv_kind=[5,0,0];s.inv_count=1;let fizzle=EventRecord {tick:630,kind:15,other_snake_id:3,cut_index:0,duration_ticks:8,snake_id:0,generation:1,x:660.4,y:500.0,..Default::default()};
        let n=r.build_shader(&i,&[s],&b,&[],&[fizzle],&pal,&p,&mut out).vertex_count;
        assert_eq!(out[..n].iter().filter(|v|v.params[0]==26).count(),12);
        r.build(&i,&[s],&b,&[],&[fizzle],&pal,&p,&mut fallback);
        i.tick=639;i.simulation_time=21.3;p.presentation_time=i.simulation_time;
        let n=r.build_shader(&i,&[s],&b,&[],&[],&pal,&p,&mut out).vertex_count;
        assert_eq!(out[..n].iter().filter(|v|v.params[0]==26).count(),6);
        assert_eq!(COUNT.with(|c|c.replace(None).unwrap()),0);
        s.generation+=1;s.inv_count=0;s.effect_kind=0;s.effect_ticks=0;
        let n=r.build_shader(&i,&[s],&b,&[],&[],&pal,&p,&mut out).vertex_count;
        assert!(!out[..n].iter().any(|v|v.params[0]==26));
    }
}

#[test]
fn inventory_item_bubbles_map_all_five_glyphs_and_palette_accents() {
    let b=body();let s=snake();let mut out=[ShaderRenderVertex::default();256];
    for pal in [palette(),[RenderColor {red:255,green:255,blue:255,alpha:255}],[RenderColor {red:255,green:200,blue:221,alpha:255}]] {
        for glyph in 5..=9 {
            let mut i=info();i.bubble_count=1;i.bubbles[0]=snakes_core::Bubble {snake_id:0,generation:1,glyph,age_ticks:12,..Default::default()};
            let n=RenderHandle::new().build_shader(&i,&[s],&b,&[],&[],&pal,&params(),&mut out).vertex_count;
            let bubble=out[..n].iter().find(|v|v.params[0]==16).unwrap();assert_eq!(bubble.params[1],glyph);
            assert_ne!(bubble.color,pal[0]);assert_eq!(bubble.color.alpha,255);
        }
    }
}

#[test]
fn inventory_surge_ribbons_and_gems_obey_global_vertex_caps() {
    let mut r=RenderHandle::new();let b:[SegmentRecord;14*30]=std::array::from_fn(|j|SegmentRecord {x:500.0+(j/30) as f32*180.0-(j%30) as f32*9.44,y:300.0+(j/30) as f32*40.0,previous_x:500.0+(j/30) as f32*180.0-(j%30) as f32*9.44,previous_y:300.0+(j/30) as f32*40.0});
    let s:[SnakeRecord;14]=std::array::from_fn(|j|SnakeRecord {id:j as u32,segment_offset:(j*30) as u32,segment_count:30,effect_kind:1,effect_ticks:180,inv_kind:[1,2,3],inv_life:[255;3],inv_count:3,flags:flags::BOOSTING,..snake()});
    let mut out=[ShaderRenderVertex::default();4096];
    COUNT.with(|c|c.set(Some(0)));
    let n=r.build_shader(&info(),&s,&b,&[],&[],&palette(),&params(),&mut out).vertex_count;
    assert_eq!(COUNT.with(|c|c.replace(None).unwrap()),0);
    assert_eq!(out[..n].iter().filter(|v|v.params[0]==26).count(),3*snakes_core::MAX_SNAKES*6);
    assert!(out[..n].iter().filter(|v|v.params[0]==27).count()<=336);
    assert_eq!(out[..n].iter().filter(|v|v.params[0]==28).count(),14*6);
}

#[test]
fn inventory_magnet_opens_and_frost_waits_for_visual_front() {
    let mut r=RenderHandle::new();let mut i=info();let mut p=params();let mut out=[ShaderRenderVertex::default();1024];
    let mut s=[SnakeRecord {effect_kind:2,effect_ticks:180,..snake()},SnakeRecord {id:1,segment_offset:3,flags:flags::FROZEN,frozen_ticks:75,mood:8,..snake()}];
    let b=[body()[0],body()[1],body()[2],SegmentRecord {x:300.0,y:200.0,previous_x:300.0,previous_y:200.0},SegmentRecord {x:292.0,y:200.0,previous_x:292.0,previous_y:200.0},SegmentRecord {x:284.0,y:200.0,previous_x:284.0,previous_y:200.0}];
    let pickup=EventRecord {tick:600,kind:2,snake_id:0,generation:1,other_snake_id:2,x:200.0,y:200.0,..Default::default()};
    let nova=EventRecord {tick:600,kind:3,snake_id:0,generation:1,other_snake_id:5,x:200.0,y:200.0,value:128.0,duration_ticks:21,..Default::default()};
    let n=r.build_shader(&i,&s,&b,&[],&[pickup,nova],&palette(),&p,&mut out).vertex_count;
    assert_eq!(out[..n].iter().find(|v|v.params[0]==15).unwrap().params[3],1);
    assert!(out[..n].iter().filter(|v|v.params[0]==0).all(|v|v.params[1]&128==0));
    i.tick=612;i.simulation_time=20.4;p.presentation_time=i.simulation_time;s[1].frozen_ticks=63;
    let n=r.build_shader(&i,&s,&b,&[],&[],&palette(),&p,&mut out).vertex_count;
    assert_eq!(out[..n].iter().find(|v|v.params[0]==15).unwrap().params[3],255);
    assert!(out[..n].iter().any(|v|v.params[0]==0 && v.params[1]&128!=0));
}

#[test]
fn inventory_classic_magnet_grows_from_head_and_ignores_invalid_owners() {
    let mut r=RenderHandle::new();let mut i=info();let mut p=params();let b=body();let pal=palette();
    let s=SnakeRecord {effect_kind:2,effect_ticks:180,..snake()};
    let invalid=SnakeRecord {id:1,radius:f64::NAN,flags:flags::FROZEN,inv_count:3,..s};
    let pickup=EventRecord {tick:600,kind:2,snake_id:0,generation:1,other_snake_id:2,x:200.0,y:200.0,..Default::default()};
    let nova=EventRecord {tick:600,kind:3,snake_id:0,generation:1,x:200.0,y:200.0,value:128.0,duration_ticks:21,..Default::default()};
    let mut out=[RenderVertex::default();8192];let mut shader=[ShaderRenderVertex::default();1024];
    let n=r.build(&i,&[s,invalid],&b,&[],&[pickup,nova],&pal,&p,&mut out).vertex_count;
    let reach=|vertices:&[RenderVertex]|vertices.iter().filter(|v|v.color.red==255 && v.color.green==95 && v.color.blue==210)
        .map(|v|((v.x-200.0) as f64).hypot((v.y-200.0) as f64)).fold(0.0_f64,f64::max);
    assert!(reach(&out[..n])<4.0*s.radius);
    i.tick=612;i.simulation_time=20.4;p.presentation_time=i.simulation_time;
    let n=r.build(&i,&[s,invalid],&b,&[],&[],&pal,&p,&mut out).vertex_count;
    assert!(reach(&out[..n])>8.0*s.radius);
    assert!(r.build_shader(&i,&[s,invalid],&b,&[],&[],&pal,&p,&mut shader).vertex_count>0);
}

#[test]
fn inventory_death_drops_grow_drift_and_resolve_compact_tail_history() {
    let mut r=RenderHandle::new();let mut i=info();let mut p=params();let mut out=[ShaderRenderVertex::default();1024];
    let s=SnakeRecord {alive:0,flags:flags::CORPSE,segment_count:12,radius:18.0,..snake()};
    let b:[SegmentRecord;12]=std::array::from_fn(|j|SegmentRecord {x:700.0-j as f32*21.24,y:500.0,previous_x:700.0-j as f32*21.24,previous_y:500.0});
    let item=ItemRecord {id:1,kind:1,state:1,x:660.4,y:554.0,age_ticks:0,life_ticks:300,..Default::default()};
    let event=EventRecord {tick:600,kind:5,snake_id:0,generation:1,cut_index:0,other_snake_id:1,duration_ticks:15,x:item.x,y:item.y,..Default::default()};
    let compact=SnakeRecord {segment_count:1,..s};let compact_p=RenderParams {scale_x:0.0,scale_y:0.0,..p};
    r.build_shader(&i,&[compact],&[b[11]],&[],&[event],&palette(),&compact_p,&mut out);
    r.set_items(&[item],37.8);
    let n=r.build_shader(&i,&[s],&b,&[],&[event],&palette(),&p,&mut out).vertex_count;
    let drop=out[..n].chunks_exact(6).find(|v|v[0].params[0]==11).unwrap();
    assert!(((drop[0].y+drop[2].y)*0.5-500.0).abs()<0.001);
    let start_width=drop[1].x-drop[0].x;
    i.tick=611;i.simulation_time=611.0/30.0;p.presentation_time=i.simulation_time;
    r.set_items(&[ItemRecord {age_ticks:11,..item}],37.8);
    let n=r.build_shader(&i,&[s],&b,&[],&[],&palette(),&p,&mut out).vertex_count;
    let drop=out[..n].chunks_exact(6).find(|v|v[0].params[0]==11).unwrap();
    assert!(((drop[0].y+drop[2].y)*0.5-item.y).abs()<0.001);
    assert!(((drop[1].x-drop[0].x)/start_width-2.47).abs()<0.001);
    // A new death can reuse the target and kind while old history survives.
    i.tick=620;i.simulation_time=620.0/30.0;p.presentation_time=i.simulation_time;
    let next=ItemRecord {id:2,age_ticks:0,..item};r.set_items(&[next],37.8);
    let next_event=EventRecord {tick:620,generation:2,..event};
    let next_snake=SnakeRecord {generation:2,..s};
    let n=r.build_shader(&i,&[next_snake],&b,&[],&[next_event],&palette(),&p,&mut out).vertex_count;
    let drop=out[..n].chunks_exact(6).find(|v|v[0].params[0]==11).unwrap();
    assert!(((drop[0].y+drop[2].y)*0.5-500.0).abs()<0.001,"new drop must start at the new neck");
    assert!((drop[1].x-drop[0].x-start_width).abs()<0.001,"new drop must restart at pip size");
}

#[test]
fn inventory_phase_enters_over_three_ticks_and_frost_does_not_restart_fade() {
    let mut r=RenderHandle::new();let mut i=info();let mut p=params();let b=body();let mut out=[ShaderRenderVertex::default();512];
    let s=SnakeRecord {effect_kind:3,effect_ticks:120,..snake()};
    let phase=EventRecord {tick:600,kind:2,snake_id:0,generation:1,other_snake_id:3,x:200.0,y:200.0,..Default::default()};
    let n=r.build_shader(&i,&[s],&b,&[],&[phase],&palette(),&p,&mut out).vertex_count;
    assert!(out[..n].iter().any(|v|v.params[0]==0 && (v.params[1]>>2)&7==6));
    assert_eq!(out[..n].iter().find(|v|v.params[0]==29).unwrap().color.alpha,crate_taper_alpha(3));
    i.tick=602;i.simulation_time=602.0/30.0;p.presentation_time=i.simulation_time;
    let n=r.build_shader(&i,&[s],&b,&[],&[],&palette(),&p,&mut out).vertex_count;
    let head=out[..n].iter().find(|v|v.params[0]>=29).unwrap();
    let progress=(head.params[0] as u16-29)*255/226;assert!((168..=171).contains(&progress));
    assert_eq!(head.color.alpha,crate_taper_alpha(3));
    i.tick=604;i.simulation_time=604.0/30.0;p.presentation_time=i.simulation_time;
    let frost=EventRecord {tick:604,other_snake_id:5,..phase};
    let n=r.build_shader(&i,&[s],&b,&[],&[frost],&palette(),&p,&mut out).vertex_count;
    assert!(out[..n].iter().any(|v|v.params[0]==0 && (v.params[1]>>2)&7==3));
    assert!(!out[..n].iter().any(|v|v.params[0]==29));
}

#[test]
fn inventory_neck_cache_matches_fresh_geometry_after_same_tick_mutations() {
    let mut r=RenderHandle::new();let mut s=SnakeRecord {radius:18.0,segment_count:12,inv_kind:[1,2,3],inv_count:3,inv_life:[255;3],..snake()};
    let mut b:[SegmentRecord;12]=std::array::from_fn(|j|SegmentRecord {x:700.0-j as f32*21.24,y:500.0,previous_x:700.0-j as f32*21.24,previous_y:500.0});
    let mut out=[ShaderRenderVertex::default();1024];let mut expected=out;let mut p=params();
    for change in 0..5 {
        match change {1=>b[2].y+=10.0,2=>{s.radius=16.0;s.inv_life[1]=12;},3=>{p.interpolation=0.75;s.inv_kind=[5,1,4];},4=>p.deadly_walls=0,_=>{}}
        let n=r.build_shader(&info(),&[s],&b,&[],&[],&palette(),&p,&mut out).vertex_count;
        let m=RenderHandle::new().build_shader(&info(),&[s],&b,&[],&[],&palette(),&p,&mut expected).vertex_count;
        assert_eq!(n,m);assert_eq!(&out[..n],&expected[..m]);
    }
}

#[test]
fn inventory_redraw_rainbow_cache_preserves_ice_and_classic_interleaving() {
    let mut r=RenderHandle::new();let s=SnakeRecord {radius:18.0,segment_count:40,flags:flags::FROZEN,frozen_ticks:75,..snake()};
    let b:[SegmentRecord;40]=std::array::from_fn(|j|SegmentRecord {x:1200.0-j as f32*21.24,y:500.0,previous_x:1200.0-j as f32*21.24,previous_y:500.0});
    let event=EventRecord {tick:585,kind:10,snake_id:0,generation:1,duration_ticks:90,..Default::default()};
    let mut out=[ShaderRenderVertex::default();2048];let mut expected=out;let mut fallback=[RenderVertex::default();12000];
    let n=r.build_shader(&info(),&[s],&b,&[],&[event],&palette(),&params(),&mut expected).vertex_count;
    assert!(expected[..n].iter().any(|v|v.params[0]==0 && v.color.red!=151));
    for j in 0..3 {
        if j==1 {r.build(&info(),&[s],&b,&[],&[event],&palette(),&params(),&mut fallback);}
        let m=r.build_shader(&info(),&[s],&b,&[],&[event],&palette(),&params(),&mut out).vertex_count;
        assert_eq!(n,m);assert_eq!(&out[..n],&expected[..m]);
    }
}

fn crate_taper_alpha(n:usize)->u8 {
    (snakes_core::shape::taper(1.0/(n-1) as f64)*255.0).round() as u8
}

#[test]
fn inventory_full_touch_preserves_flight_and_later_compaction() {
    for calm in [false,true] {
        let mut r=RenderHandle::new();r.reduced_motion=calm;let mut i=info();let mut p=params();
        let mut s=SnakeRecord {radius:18.0,segment_count:12,inv_count:3,inv_kind:[3,1,5],inv_life:[255;3],inv_windup:2,..snake()};
        let b:[SegmentRecord;12]=std::array::from_fn(|j|SegmentRecord {x:700.0-j as f32*21.24,y:500.0,previous_x:700.0-j as f32*21.24,previous_y:500.0});
        let mut out=[ShaderRenderVertex::default();1024];
        let start=EventRecord {tick:600,kind:13,snake_id:0,generation:1,cut_index:1,duration_ticks:4,other_snake_id:1,..Default::default()};
        r.build_shader(&i,&[s],&b,&[],&[start],&palette(),&p,&mut out);
        i.tick+=1;i.simulation_time=i.tick as f64/30.0;p.presentation_time=i.simulation_time;
        let touch=EventRecord {tick:i.tick,duration_ticks:0,other_snake_id:4,..start};
        let pickup=EventRecord {kind:2,..touch};
        let n=r.build_shader(&i,&[s],&b,&[],&[touch,pickup],&palette(),&p,&mut out).vertex_count;
        assert_eq!(out[..n].iter().filter(|v|v.params[0]==26 && v.params[3]>=128).count(),6);
        i.tick+=3;i.simulation_time=i.tick as f64/30.0;p.presentation_time=i.simulation_time;
        s.inv_windup=0;s.inv_count=2;s.inv_kind=[3,5,0];
        let held=EventRecord {tick:i.tick,other_snake_id:1,flags:event_flags::HELD_ACTIVATION,..pickup};
        let n=r.build_shader(&i,&[s],&b,&[],&[held],&palette(),&p,&mut out).vertex_count;
        let pip=out[..n].chunks_exact(6).find(|v|v[0].params[0]==26 && v[0].params[1]==5).unwrap();
        let center=(pip[0].x+pip[1].x)*0.5;
        assert!(center<700.0-4.3*18.0-0.01,"remaining gem must retain its closing-gap animation");
    }
}

#[test]
fn inventory_held_completion_and_same_kind_touch_compact_in_order_without_allocations() {
    for calm in [false,true] {for kind in 1..=5 {
        let mut r=RenderHandle::new();r.reduced_motion=calm;let mut i=info();let mut p=params();let pal=palette();
        let mut s=SnakeRecord {radius:18.0,segment_count:12,inv_count:3,inv_kind:[3,kind,5],inv_life:[255;3],inv_windup:2,..snake()};
        let b:[SegmentRecord;12]=std::array::from_fn(|j|SegmentRecord {x:700.0-j as f32*21.24,y:500.0,previous_x:700.0-j as f32*21.24,previous_y:500.0});
        let mut out=[ShaderRenderVertex::default();1024];let mut retry=out;
        let start=EventRecord {tick:600,kind:13,snake_id:0,generation:1,cut_index:1,duration_ticks:4,other_snake_id:kind as u32,..Default::default()};
        COUNT.with(|c|c.set(Some(0)));
        r.build_shader(&i,&[s],&b,&[],&[start],&pal,&p,&mut out);
        i.tick+=4;i.simulation_time=i.tick as f64/30.0;p.presentation_time=i.simulation_time;
        s.inv_windup=0;s.inv_kind=[3,5,2];
        let held=EventRecord {tick:i.tick,kind:2,flags:event_flags::HELD_ACTIVATION,duration_ticks:0,..start};
        let stash=EventRecord {kind:14,flags:0,cut_index:2,other_snake_id:2,x:740.0,y:500.0,..held};
        let touch=EventRecord {kind:13,flags:0,cut_index:u16::MAX,..held};
        let pickup=EventRecord {kind:2,..touch};let events=[held,stash,touch,pickup];
        let n=r.build_shader(&i,&[s],&b,&[],&events,&pal,&p,&mut out).vertex_count;
        let m=r.build_shader(&i,&[s],&b,&[],&events,&pal,&p,&mut retry).vertex_count;
        assert_eq!(&out[..n],&retry[..m]);
        let pip=out[..n].chunks_exact(6).find(|v|v[0].params[0]==26 && v[0].params[1]==5).unwrap();
        let center=(pip[0].x+pip[1].x)*0.5;
        assert!(center<700.0-4.3*18.0-0.01,"surviving gem must close the gap despite a later same-kind touch");
        assert_eq!(COUNT.with(|c|c.replace(None).unwrap()),0);
    }}
}

#[test]
fn inventory_stash_after_fizzle_resets_empty_slot_slide_in_calm() {
    let mut r=RenderHandle::new();r.reduced_motion=true;let i=info();let p=params();
    let s=SnakeRecord {radius:18.0,segment_count:12,inv_count:1,inv_kind:[2,0,0],inv_life:[255,0,0],..snake()};
    let b:[SegmentRecord;12]=std::array::from_fn(|j|SegmentRecord {x:700.0-j as f32*21.24,y:500.0,previous_x:700.0-j as f32*21.24,previous_y:500.0});
    let fizzle=EventRecord {tick:i.tick,kind:15,other_snake_id:3,snake_id:0,generation:1,x:660.4,y:500.0,duration_ticks:8,..Default::default()};
    let stash=EventRecord {kind:14,other_snake_id:2,..fizzle};let mut out=[ShaderRenderVertex::default();1024];
    let n=r.build_shader(&i,&[s],&b,&[],&[fizzle,stash],&palette(),&p,&mut out).vertex_count;
    let pip=out[..n].chunks_exact(6).find(|v|v[0].params[0]==26 && v[0].params[1]==2).unwrap();
    assert!(((pip[0].x+pip[1].x)*0.5-(700.0-2.2*18.0)).abs()<0.001);
}

#[test]
fn inventory_same_tick_and_future_configuration_fizzles_survive_retries_and_empty_next_export() {
    for calm in [false,true] {for future in [false,true] {
        let mut r=RenderHandle::new();r.reduced_motion=calm;let mut i=info();let mut p=params();let mut s=snake();
        s.inv_count=1;s.inv_kind=[3,0,0];s.inv_life=[255,0,0];let b=body();let mut out=[ShaderRenderVertex::default();512];let mut retry=out;
        let stash=EventRecord {tick:i.tick,kind:14,snake_id:0,generation:1,other_snake_id:3,x:185.0,y:200.0,..Default::default()};
        r.build_shader(&i,&[s],&b,&[],&[stash],&palette(),&p,&mut out);
        s.inv_count=0;s.inv_kind=[0;3];let fizzle=EventRecord {kind:15,tick:i.tick+u64::from(future),duration_ticks:8,..stash};
        let n=r.build_shader(&i,&[s],&b,&[],&[fizzle],&palette(),&p,&mut out).vertex_count;
        let m=r.build_shader(&i,&[s],&b,&[],&[fizzle],&palette(),&p,&mut retry).vertex_count;
        assert_eq!(n,m);assert_eq!(&out[..n],&retry[..m]);
        if !future {assert_eq!(out[..n].iter().filter(|v|v.params[0]==26).count(),6);}
        i.tick+=1;i.simulation_time=i.tick as f64/30.0;p.presentation_time=i.simulation_time;
        let n=r.build_shader(&i,&[s],&b,&[],&[],&palette(),&p,&mut out).vertex_count;
        assert_eq!(out[..n].iter().filter(|v|v.params[0]==26).count(),6);
        assert!(out[..n].iter().any(|v|v.params[0]==13));
    }}
}

#[test]
fn inventory_compact_history_resolves_nova_against_full_heads() {
    let mut r=RenderHandle::new();let i=info();let p=params();let mut out=[ShaderRenderVertex::default();512];
    let snakes:[SnakeRecord;3]=std::array::from_fn(|id|SnakeRecord {id:id as u32,segment_offset:id as u32*3,flags:if id==2 {flags::FROZEN} else {0},frozen_ticks:if id==2 {75} else {0},..snake()});
    let b:[SegmentRecord;9]=std::array::from_fn(|j| {let x=[200.0,1000.0,1010.0][j/3]-(j%3) as f32*8.0;SegmentRecord {x,y:200.0,previous_x:x,previous_y:200.0}});
    let compact:[SnakeRecord;3]=std::array::from_fn(|id|SnakeRecord {segment_offset:id as u32,segment_count:1,..snakes[id]});
    let tails:[SegmentRecord;3]=std::array::from_fn(|id|b[id*3+2]);
    let events:[EventRecord;2]=std::array::from_fn(|id|EventRecord {tick:i.tick,kind:3,snake_id:id as u32,generation:1,x:[200.0,1000.0][id],y:200.0,value:128.0,duration_ticks:21,..Default::default()});
    let history=RenderParams {scale_x:0.0,scale_y:0.0,..p};
    r.build_shader(&i,&compact,&tails,&[],&events,&palette(),&history,&mut []);
    let n=r.build_shader(&i,&snakes,&b,&[],&[],&palette(),&p,&mut out).vertex_count;
    assert!(out[..n].iter().any(|v|v.params[0]==0 && v.x>990.0 && v.params[1]&128!=0));
}

#[test]
fn inventory_slipstream_budget_counts_visible_ribbons() {
    let mut r=RenderHandle::new();let mut out=[ShaderRenderVertex::default();4096];
    // Earlier heads are invisible, but their tails admit all four snakes.
    let snakes:[SnakeRecord;5]=std::array::from_fn(|id|SnakeRecord {id:id as u32,segment_offset:id as u32*16,segment_count:16,effect_kind:1,effect_ticks:90,..snake()});
    let b:[SegmentRecord;80]=std::array::from_fn(|j| {let id=j/16;let k=j%16;let x=if id==4 {500.0-k as f32*9.44} else if k==15 {100.0} else {-500.0-k as f32*9.44};SegmentRecord {x,y:200.0+id as f32*100.0,previous_x:x,previous_y:200.0+id as f32*100.0}});
    let n=r.build_shader(&info(),&snakes,&b,&[],&[],&palette(),&params(),&mut out).vertex_count;
    assert!(out[..n].iter().any(|v|v.params[0]==27 && v.x>350.0));
}

#[test]
fn inventory_gulp_slipstream_wrap_and_viewport_bounds_cover_actual_extrusion() {
    for wrap in [false,true] {
        let mut i=info();i.world_height=500.0;let mut p=params();p.viewport_height=500.0;p.interpolation=1.0;p.deadly_walls=u32::from(!wrap);
        let mut s=SnakeRecord {radius:18.0,segment_count:24,effect_kind:1,effect_ticks:90,..snake()};
        // A full-strength feeding pulse centred on the head widens the normal.
        s.bulges[0]=snakes_core::Bulge {start_tick:i.tick-3,duration_ticks:30,strength:1.0,..Default::default()};
        let y=if wrap {2.2*18.0} else {-2.2*18.0};
        let b:[SegmentRecord;24]=std::array::from_fn(|j|SegmentRecord {x:700.0-j as f32*21.24,y:y as f32,previous_x:700.0-j as f32*21.24,previous_y:y as f32});
        let mut out=[ShaderRenderVertex::default();2048];
        let n=RenderHandle::new().build_shader(&i,&[s],&b,&[],&[],&palette(),&p,&mut out).vertex_count;
        assert!(out[..n].iter().any(|v|v.params[0]==27 && if wrap {v.y>p.viewport_height as f32} else {v.y>0.0}),"feeding streak must survive edge culling / receive seam copy");
    }
}

#[test]
fn inventory_phase_fade_preserves_adult_head_neck_contour() {
    let mut r=RenderHandle::new();let mut i=info();let mut p=params();
    let s=SnakeRecord {radius:18.0,segment_count:24,effect_kind:3,effect_ticks:120,..snake()};
    let b:[SegmentRecord;24]=std::array::from_fn(|j|SegmentRecord {x:700.0-j as f32*21.24,y:500.0,previous_x:700.0-j as f32*21.24,previous_y:500.0});
    let event=EventRecord {tick:600,kind:2,snake_id:0,generation:1,other_snake_id:3,..Default::default()};
    let mut out=[ShaderRenderVertex::default();1024];let expected=crate_taper_alpha(24);
    let contour=|alpha:u8| {
        let neck=alpha as f64/255.0/1.14;let x=(-0.9-0.05)/1.37_f64;
        let spade=1.10*(1.0-x*x).max(0.0).sqrt();let k=0.15;let h=(0.5+0.5*(spade-neck)/k).clamp(0.0,1.0);
        neck*(1.0-h)+spade*h+k*h*(1.0-h)
    };
    let reference=contour(expected);assert!((reference-0.8484).abs()<0.0001);
    for tick in 600..=604 {
        i.tick=tick;i.simulation_time=tick as f64/30.0;p.presentation_time=i.simulation_time;
        let events=if tick==600 {&[event][..]} else {&[][..]};
        let n=r.build_shader(&i,&[s],&b,&[],events,&palette(),&p,&mut out).vertex_count;
        let head=out[..n].iter().find(|v|v.params[0]==1 || v.params[0]>=29).unwrap();
        assert_eq!(head.color.alpha,expected);assert_eq!(contour(head.color.alpha),reference);
    }
    let vertex=include_str!("../../../src/shaders/snake.vert");let fragment=include_str!("../../../src/shaders/snake.frag");
    assert!(vertex.contains("phaseFade=float(kind-29)/226.0"));
    assert!(fragment.contains("float neck=base.a/hr"));
    assert!(fragment.contains("phaseHead?mix(1.0,0.45,phaseFade)"));
}

#[test]
fn inventory_retained_nova_cannot_claim_a_later_freeze() {
    let mut r=RenderHandle::new();let mut i=info();let mut p=params();let mut out=[ShaderRenderVertex::default();512];
    let mut snakes:[SnakeRecord;3]=std::array::from_fn(|id|SnakeRecord {id:id as u32,segment_offset:id as u32*3,..snake()});
    let mut b:[SegmentRecord;9]=std::array::from_fn(|j| {let x=[200.0,330.0,330.0][j/3]-(j%3) as f32*8.0;SegmentRecord {x,y:200.0+(j/3) as f32*100.0,previous_x:x,previous_y:200.0+(j/3) as f32*100.0}});
    let first=EventRecord {tick:600,kind:3,snake_id:0,generation:1,x:200.0,y:400.0,value:128.0,duration_ticks:21,..Default::default()};
    r.build_shader(&i,&snakes,&b,&[],&[first],&palette(),&p,&mut out);
    i.tick=603;i.simulation_time=603.0/30.0;p.presentation_time=i.simulation_time;
    for seg in &mut b[6..] {seg.x-=4.0;seg.previous_x=seg.x;}
    snakes[2].flags=flags::FROZEN;snakes[2].frozen_ticks=75;
    let second=EventRecord {tick:603,snake_id:1,x:330.0,..first};
    let n=r.build_shader(&i,&snakes,&b,&[],&[second],&palette(),&p,&mut out).vertex_count;
    assert!(out[..n].iter().any(|v|v.params[0]==0 && v.y>380.0 && v.params[1]&128!=0));
}

#[test]
fn inventory_pending_configuration_fizzle_does_not_advance_other_event_cutoff() {
    let mut r=RenderHandle::new();let mut i=info();let mut p=params();let mut s=snake();let b=body();
    let mut out=[ShaderRenderVertex::default();512];
    let future=EventRecord {tick:601,kind:15,snake_id:0,generation:1,other_snake_id:3,duration_ticks:8,x:180.0,y:200.0,..Default::default()};
    r.build_shader(&i,&[s],&b,&[],&[future],&palette(),&p,&mut out);
    i.tick=601;i.simulation_time=601.0/30.0;p.presentation_time=i.simulation_time;
    s.inv_count=1;s.inv_kind=[1,0,0];s.inv_life=[255,0,0];s.inv_windup=1;
    let use_event=EventRecord {kind:13,other_snake_id:1,duration_ticks:4,..future};
    let n=r.build_shader(&i,&[s],&b,&[],&[use_event],&palette(),&p,&mut out).vertex_count;
    assert_eq!(out[..n].iter().filter(|v|v.params[0]==26 && v.params[3]>=128).count(),6);
}
