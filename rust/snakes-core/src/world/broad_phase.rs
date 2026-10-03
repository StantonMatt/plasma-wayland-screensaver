// SPDX-License-Identifier: GPL-3.0-or-later
//! Mechanics endpoint-search bounds. Contact rules remain in each narrow phase.
use super::*;

impl World {
    /// Test-only entry point for allocation measurements of the broad phase.
    #[cfg(test)]
    pub(crate) fn diagnostic_mark_collisions(&mut self) { self.mark_collisions(); }

    /// Any contact along a sweep leaves the endpoints at most this far apart.
    /// `maximum_sweep` is the sum of both participants' maximum displacements
    /// this tick (zero for stationary participants). Measure movement instead
    /// of duplicating speed/effect multipliers or assuming body points follow
    /// the head exactly. A full-width body radius also bounds every taper tier.
    #[inline]
    pub(super) fn sweep_search_radius(contact_radius: f64, maximum_sweep: f64) -> f64 {
        contact_radius + maximum_sweep
    }

    /// Per-head displacement for acquisition broad phases. Their narrow phases
    /// retain endpoint-only pickup/vacuum rules; widening a reject cannot change
    /// ownership or the Magnet 9r boundary. Classic never measures these sweeps.
    pub(super) fn head_sweeps(&self) -> [f64; MAX_SNAKES] {
        let mut sweeps = [0.0; MAX_SNAKES];
        if self.config.rules == RuleSet::V2 {
            let g = self.config.geometry();
            for (id, s) in self.snakes.iter().enumerate() {
                if !s.alive { continue; }
                let head = self.segments[id * MAX_SEGMENTS];
                sweeps[id] = g.distance2(head.previous, head.current).sqrt();
            }
        }
        sweeps
    }

