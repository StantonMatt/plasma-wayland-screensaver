// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::controller::{ BaselineController, ScriptedController, Steering };
fn world() -> World {
    World::new(Config {
        seed: 73,
        ..Config::default()
    }).unwrap()
}
fn only(w: &mut World, count: usize) {
    w.snakes.truncate(count);
    w.food.clear();
}
fn line(w: &mut World, i: usize, p: Point, angle: f64, len: usize) {
    let s = &mut w.snakes[i];
    s.alive = true;
    s.len = len;
    s.angle = angle;
    s.desired = angle;
    s.growth = 0.0;
    s.stretch = 0.0;
    let spacing = s.radius*1.18;
    for j in 0..len {
        let q = w.config.geometry().wrap(Point {
            x: p.x-angle.cos()*spacing*j as f64,
            y: p.y-angle.sin()*spacing*j as f64
        });
        w.segments[i*MAX_SEGMENTS+j] = Segment {
            current: q,
            previous: q
        };
    }
    w.rebuild_trail(i);
}
fn food(w: &mut World, p: Point, value: f64, life: f64) {
    w.add_food(Food {
        p,
        value,
        life,
        ..Food::default()
    });
}
fn close(a: f64, b: f64) {
    assert!((a-b).abs()<0.00001,"{a} != {b}");
}
#[test] fn initial_population_and_clearance() {
    let w = world();
    assert_eq!(w.snake_count(), 9);
    assert_eq!(w.food.len(), 82);
    // VisualUtils.colors provides six entries for every built-in JS palette.
    assert_eq!(w.snakes().map(|s|s.color_index).collect::<Vec<_>>(), vec![0, 1, 2, 3, 4, 5, 0, 1, 2]);
    assert!(w.foods().all(|f|f.color_index<6));
    for (i, s) in w.snakes.iter().enumerate() {
        assert!((14..=32).contains(&s.len));
        assert_eq!(s.generation, 1);
        for seg in w.snake(i).unwrap().segments {
            assert!(seg.current.x>0.0 && seg.current.x<w.config.width && seg.current.y>0.0 && seg.current.y<w.config.height);
            assert_eq!(seg.current, seg.previous);
        }
        for j in 0..i {
            for seg in w.snake(j).unwrap().segments {
                assert!(w.distance_squared(w.segments[i*MAX_SEGMENTS].current, seg.current).sqrt()>s.radius*6.0);
            }
        }
    }
}
#[test] fn food_expiry_replenishes_at_most_three_and_bounces() {
    let mut w = world();
    w.food.clear();
    food(&mut w, Point {
        x: 300.0,
        y: 300.0
    }, 1.0, 0.001);
    let id = w.food[0].id;
    w.update_food(0.01);
    assert_eq!(w.food.len(), 3);
    assert!(w.food.iter().all(|f|f.id!=id && f.life>=34.0 && f.life<=46.0));
    w.food[0].p.x = 2.1;
    w.food[0].velocity.x =  -200.0;
    w.update_food(0.01);
    assert_eq!(w.food[0].p.x, 2.0);
    assert!(w.food[0].velocity.x>0.0);
}
#[test] fn body_replays_l_shaped_head_path() {
    let mut w = world();
    only(&mut w, 1);
    let len = w.snakes[0].len;
    line(&mut w, 0, Point {
        x: 500.0,
        y: 320.0
    }, 0.0, len);
    for _ in 0..8 {
        w.move_snake(0, 1.0/60.0);
    }
    let corner = w.segments[0].current.x;
    w.snakes[0].angle = std::f64::consts::FRAC_PI_2;
    w.snakes[0].desired = std::f64::consts::FRAC_PI_2;
    for _ in 0..12 {
        w.move_snake(0, 1.0/60.0);
    }
    for seg in &w.segments[1..len] {
        assert!((seg.current.x-corner).abs()<0.02 || (seg.current.y-320.0).abs()<0.02);
    }
    close(w.segments[len-1].current.y, 320.0);
}
#[test] fn growth_inserts_neck_and_preserves_previous_correspondence() {
    let mut w = world();
    only(&mut w, 1);
    line(&mut w, 0, Point {
        x: 500.0,
        y: 320.0
    }, 0.0, 25);
    w.snakes[0].growth = 2.2;
    let radius = w.snakes[0].radius;
    for _ in 0..90 {
        let len = w.snakes[0].len;
        let previous = w.segments[1..len].to_vec();
        w.move_snake(0, 1.0/60.0);
        if w.snakes[0].len>len {
            for (j, old) in previous.iter().enumerate() {
                assert_eq!(w.segments[j+2].previous, old.current);
            }
            let p = w.segments[1].previous;
            assert_eq!(p, w.segments[0].current);
        }
    }
    assert_eq!(w.snakes[0].len, 27);
    assert!(w.snakes[0].radius>radius);
    assert!(w.distance_squared(w.segments[25].current, w.segments[26].current).sqrt()>w.snakes[0].radius*0.9);
}
#[test] fn vacuum_capture_lock_release_and_growth_storage() {
    let mut w = world();
    only(&mut w, 1);
    let head = w.segments[0].current;
    let radius = w.snakes[0].radius;
    food(&mut w, Point {
        x: head.x+radius*2.75,
        y: head.y
    }, 1.0, 37.0);
    w.food[0].size = 1.0;
    let x = w.food[0].p.x;
    w.feed_snakes(1.0/60.0);
    assert_eq!(w.food.len(), 1);
    assert!(w.food[0].p.x<x);
    assert!(w.food[0].attraction>0.0);
    assert_eq!(w.food[0].owner, 0);
    assert_eq!(w.food[0].life, -1.0);
    w.segments[0].current.x+=150.0;
    w.segments[0].current.y+=90.0;
    for _ in 0..180 {
        w.segments[0].current.x+=1.0;
        w.feed_snakes(1.0/60.0);
    }
    assert!(w.food.is_empty());
    close(w.snakes[0].growth, 1.0);
    let p = w.segments[0].current;
    food(&mut w, Point {
        x: p.x+radius*2.7,
        y: p.y
    }, 1.0, 37.0);
    w.feed_snakes(1.0/60.0);
    w.snakes[0].alive = false;
    w.feed_snakes(1.0/60.0);
    assert_eq!(w.food[0].owner, -1);
    close(w.food[0].life, 37.0);
    w.snakes[0].alive = true;
    w.food.clear();
    food(&mut w, p, 200.0, -1.0);
    w.feed_snakes(STEP_SECONDS);
    close(w.snakes[0].growth, 12.0*World::growth_cost(&w.snakes[0]));
}
#[test] fn wrap_spacing_vacuum_and_narrow_final_bin() {
    let mut w = world();
    w.config.deadly_walls = false;
    only(&mut w, 2);
    line(&mut w, 0, Point {
        x: 1279.75,
        y: 200.0
    }, 0.0, 25);
    line(&mut w, 1, Point {
        x: 900.0,
        y: 400.0
    }, 0.0, 25);
    let spacing = w.snakes[0].radius*1.18;
    w.move_snake(0, STEP_SECONDS);
    assert!(w.segments[0].current.x<20.0);
    close(w.distance_squared(w.segments[0].current, w.segments[1].current).sqrt(), spacing);
    w.segments[0] = Segment {
        current: Point {
            x: 1.0,
            y: 200.0
        },
        previous: Point {
            x: 1.0,
            y: 200.0
        }
    };
    let radius = w.snakes[0].radius;
    food(&mut w, Point {
        x: 1280.0-radius,
        y: 200.0
    }, 1.0, -1.0);
    w.food[0].size = 1.0;
    w.feed_snakes(1.0/60.0);
    assert!(w.food.is_empty());
    w.snakes[0].radius = 10.0;
    w.snakes[1].radius = 10.0;
    for j in 0..25 {
        let p = Point {
            x: 300.0,
            y: 1.0
        };
        w.segments[j] = Segment {
            current: p,
            previous: p
        };
    }
    let p = Point {
        x: 300.0,
        y: 711.0
    };
    w.segments[MAX_SEGMENTS+5] = Segment {
        current: p,
        previous: p
    };
    w.mark_collisions();
    assert_eq!(w.snakes[0].dying, DeathReason::Body);
}
#[test] fn head_on_size_and_less_than_four_tie_rule() {
    for difference in [-4, -3, 0, 3, 4] {
        let mut w = world();
        only(&mut w, 2);
        line(&mut w, 0, Point {
            x: 500.0,
            y: 300.0
        }, 0.0, (25+difference) as usize);
        line(&mut w, 1, Point {
            x: 500.0,
            y: 300.0
        }, std::f64::consts::PI, 25);
        for j in 1..w.snakes[0].len {
            let p = Point {
                x: 100.0,
                y: 100.0
            };
            w.segments[j] = Segment {
                current: p,
                previous: p
            };
        }
        for j in 1..w.snakes[1].len {
            let p = Point {
                x: 900.0,
                y: 600.0
            };
            w.segments[MAX_SEGMENTS+j] = Segment {
                current: p,
                previous: p
            };
        }
        w.mark_collisions();
        if difference<4 {
            assert_eq!(w.snakes[0].dying, DeathReason::Head);
        }
        if difference> -4 {
            assert_eq!(w.snakes[1].dying, DeathReason::Head);
        }
        if difference==4 {
            assert_eq!(w.snakes[0].dying, DeathReason::None);
        }
        if difference== -4 {
            assert_eq!(w.snakes[1].dying, DeathReason::None);
        }
    }
}
#[test] fn swept_head_on_and_body_hits_neck_included() {
    let mut w = world();
    only(&mut w, 2);
    line(&mut w, 0, Point {
        x: 200.0,
        y: 360.0
    }, 0.0, 25);
    line(&mut w, 1, Point {
        x: 900.0,
        y: 360.0
    }, std::f64::consts::PI, 25);
    let p = Point {
        x: 600.0,
        y: 360.0
    };
    w.segments[MAX_SEGMENTS+5] = Segment {
        current: p,
        previous: p
    };
    w.segments[0] = Segment {
        current: Point {
            x: 630.0,
            y: 360.0
        },
        previous: Point {
            x: 570.0,
            y: 360.0
        }
    };
    w.mark_collisions();
    assert_eq!(w.snakes[0].dying, DeathReason::Body);
    w.segments[0] = w.segments[MAX_SEGMENTS+2];
    w.mark_collisions();
    assert_eq!(w.snakes[0].dying, DeathReason::Body);
    w.segments[0] = Segment {
        current: Point {
            x: 550.0,
            y: 200.0
        },
        previous: Point {
            x: 450.0,
            y: 200.0
        }
    };
    w.segments[MAX_SEGMENTS] = Segment {
        current: Point {
            x: 450.0,
            y: 200.0
        },
        previous: Point {
            x: 550.0,
            y: 200.0
        }
    };
    w.mark_collisions();
    assert_eq!(w.snakes[0].dying, DeathReason::Head);
    assert_eq!(w.snakes[1].dying, DeathReason::Head);
}
#[test] fn self_collision_excludes_first_ten_segments() {
    let mut w = world();
    only(&mut w, 1);
    line(&mut w, 0, Point {
        x: 500.0,
        y: 300.0
    }, 0.0, 25);
    w.segments[0] = w.segments[12];
    w.mark_collisions();
    assert_eq!(w.snakes[0].dying, DeathReason::None);
    w.config.self_collisions = true;
    w.mark_collisions();
    assert_eq!(w.snakes[0].dying, DeathReason::SelfHit);
    for j in 10..25 {
        let p = Point {
            x: 900.0,
            y: 600.0
        };
        w.segments[j] = Segment {
            current: p,
            previous: p
        };
    }
    w.segments[0] = w.segments[5];
    w.mark_collisions();
    assert_eq!(w.snakes[0].dying, DeathReason::None);
}
#[test] fn maturity_turn_radius_length_penalty_rush_and_radius_cap() {
    let mut w = world();
    only(&mut w, 1);
    line(&mut w, 0, Point {
        x: 800.0,
        y: 300.0
    }, 0.0, 20);
    let short = World::minimum_turn_radius(&w.snakes[0])/w.snakes[0].radius;
    let fast = w.speed(&w.snakes[0]);
    w.snakes[0].len = 400;
    World::update_radius(&mut w.snakes[0]);
    let s = w.snakes[0];
    close(s.radius/s.base_radius, 1.25);
    assert!(World::minimum_turn_radius(&s)/s.radius>short);
    assert!(w.speed(&s)<fast);
    w.snakes[0].desired = std::f64::consts::PI;
    let speed = w.speed(&s);
    w.move_snake(0, STEP_SECONDS);
    assert!(speed/(w.snakes[0].angle.abs()/STEP_SECONDS)>=World::minimum_turn_radius(&s)-0.1);
    let s = w.snakes[0];
    w.snakes[0].rush = 0.4;
    close(w.speed(&w.snakes[0]), w.speed(&s)*1.4);
}
#[test] fn per_snake_and_world_caps_block_growth_not_motion() {
    let mut w = world();
    only(&mut w, 1);
    let cap = w.maximum_snake_segments(&w.snakes[0]);
    line(&mut w, 0, Point {
        x: 800.0,
        y: 300.0
    }, 0.0, cap);
    w.snakes[0].growth = 100.0;
    w.growth_slots = 6000;
    let old = w.segments[0].current;
    w.move_snake(0, STEP_SECONDS);
    assert_eq!(w.snakes[0].len, cap);
    assert!(w.snakes[0].blocked);
    assert_ne!(w.segments[0].current, old);
    line(&mut w, 0, Point {
        x: 800.0,
        y: 300.0
    }, 0.0, 25);
    w.snakes[0].growth = 100.0;
    w.growth_slots = 0;
    w.move_snake(0, STEP_SECONDS);
    assert_eq!(w.snakes[0].len, 25);
    assert!(w.snakes[0].blocked);
}
#[test] fn death_burst_cap_preserves_nutrition_and_locked_food() {
    let mut w = world();
    only(&mut w, 1);
    line(&mut w, 0, Point {
        x: 700.0,
        y: 300.0
    }, 0.0, 240);
    while w.food.len()<w.config.maximum_food() {
        w.add_ambient_food();
    }
    let locked = w.food[0].id;
    w.food[0].owner = 0;
    w.snakes[0].dying = DeathReason::SelfHit;
    let emitted = w.explode_snake(0);
    assert!(emitted>=48);
    assert!(w.food.len()<=w.config.maximum_food());
    assert!(w.food.iter().any(|f|f.id==locked));
    assert_eq!(w.food.iter().filter(|f|f.feast>0).count(), emitted);
    assert!(w.food.iter().filter(|f|f.feast>0).map(|f|f.value).sum::<f64>()>200.0);
    assert!(!w.snakes[0].alive);
    assert_eq!(w.snakes[0].len, 0);
    assert!((2.2..=5.4).contains(&w.snakes[0].respawn));
    assert_eq!(w.stats().self_deaths, 1);
    w.food.clear();
    w.snakes[0].len = 240;
    assert!(w.explode_snake(0)>180);
}
#[test] fn respawn_reuses_id_increments_generation_and_checks_clearance() {
    let mut w = world();
    let generation = w.snakes[0].generation;
    w.explode_snake(0);
    w.snakes[0].respawn = 0.001;
    let mut controller = ScriptedController::new(|_, s: SnakeView<'_>|Steering {
        desired_angle: s.angle,
        rush: 0.0
    });
    w.step(&mut controller);
    let s = w.snake(0).unwrap();
    assert_eq!(s.id, 0);
    assert_eq!(s.generation, generation+1);
    assert!(s.alive);
    assert_eq!(s.segments[0].current, s.segments[0].previous);
    for other in w.snakes().skip(1).filter(|s|s.alive) {
        for seg in other.segments {
            assert!(w.distance_squared(s.segments[0].current, seg.current).sqrt()>s.radius*6.0);
        }
    }
}
#[test] fn resize_scales_current_previous_trail_food_velocity_and_targets() {
    let mut w = world();
    w.food.clear();
    line(&mut w, 0, Point {
        x: 500.0,
        y: 300.0
    }, 0.0, 25);
    food(&mut w, Point {
        x: 300.0,
        y: 200.0
    }, 1.0, 37.0);
    w.food[0].velocity = Point {
        x: 10.0,
        y: 20.0
    };
    let old = w.segments[0];
    let r = w.snakes[0].radius;
    let t = w.trail_point(0, 0);
    let f = w.food[0];
    let generation = w.geometry_generation;
    w.resize(2560.0, 1080.0).unwrap();
    close(w.segments[0].current.x, old.current.x*2.0);
    close(w.segments[0].previous.y, old.previous.y*1.5);
    close(w.snakes[0].radius, r);
    close(w.trail_point(0, 0).p.x, t.p.x*2.0);
    close(w.food[0].p.y, f.p.y*1.5);
    close(w.food[0].velocity.x, 20.0);
    close(w.food[0].target.y, f.target.y*1.5);
    assert_eq!(w.geometry_generation, generation+1);
    let cfg = w.config;
    assert!(w.resize(f64::NAN, 100.0).is_err());
    assert_eq!(w.config, cfg);
}
#[test] fn ring_pruning_and_wraparound_preserve_placement() {
    let mut w = world();
    only(&mut w, 1);
    w.config.deadly_walls = false;
    line(&mut w, 0, Point {
        x: 600.0,
        y: 300.0
    }, 0.0, 25);
    // Exercise physical ring wrap without millions of simulation ticks.
    let len = w.snakes[0].trail_len;
    let saved: (Vec<_>, usize) = ((0..len).map(|j|w.trail_point(0, j)).collect(), w.trail_capacity-2);
    w.snakes[0].trail_start = saved.1;
    for (j, t) in saved.0.iter().enumerate() {
        let idx = w.trail_index(0, j);
        w.trails[idx] = *t;
    }
    for _ in 0..100 {
        w.move_snake(0, STEP_SECONDS);
    }
    let spacing = w.snakes[0].radius*1.18;
    for j in 1..25 {
        close(w.distance_squared(w.segments[j-1].current, w.segments[j].current).sqrt(), spacing);
    }
    assert!(w.snakes[0].trail_len<200);
}
#[test] fn invalid_controls_do_not_poison_world_and_reconfigure_count_resets() {
    let mut w = world();
    let angle = w.snakes[0].angle;
    let mut c = ScriptedController::new(|_, _: SnakeView<'_>|Steering {
        desired_angle: f64::NAN,
        rush: 2.0
    });
    w.step(&mut c);
    assert_eq!(w.snakes[0].angle, normalize_angle(angle));
    w.reconfigure(Config {
        density: 100.0,
        ..w.config
    }).unwrap();
    assert_eq!(w.snake_count(), 14);
    assert_eq!(w.tick, 0);
    assert_eq!(w.time, 0.0);
    w.step(&mut BaselineController);
}
// Test-only cap fixture: also compiled directly by the standalone allocator
// integration binary. It is never part of the library's public API.
pub(crate) fn allocation_fixture() -> World {
    let mut w = World::new(Config {
        width: 3440.0,
        height: 1440.0,
        density: 100.0,
        trails: 1000.0,
        scale: 25.0,
        deadly_walls: false,
        ..Config::default()
    }).unwrap();
    let cap = w.config.maximum_world_segments();
    assert_eq!(cap, 6000);
    for i in 0..MAX_SNAKES {
        let remainder = cap - MAX_SEGMENTS;
        let len = if i == 0 {MAX_SEGMENTS} else {
            remainder/(MAX_SNAKES-1)+usize::from(i-1<remainder%(MAX_SNAKES-1))
        };
        w.snakes[i].radius = w.snakes[i].base_radius*1.25;
        line(&mut w, i, Point {x: 3200.0, y: 50.0+i as f64*95.0}, 0.0, len);
        w.snakes[i].growth = 100.0;
    }
    refill_food_to_cap(&mut w);
    assert_eq!(w.food.len(), 450);
    w
}
#[test] fn cap_fixture_is_finite() {
    let w = allocation_fixture();
    assert_eq!(w.stats().total_segments, 6000);
}
#[path = "golden.rs"] mod golden;
#[test] fn density_shrink_and_grow_do_not_reuse_generation() {
    let mut w = World::new(Config {
        density: 100.0,
        ..Config::default()
    }).unwrap();
    let old = w.snake(13).unwrap().generation;
    w.reconfigure(Config {
        density: 0.0,
        ..w.config
    }).unwrap();
    assert_eq!(w.snake_count(), 3);
    w.reconfigure(Config {
        density: 100.0,
        ..w.config
    }).unwrap();
    assert!(w.snake(13).unwrap().generation>old);
}
#[test] fn toroidal_segment_math_and_touching_intersections() {
    let w = World::new(Config {
        deadly_walls: false,
        ..Config::default()
    }).unwrap();
    close(w.segment_distance_squared(Point {
        x: 1.0,
        y: 300.0
    }, Point {
        x: 1275.0,
        y: 290.0
    }, Point {
        x: 5.0,
        y: 310.0
    }), 0.8);
    close(w.segments_distance_squared(Point {
        x: 1275.0,
        y: 300.0
    }, Point {
        x: 5.0,
        y: 300.0
    }, Point {
        x: 1.0,
        y: 290.0
    }, Point {
        x: 1.0,
        y: 310.0
    }), 0.0);
    close(w.segments_distance_squared(Point {
        x: 100.0,
        y: 100.0
    }, Point {
        x: 110.0,
        y: 100.0
    }, Point {
        x: 110.0,
        y: 100.0
    }, Point {
        x: 110.0,
        y: 110.0
    }), 0.0);
    assert_eq!(crate::wrap_coordinate(-2561.0, 1280.0), 1279.0);
}

