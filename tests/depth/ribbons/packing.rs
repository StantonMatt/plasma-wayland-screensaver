use super::*;
use snakes_core::visuals::depth::ribbons::{CapPair, MotionPair, visuals_ribbons_write_packed};

fn decode_vertex(
    body: snakes_core::visuals::depth::ribbons::BodyKnot,
    cap: [f32; 4],
) -> snakes_core::visuals::depth::ribbons::Vertex {
    snakes_core::visuals::depth::ribbons::Vertex {
        pos: body.pos,
        normal: body.normal,
        half_width: body.half_width,
        facing: body.facing,
        pulse: body.pulse,
        alpha: body.alpha,
        halo_radius: cap[0],
        cap_mix: cap[1],
        tip_distance: cap[2],
        tip_radius: cap[3],
    }
}
pub(super) struct Packets {
    motion: Vec<MotionPair>,
    caps: Vec<CapPair>,
    pub(super) draws: Vec<Draw>,
    pub(super) cap_count: usize,
}
impl Packets {
    pub(super) fn new() -> Self {
        Self {
            motion: vec![MotionPair::default(); 10754],
            caps: vec![CapPair::default(); 10754],
            draws: vec![Draw::default(); 82],
            cap_count: 0,
        }
    }
    pub(super) fn export(
        &mut self,
        lo: &Arc<Snapshot>,
        hi: &Arc<Snapshot>,
        rect: Rect,
        ns: i64,
        alpha: f64,
    ) -> (usize, usize) {
        let lease = export_lease(lo, hi, rect, ns, alpha);
        let mut used = 0;
        let draws = unsafe {
            visuals_ribbons_write_packed(
                &lease,
                self.motion.as_mut_ptr(),
                self.motion.len(),
                self.caps.as_mut_ptr(),
                self.caps.len(),
                &mut self.cap_count,
                self.draws.as_mut_ptr(),
                self.draws.len(),
                &mut used,
            )
        };
        (used, draws)
    }
    pub(super) fn decode(&self, count: usize, dst: &mut Vec<VertexPair>) {
        dst.clear();
        dst.extend(self.motion[..count].iter().map(|pair| {
            let slot = (pair.tag >> 16) as usize;
            let cap = if slot == 0 {
                CapPair::default()
            } else {
                self.caps[slot - 1]
            };
            VertexPair {
                lower: decode_vertex(pair.lower, cap.lower),
                upper: decode_vertex(pair.upper, cap.upper),
                tag: (pair.tag & 65535) as f32,
            }
        }));
    }
    pub(super) fn bytes(&self, nodes: usize) -> usize {
        nodes * size_of::<MotionPair>() + self.cap_count.div_ceil(32) * 32 * size_of::<CapPair>()
    }
}
#[test]
fn sparse_motion_packet_preserves_every_sampled_shader_input() {
    let mut logical = Buffers::new();
    let mut packet = Packets::new();
    let mut compared = 0;
    for still in [false, true] {
        let input = inputs(still, true);
        let mut world = Ribbons::new(input.clone());
        let mut previous = Arc::new(world.snapshot());
        for tick in 1..=if still { 3 } else { 240 } {
            world.step(tick);
            let current = Arc::new(world.snapshot());
            if tick % 19 == 1 || still {
                for out in &input.layout.outputs {
                    let ns = if still { 604_000_000_000 } else { 0 };
                    let (n, d) = logical.export(&previous, &current, out.rect, ns, 0.5);
                    let (used, draws) = packet.export(&previous, &current, out.rect, ns, 0.5);
                    assert_eq!((draws, used), (d, n));
                    for range in &packet.draws[..d] {
                        let mut previous_uses_cap = false;
                        for i in range.first as usize..(range.first + range.count) as usize {
                            let a = logical.nodes[i];
                            let b = packet.motion[i];
                            assert_eq!(b.tag & 65535, a.tag as u32);
                            let active = a.lower.cap_mix != 0. || a.upper.cap_mix != 0.;
                            let index = (b.tag >> 16) as usize;
                            assert!(index <= packet.cap_count);
                            for (k, (full, packed)) in [(a.lower, b.lower), (a.upper, b.upper)]
                                .into_iter()
                                .enumerate()
                            {
                                assert_eq!(packed.pos, full.pos);
                                assert_eq!(packed.normal, full.normal);
                                assert_eq!(
                                    [packed.half_width, packed.facing, packed.pulse, packed.alpha],
                                    [full.half_width, full.facing, full.pulse, full.alpha]
                                );
                                if index > 0 {
                                    let cap = if k == 0 {
                                        packet.caps[index - 1].lower
                                    } else {
                                        packet.caps[index - 1].upper
                                    };
                                    assert_eq!(
                                        cap,
                                        [
                                            full.halo_radius,
                                            full.cap_mix,
                                            full.tip_distance,
                                            full.tip_radius
                                        ]
                                    );
                                } else {
                                    assert!(!active && !previous_uses_cap);
                                }
                            }
                            previous_uses_cap = active;
                            compared += 1;
                        }
                    }
                }
            }
            previous = current;
        }
    }
    assert!(compared > 1000);
}
