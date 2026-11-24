use super::*;
use crate::visuals::depth::common::{
    flow::FlowSettings,
    layout::{Layout, OutputRect},
};

include!("still_306_fixture.rs");

#[test]
fn angular_transport_retains_length_and_curvature_within_the_motion_budget() {
    for allowance in [0., 0.5, 3., 20.] {
        let mut r = still_306_fixture();
        r.count = 64;
        r.spacing = 8.;
        r.nodes[0].pos = p(0., 0.);
        for i in 1..r.count {
            r.nodes[i].pos = r.nodes[i - 1].pos + polar(0.02 * i as f64) * r.spacing;
        }
        let old = r.nodes;
        r.nodes[0].pos = p(1., 0.5);
        for i in 1..r.count {
            r.nodes[i].pos = r.nodes[i - 1].pos + polar(0.7 - 0.02 * i as f64) * r.spacing;
        }
        let maximum = r.nodes[0].pos.length() + allowance;
        r.transport_body(&old, maximum);
        for (a, b) in r.nodes[..r.count].iter().zip(old) {
            assert!((a.pos - b.pos).length() <= maximum + 1e-9);
        }
        for pair in r.nodes[..r.count].windows(2) {
            assert!(((pair[1].pos - pair[0].pos).length() - 8.).abs() < 1e-10);
        }
        for triple in r.nodes[..r.count].windows(3) {
            let a = triple[1].pos - triple[0].pos;
            let b = triple[2].pos - triple[1].pos;
            assert!(turn(a, b).abs() <= 0.020000001);
        }
    }
}

#[test]
fn natural_still_cannot_whip_strand_306_node_65() {
    let mut r = still_306_fixture();
    let bounds = Rect {
        x: 0.,
        y: 0.,
        width: 7920.,
        height: 1440.,
    };
    let layout = Layout::new(
        vec![OutputRect {
            id: 1,
            rect: bounds,
            primary: true,
        }],
        1,
    )
    .unwrap();
    let water = FlowField::new(1, &layout, FlowSettings::ribbons(0.4, 3.));
    let lanes = super::super::water::Lanes::new(bounds);
    let before = r.nodes;
    let flow = r.velocities.iter().map(|v| v.length()).fold(0., f64::max);
    let pressure = r.pressure.iter().map(|v| v.length()).fold(0., f64::max);
    let dt = 1. / 30.;
    r.swim(
        Steering {
            desired: p(117.62776739659193, 52.374905707086995),
            curl: -0.008232492317599824,
            gust: 0.,
            gust_force_limit: f64::INFINITY,
            maelstrom_motor_limit: f64::INFINITY,
            launch: 0.,
            gust_front: None,
            turbulence: 0.4,
            still: 0.39078874074074127,
            smooth_twist: true,
            spanning: true,
            braid_phase: None,
            orbit: None,
        },
        &water,
        &lanes,
        0.55,
        dt,
        false,
    );
    let head = (r.nodes[0].pos - before[0].pos).length() / dt;
    let allowance = (r.speed + 46.) * r.spacing / 300.;
    let envelope = 2. * (head + flow + pressure + allowance);
    let point = (r.nodes[65].pos - before[65].pos).length() / dt;
    let maximum = r.nodes[..r.count]
        .iter()
        .zip(before)
        .map(|(a, b)| (a.pos - b.pos).length() / dt)
        .fold(0., f64::max);
    println!(
        "Still 306/65: head={head:.6}, point={point:.6}, maximum={maximum:.6}, physical envelope={envelope:.6} px/s"
    );
    assert!(point <= envelope && maximum <= envelope);
    for pair in r.nodes[..r.count].windows(2) {
        assert!(((pair[1].pos - pair[0].pos).length() - r.spacing).abs() < 1e-8);
    }
    for triple in r.nodes[..r.count].windows(3) {
        let a = triple[1].pos - triple[0].pos;
        let b = triple[2].pos - triple[1].pos;
        let radius = r.spacing / (2. * (0.5 * turn(a, b)).sin().abs().max(1e-15));
        assert!(radius >= 299.99, "body radius {radius}");
    }
}