pub(crate) fn refill_food_to_cap(w: &mut World) {
    while w.food.len() < w.config.maximum_food() {w.add_ambient_food();}
}

// QML's appendGrowthSegment is a test construction helper, not the runtime
// growth path. Keep its tail extension and interpolation behavior in fixtures.
fn append_growth_segment(w: &mut World, i: usize) {
    let s = w.snakes[i];
    assert!(s.len > 0 && s.len < MAX_SEGMENTS);
    let tail = w.segments[i*MAX_SEGMENTS+s.len-1].current;
    let mut direction = if s.len > 1 {
        w.config.geometry().delta(w.segments[i*MAX_SEGMENTS+s.len-2].current, tail)
    } else {Point {x: -s.angle.cos(), y: -s.angle.sin()}};
    let length = (direction.x*direction.x+direction.y*direction.y).sqrt();
    if length < 0.001 {direction = Point {x: -s.angle.cos(), y: -s.angle.sin()};}
    else {direction.x /= length; direction.y /= length;}
    let p = w.config.geometry().wrap(Point {
        x: tail.x+direction.x*s.radius*1.18,
        y: tail.y+direction.y*s.radius*1.18,
    });
    w.segments[i*MAX_SEGMENTS+s.len] = Segment {current: p, previous: p};
    w.snakes[i].len += 1;
    w.snakes[i].trail_len = 0;
    World::update_radius(&mut w.snakes[i]);
}
#[test] fn larger_snakes_create_more_death_food() {
    let mut w = world();
    only(&mut w, 2);
    for _ in 0..45 {append_growth_segment(&mut w, 1);}
    let small = w.explode_snake(0);
    w.food.clear();
    let large = w.explode_snake(1);
    assert!(large > small*2);
}