    /// Unique cell offsets, including grids narrower than the search diameter.
    /// Keep the original -1..=1 order whenever possible, including Classic.
    #[inline]
    pub(super) fn search_cell_offsets(search: f64, extent: f64, cells: usize, v2: bool, wrapped: bool) -> (isize, isize) {
        let radius = if v2 { (search / (extent / cells as f64)).ceil() as isize } else { 1 };
        if !wrapped {
            let radius = radius.min(cells as isize - 1);
            return (-radius, radius);
        }
        let lo = -radius.min(cells as isize / 2);
        let hi = radius.min(cells as isize - 1 + lo);
        (lo, hi)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arena(width: f64, height: f64, walls: bool) -> World {
        let mut w = World::diagnostic_arena(Config { width, height, scale: 25.0,
            speed: 250.0, rules: RuleSet::V2, self_collisions: true, deadly_walls: walls,
            density: 0.0, ..Config::default() },
            &[(Point { x: width * 0.5, y: height * 0.8 }, 0.0, 24, 0.0),
              (Point { x: width * 0.7, y: height * 0.8 }, 0.0, 24, 0.0)], &[]).unwrap();
        // Keep all unrelated segments well away from the tested sweeps.
        for id in 0..2 {
            for j in 0..24 {
                let p = Point { x: width * (0.5 + id as f64 * 0.2), y: height * 0.8 };
                w.segments[id * MAX_SEGMENTS + j] = Segment { current: p, previous: p };
            }
        }
        w
    }

    #[test]
    fn boosted_surge_advisory_tail_contact_crosses_two_bucket_columns() {
        let mut w = arena(1280.0, 720.0, true);
        for s in &mut w.snakes[..2] {
            s.base_radius = 5.13; s.radius = 5.13;
            s.traits.speed_bias = 1.13; s.rush = 0.6;
            s.effect_kind = effects::EffectKind::Surge as u8; s.effect_ticks = 100;
        }
        assert!((w.speed(&w.snakes[0]) * STEP_SECONDS - 16.347333333333335).abs() < 1e-12);
        let head = Segment { previous: Point { x: 357.8, y: 305.4 },
            current: Point { x: 373.364, y: 300.4 } };
        let tail = Segment { previous: Point { x: 361.8, y: 300.0 },
            current: Point { x: 345.453, y: 300.0 } };
        assert_eq!((w.cell(head.current).0, w.cell(tail.current).0), (14, 12));
        let reach = taper::contact_radius(RuleSet::V2, 5.13, taper::body_radius(5.13, 23.0, 24), false);
        assert!((reach - 4.881708).abs() < 1e-12);
        assert!(World::swept_hit(w.config.geometry(), head, tail, reach));
        w.segments[0] = head; w.segments[MAX_SEGMENTS + 23] = tail;
        w.diagnostic_mark_collisions();
        assert_eq!(w.snakes[0].dying, DeathReason::Body);
        assert_eq!(w.collisions[0].owner_mask, 1 << 1);
        // Explicit compatibility guard: Classic retains its historical search.
        w.config.rules = RuleSet::Classic;
        w.mark_collisions();
        assert_eq!(w.snakes[0].dying, DeathReason::None);
    }

    #[test]
    fn cell_offsets_are_unique_and_cover_all_nearby_endpoint_cells() {
        for cells in 1..=40 {
            let extent = cells as f64 * 7.3;
            for search in [0.0, 1.0, 7.3, 7.31, 20.0, extent, extent * 2.0] {
                for wrapped in [false, true] {
                    let (lo, hi) = World::search_cell_offsets(search, extent, cells, true, wrapped);
                    for head in 0..cells {
                        let mut seen = vec![false; cells];
                        for offset in lo..=hi {
                            let x = head as isize + offset;
                            let x = if wrapped { x.rem_euclid(cells as isize) }
                                else if x < 0 || x >= cells as isize { continue; } else { x };
                            assert!(!seen[x as usize]); seen[x as usize] = true;
                        }
                        for other in 0..cells {
                            // Minimum separation between the closed cell intervals.
                            let delta = head.abs_diff(other);
                            let delta = if wrapped { delta.min(cells - delta) } else { delta };
                            let minimum = delta.saturating_sub(1) as f64 * extent / cells as f64;
                            if minimum < search { assert!(seen[other]); }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn sweep_contacts_are_never_lost_by_body_self_or_head_searches() {
        let mut rng = WorldRng::new(0x5e33);
        let mut contacts = 0;
        for walls in [false, true] {
            for (width, height) in [(1280.0, 720.0), (80.0, 80.0)] {
                let mut w = arena(width, height, walls);
                if width == 80.0 {
                    w.grid_columns = 1; w.grid_rows = 2; w.grid_heads.resize(2, -1);
                }
                let g = w.config.geometry();
                for sample in 0..600 {
                    let same = sample % 8 < 4;
                    let kind = if sample % 4 >= 2 { effects::EffectKind::Surge }
                        else { effects::EffectKind::None };
                    for s in &mut w.snakes[..2] {
                        s.radius = 4.5; s.base_radius = 4.5;
                        s.effect_kind = kind as u8; s.effect_ticks = 100;
                        s.rush = if sample % 2 == 1 { 0.6 } else { 0.0 };
                        s.growth = if sample % 3 == 0 { 2.0 } else { 0.0 };
                    }
                    let index = [10, 14, 20, 23][sample % 4];
                    let owner = usize::from(!same);
                    let reach = taper::contact_radius(RuleSet::V2, 4.5,
                        taper::body_radius(4.5, index as f64, 24), same);
                    let speed = (w.speed(&w.snakes[0]) * STEP_SECONDS).min(width * 0.1).min(height * 0.1);
                    let contact = if walls {
                        Point { x: speed + rng.random() * (width - 2.0 * speed),
                            y: speed + rng.random() * (height * 0.45 - 2.0 * speed).max(0.0) }
                    } else { Point { x: rng.random() * width, y: rng.random() * height * 0.45 } };
                    let angle = rng.random() * TAU;
                    let dx = angle.cos() * speed * rng.random();
                    let dy = angle.sin() * speed * rng.random();
                    let offset = reach * 0.8 * rng.random();
                    let head = Segment {
                        previous: g.wrap(Point { x: contact.x - dx, y: contact.y - dy }),
                        current: g.wrap(Point { x: contact.x + dx, y: contact.y + dy }),
                    };
                    let body = Segment {
                        previous: g.wrap(Point { x: contact.x + dx, y: contact.y + dy + offset }),
                        current: g.wrap(Point { x: contact.x - dx, y: contact.y - dy + offset }),
                    };
                    // Narrow-phase oracle, before the bucket search.
                    if g.distance2(head.current, body.current) >= reach * reach
                        && g.segments_distance2(head.previous, head.current, body.previous, body.current) >= reach * reach { continue; }
                    assert!(World::swept_hit(g, head, body, reach), "sweep prefilter lost a contact");
                    contacts += 1;
                    w.segments[0] = head;
                    w.segments[owner * MAX_SEGMENTS + index] = body;
                    w.mark_collisions();
                    assert_eq!(w.snakes[0].dying,
                        if same { DeathReason::SelfHit } else { DeathReason::Body },
                        "sample={sample} walls={walls} width={width} same={same} head={head:?} body={body:?}");
                    let head_reach = (w.snakes[0].radius + w.snakes[1].radius) * 0.82;
                    assert!(World::swept_hit(g, head, body, head_reach));
                    if owner == 0 {
                        let p = Point { x: width * 0.5, y: height * 0.8 };
                        w.segments[index] = Segment { current: p, previous: p };
                    } else {
                        let p = Point { x: width * 0.7, y: height * 0.8 };
                        w.segments[MAX_SEGMENTS + index] = Segment { current: p, previous: p };
                    }
                    w.segments[MAX_SEGMENTS] = body;
                    w.mark_collisions();
                    assert_eq!(w.snakes[0].dying, DeathReason::Head);
                    let p = Point { x: width * 0.7, y: height * 0.8 };
                    w.segments[MAX_SEGMENTS] = Segment { current: p, previous: p };
                }
            }
        }
        assert!(contacts > 2000, "only {contacts} sweep contacts exercised");
    }

    #[test]
    fn acquisition_searches_preserve_endpoint_rules_and_magnet_boundary() {
        let mut rng = WorldRng::new(0xac901);
        let mut checked = 0;
        for walls in [false, true] {
            let mut w = arena(1280.0, 720.0, walls);
            w.snakes[1].alive = false;
            let g = w.config.geometry();
            for sample in 0..1200 {
                let magnet = sample % 2 == 0;
                w.snakes[0].effect_kind = if magnet { effects::EffectKind::Magnet as u8 }
                    else { effects::EffectKind::Surge as u8 };
                w.snakes[0].effect_ticks = 100;
                let head = Point { x: rng.random() * 1280.0, y: rng.random() * 720.0 };
                let angle = rng.random() * TAU;
                let travel = w.speed(&w.snakes[0]) * STEP_SECONDS * 1.6;
                w.segments[0] = Segment { current: head,
                    previous: g.wrap(Point { x: head.x - travel * angle.cos(), y: head.y - travel * angle.sin() }) };
                let reach = 1.3 * w.snakes[0].radius + 9.45;
                let offset = reach + [0.01, -0.01, 0.0][sample % 3];
                let p = g.wrap(Point { x: head.x + offset * angle.cos(), y: head.y + offset * angle.sin() });
                let expected = g.distance2(head, p) <= reach * reach;
                w.items.clear();
                w.items.push(Item { id: 1, position: p, radius: 9.45, kind: effects::EffectKind::Phase,
                    life_ticks: 750, ..Item::default() });
                w.pickup_items();
                assert_eq!(w.items.is_empty(), expected, "pickup sample={sample} walls={walls}");
                // Restore acquisition reach after a possible replacing pickup.
                w.snakes[0].effect_kind = if magnet { effects::EffectKind::Magnet as u8 }
                    else { effects::EffectKind::Surge as u8 };
                let reach = w.snakes[0].radius * if magnet { 9.0 } else { 3.0 } + 1.0;
                let offset = reach + [0.01, -0.01, 0.0][sample % 3];
                let p = g.wrap(Point { x: head.x + offset * angle.cos(), y: head.y + offset * angle.sin() });
                let expected = g.distance2(head, p) <= reach * reach;
                w.food.clear();
                w.food.push(Food { id: 1, p, owner: -1, size: 1.0, life: 50.0,
                    value: 1.0, ..Food::default() });
                w.feed_snakes(0.0);
                assert_eq!(w.food[0].owner, if expected { 0 } else { -1 },
                    "food sample={sample} walls={walls} magnet={magnet}");
                checked += 1;
            }
        }
        assert_eq!(checked, 2400);
    }


    #[test]
    fn canonical_bucket_fast_path_matches_legacy_runtime_endpoint_indexing() {
        let mut rng = WorldRng::new(0xce11);
        for (width, height) in [(1280.0, 720.0), (3440.0, 1440.0), (80.0, 80.0)] {
            let w = arena(width, height, false);
            let g = w.config.geometry();
            for _ in 0..4000 {
                // Head movement and body placement both store exactly this
                // wrapped representation, including seam crossings.
                let p = g.wrap(Point { x: (rng.random() * 5.0 - 2.0) * width,
                    y: (rng.random() * 5.0 - 2.0) * height });
                assert_eq!(w.cell_for_rules::<true>(p), w.cell_for_rules::<false>(p));
            }
            for column in 0..=w.grid_columns {
                for offset in [-1e-10, 0.0, 1e-10] {
                    let p = g.wrap(Point { x: column as f64 * width / w.grid_columns as f64 + offset,
                        y: height * 0.5 });
                    assert_eq!(w.cell_for_rules::<true>(p), w.cell_for_rules::<false>(p));
                }
            }
            for p in [Point { x: -width, y: -height }, Point { x: width, y: height },
                Point { x: width * 2.1, y: height * -1.4 }] {
                assert_eq!(w.cell_for_rules::<true>(p), w.cell_for_rules::<false>(p));
            }
        }
    }

}