#[test]
fn released_head_keeps_radius_after_timer_ends() {
    for layer in [1, 2] {
        let minimum = [200., 220., 300.][layer];
        let mut r = still_306_fixture();
        r.layer = layer;
        r.count = 32;
        r.spacing = 20.;
        r.nodes[0].pos = p(1000., 720.);
        for i in 1..r.count {
            r.nodes[i].pos =
                r.nodes[i - 1].pos + polar(std::f64::consts::PI + 0.1 * i as f64) * r.spacing;
        }
        r.heading = p(1., 0.);
        r.previous_velocity = p(50., 0.);
        r.speed = 50.;
        r.course_sign = 1.;
        r.knot_for = 0.;
        r.escape_for = 0.;
        r.turn_sum = 0.;
        r.lane = 720.;
        r.sink = 0.;
        r.release_orbit(r.head() - p(100., 0.));
        r.release_age = 0.;
        r.velocities.fill(p(0., 0.));
        for (k, push) in r.pressure.iter_mut().enumerate() {
            *push = p(0., if k % 2 == 0 { 0. } else { 200. });
        }
        let bounds = Rect {
            x: 0.,
            y: 0.,
            width: 2560.,
            height: 1440.,
        };
        let layout = Layout::new(
            vec![OutputRect {
                id: 1,
                rect: bounds,
                primary: true,
            }],
            1,
        )
        .unwrap();
        let water = FlowField::new(1, &layout, FlowSettings::ribbons(0.4, 3.));
        let lanes = super::super::water::Lanes::new(bounds);
        let dt = 0.1;
        let before = r.head();
        let previous = before - r.previous_velocity * dt;
        r.swim(
            Steering {
                desired: p(0., 1000.),
                curl: 0.,
                gust: 0.,
                gust_force_limit: f64::INFINITY,
                maelstrom_motor_limit: f64::INFINITY,
                launch: 0.,
                gust_front: None,
                turbulence: 0.4,
                still: 0.,
                smooth_twist: false,
                spanning: false,
                braid_phase: None,
                orbit: None,
            },
            &water,
            &lanes,
            0.55,
            dt,
            false,
        );
        let a = before - previous;
        let b = r.head() - before;
        let travelled = (r.head() - previous).length() / (2. * turn(a, b).sin().abs());
        let bend = r.nodes[..r.count]
            .windows(3)
            .map(|n| turn(n[1].pos - n[0].pos, n[2].pos - n[1].pos).abs())
            .fold(0_f64, f64::max);
        println!(
            "Released Independent layer{layer} after timer expiry: pathRadius{travelled:.9},bodyBend{bend:.9}"
        );
        assert!(travelled >= minimum);
        assert!(bend <= r.spacing / (0.62 * minimum) + 1e-8);
    }
}

fn straight_release_fixture() -> Ribbon {
    let mut r = still_306_fixture();
    r.layer = 1;
    r.count = 96;
    r.spacing = 20.;
    r.nodes[0].pos = p(1000., 720.);
    for i in 1..r.count {
        r.nodes[i].pos = r.nodes[0].pos - p(i as f64 * r.spacing, 0.);
    }
    r.heading = p(1., 0.);
    r.previous_velocity = p(116., 0.);
    r.speed = 116.;
    r.course_sign = 1.;
    r.knot_for = 0.;
    r.turn_sum = 0.;
    r.lane = 720.;
    r.sink = 0.;
    r.release_orbit(r.head() - p(100., 0.));
    r.release_age = 10.;
    r.velocities.fill(p(0., 0.));
    r.pressure.fill(p(0., 0.));
    r
}
fn turn_release(r: &mut Ribbon, limit: f64) {
    let bounds = Rect {
        x: 0.,
        y: 0.,
        width: 2560.,
        height: 1440.,
    };
    let layout = Layout::new(
        vec![OutputRect {
            id: 1,
            rect: bounds,
            primary: true,
        }],
        1,
    )
    .unwrap();
    let water = FlowField::new(1, &layout, FlowSettings::ribbons(0.4, 3.));
    let lanes = super::super::water::Lanes::new(bounds);
    r.swim(
        Steering {
            desired: p(0., 1000.),
            curl: 0.,
            gust: 0.,
            gust_force_limit: f64::INFINITY,
            maelstrom_motor_limit: limit,
            launch: 1.,
            gust_front: None,
            turbulence: 0.4,
            still: 0.,
            smooth_twist: false,
            spanning: false,
            braid_phase: None,
            orbit: None,
        },
        &water,
        &lanes,
        0.55,
        0.1,
        false,
    );
}
#[test]
fn capped_maelstrom_head_turn_fits_the_tail_motion_budget() {
    let mut r = straight_release_fixture();
    let before = r.nodes;
    let velocity = r.previous_velocity;
    let lever = before[..r.count]
        .iter()
        .map(|n| (n.pos - before[0].pos).length())
        .fold(0_f64, f64::max);
    turn_release(&mut r, 350. / 3.);
    let head_step = r.head() - before[0].pos;
    let angle = turn(velocity, head_step).abs();
    let turn_step = 2. * lever * (0.5 * angle).sin();
    println!(
        "Capped Maelstrom turn: lever={lever:.6},headStep={:.9},rigidTurnStep={turn_step:.9},turnReserve={:.9}",
        head_step.length(),
        2. * head_step.length()
    );
    assert!(
        turn_step <= 2. * head_step.length() + 1e-8,
        "head asks the extended tail to exceed its turn drive"
    );
}
#[test]
fn uncapped_maelstrom_retains_the_default_pose() {
    let mut budgeted = straight_release_fixture();
    let mut reference = straight_release_fixture();
    turn_release(&mut budgeted, 350.);
    turn_release(&mut reference, f64::INFINITY);
    assert_eq!(budgeted.speed.to_bits(), reference.speed.to_bits());
    for (a, b) in budgeted.nodes[..budgeted.count].iter().zip(reference.nodes) {
        assert_eq!(a.pos.x.to_bits(), b.pos.x.to_bits());
        assert_eq!(a.pos.y.to_bits(), b.pos.y.to_bits());
    }
}
