use super::*;
use snakes_core::visuals::depth::common::{flow::p, layout::Vec2, rng::output_seed};
use snakes_core::visuals::depth::ribbons::Vertex;
use snakes_core::visuals::depth::schema::Value;

#[derive(Clone, Copy)]
enum Section {
    Mixed,
    Derivative,
}
#[derive(Clone, Copy, Default)]
struct Span {
    edges: [[Vec2; 2]; 4],
    centre: [Vec2; 4],
    bounds: [f64; 4],
}
type Tri = [Vec2; 3];
fn cross(a: Vec2, b: Vec2) -> f64 {
    a.x * b.y - a.y * b.x
}
// Reconstruct the shader in its native f32 precision. Intersection predicates
// operate in f64 on those finished vertex positions.
#[derive(Clone, Copy)]
struct V(f32, f32);
impl std::ops::Add for V {
    type Output = Self;
    fn add(self, b: Self) -> Self {
        Self(self.0 + b.0, self.1 + b.1)
    }
}
impl std::ops::Sub for V {
    type Output = Self;
    fn sub(self, b: Self) -> Self {
        Self(self.0 - b.0, self.1 - b.1)
    }
}
impl std::ops::Mul<f32> for V {
    type Output = Self;
    fn mul(self, b: f32) -> Self {
        Self(self.0 * b, self.1 * b)
    }
}
impl V {
    fn length(self) -> f32 {
        (self.0 * self.0 + self.1 * self.1).sqrt()
    }
    fn unit(self) -> Self {
        self * (1. / self.length().max(0.00001_f32.sqrt()))
    }
    fn perp(self) -> Self {
        Self(-self.1, self.0)
    }
    fn cross(self, b: Self) -> f32 {
        self.0 * b.1 - self.1 * b.0
    }
    fn raster(self) -> Vec2 {
        p(f64::from(self.0), f64::from(self.1))
    }
}
fn xy(a: [f32; 2]) -> V {
    V(a[0], a[1])
}
fn tris(edges: &[[Vec2; 2]], n: usize) -> [Tri; 2] {
    let [a, b] = edges[n];
    let [c, d] = edges[n + 1];
    [[a, b, c], [c, b, d]]
}
fn make_span(a: VertexPair, b: VertexPair, draw: &Draw, alpha: f32, mode: Section) -> Option<Span> {
    let na = a.at(alpha);
    let nb = b.at(alpha);
    let pa = xy(na.pos);
    let pb = xy(nb.pos);
    let delta = pb - pa;
    let length = delta.length();
    if length < 0.0001 {
        return None;
    }
    let section = |v: VertexPair| {
        let lo = xy(v.lower.normal);
        let hi = xy(v.upper.normal);
        let blend = lo.unit() + (hi.unit() - lo.unit()) * alpha;
        let squared = (blend.0 * blend.0 + blend.1 * blend.1).clamp(0.00001, 1.);
        (
            blend * (1. / squared.sqrt()),
            1. + (lo.length().max(hi.length()) - 1.).max(0.) / squared,
        )
    };
    let (normal_a, ca) = section(a);
    let (normal_b, cb) = section(b);
    let ta = normal_a.perp() * -1.;
    let tb = normal_b.perp() * -1.;
    let m0 = ta * length;
    let m1 = tb * length;
    let c2 = delta * 3. - m0 * 2. - m1;
    let c3 = m0 + m1 - delta * 2.;
    let layer = draw.material / 6;
    let glow = draw.silk.halo[3];
    let mut result = Span {
        bounds: [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ],
        ..Default::default()
    };
    for j in 0..4 {
        let t = j as f32 / 3.;
        let (centre, normal, width, face, k) = if j == 0 {
            (pa, normal_a, na.half_width, na.facing, (ca - 1.) / 50.)
        } else if j == 3 {
            (pb, normal_b, nb.half_width, nb.facing, (cb - 1.) / 50.)
        } else {
            let centre = pa + (m0 + (c2 + c3 * t) * t) * t;
            let derivative = m0 + (c2 * 2. + c3 * (3. * t)) * t;
            let second = c2 * 2. + c3 * (6. * t);
            let k = (ca + (cb - ca) * t - 1.) / 50.;
            let (normal, k) = match mode {
                Section::Mixed => (normal_a + (normal_b - normal_a) * t, k),
                Section::Derivative => (
                    (derivative * (1. / derivative.length().max(0.001))).perp(),
                    k.max(derivative.cross(second).abs() / derivative.length().powi(3).max(1e-12)),
                ),
            };
            (
                centre,
                normal,
                na.half_width + (nb.half_width - na.half_width) * t,
                na.facing + (nb.facing - na.facing) * t,
                k,
            )
        };
        let radius = width * (0.12 + 0.88 * face.abs());
        let soft = match layer {
            0 => 1.45,
            2 => 0.19 * radius,
            _ => 0.40,
        };
        let aa = (soft * 1.4).max(1.2);
        let limit = 0.85 / k.max(0.00001);
        let halo = width.min((limit - radius - aa).max(0.) / (7. * glow).max(0.00001));
        let blend = if na.cap_mix < 0. {
            1.
        } else if j == 3 {
            nb.cap_mix
        } else {
            na.cap_mix + (nb.cap_mix - na.cap_mix) * t
        };
        let head_radius = if j == 3 {
            nb.halo_radius
        } else {
            na.halo_radius + (nb.halo_radius - na.halo_radius) * t
        };
        let body_halo = halo * 7. * glow;
        let envelope = body_halo + (1.2 * head_radius - body_halo) * blend;
        let outer = (radius + aa + envelope).min(limit);
        result.centre[j] = centre.raster();
        let pair = [
            (centre - normal * outer).raster(),
            (centre + normal * outer).raster(),
        ];
        for v in pair {
            result.bounds[0] = result.bounds[0].min(v.x);
            result.bounds[1] = result.bounds[1].min(v.y);
            result.bounds[2] = result.bounds[2].max(v.x);
            result.bounds[3] = result.bounds[3].max(v.y);
        }
        result.edges[j] = pair;
    }
    Some(result)
}
fn overlap(a: Tri, b: Tri) -> bool {
    if cross(a[1] - a[0], a[2] - a[0]).abs() < 1e-12
        || cross(b[1] - b[0], b[2] - b[0]).abs() < 1e-12
    {
        return false;
    }
    for triangle in [a, b] {
        for j in 0..3 {
            let axis = (triangle[(j + 1) % 3] - triangle[j]).perpendicular();
            let interval = |tri: Tri| {
                let p0 = axis.dot(tri[0]);
                let p1 = axis.dot(tri[1]);
                let p2 = axis.dot(tri[2]);
                (p0.min(p1).min(p2), p0.max(p1).max(p2))
            };
            let x = interval(a);
            let y = interval(b);
            if x.1 <= y.0 + 1e-8 || y.1 <= x.0 + 1e-8 {
                return false;
            }
        }
    }
    true
}
fn crosses_centreline(a: &Span, b: &Span) -> bool {
    for left in a.centre.windows(2) {
        for right in b.centre.windows(2) {
            let da = left[1] - left[0];
            let db = right[1] - right[0];
            if cross(da, right[0] - left[0]) * cross(da, right[1] - left[0]) < 0.
                && cross(db, left[0] - right[0]) * cross(db, left[1] - right[0]) < 0.
            {
                return true;
            }
        }
    }
    false
}
fn spans_overlap(a: &Span, b: &Span) -> bool {
    if a.bounds[0] >= b.bounds[2]
        || b.bounds[0] >= a.bounds[2]
        || a.bounds[1] >= b.bounds[3]
        || b.bounds[1] >= a.bounds[3]
    {
        return false;
    }
    for j in 0..3 {
        for k in 0..3 {
            for x in tris(&a.edges, j) {
                for y in tris(&b.edges, k) {
                    if overlap(x, y) {
                        return true;
                    }
                }
            }
        }
    }
    false
}
// A shared edge separates the two regions when their vertices occupy
// opposite open half-planes. Test triangles if another edge crosses that line.
fn separated(a: &[[Vec2; 2]], b: &[[Vec2; 2]]) -> bool {
    let [origin, other] = a[a.len() - 1];
    let axis = (other - origin).perpendicular();
    a[..a.len() - 1]
        .iter()
        .flatten()
        .all(|p| axis.dot(*p - origin) >= -1e-8)
        && b[1..]
            .iter()
            .flatten()
            .all(|p| axis.dot(*p - origin) <= 1e-8)
}
#[derive(Default)]
struct Counts {
    triangles: u64,
    flipped: u64,
    adjacent: u64,
    distant: u64,
    centre_crossings: u64,
    joins: u64,
    brackets: u64,
    first: Option<String>,
    reported_for_scene: usize,
}
impl Counts {
    fn fail(&mut self, context: &str, i: usize, message: &str) {
        if self.reported_for_scene < 12 {
            self.reported_for_scene += 1;
            eprintln!("{context}, span {i}: {message}");
        }
        if self.first.is_none() {
            self.first = Some(format!("{context}, span {i}: {message}"));
        }
    }
    fn check(
        &mut self,
        records: &[VertexPair],
        d: &Draw,
        alpha: f32,
        mode: Section,
        context: &str,
        count_distant: bool,
    ) {
        let mut spans = Vec::with_capacity(records.len());
        for (i, n) in records.windows(2).enumerate() {
            let Some(span) = make_span(n[0], n[1], d, alpha, mode) else {
                continue;
            };
            for j in 0..3 {
                for tr in tris(&span.edges, j) {
                    self.triangles += 1;
                    if cross(tr[1] - tr[0], tr[2] - tr[0]) > 1e-10 {
                        self.flipped += 1;
                        if self.first.is_none() {
                            eprintln!(
                                "First reversed section: lower/upper endpoints {:?} {:?}, material {}",
                                n[0], n[1], d.material
                            );
                        }
                        self.fail(context, i, "triangle orientation reversed");
                    }
                }
            }
            for joint in 1..3 {
                if !separated(&span.edges[..=joint], &span.edges[joint..]) {
                    for left in 0..joint {
                        for right in joint..3 {
                            for a in tris(&span.edges, left) {
                                for b in tris(&span.edges, right) {
                                    if overlap(a, b) {
                                        self.adjacent += 1;
                                        self.fail(context, i, "subspan interiors overlap");
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if let Some(last) = spans.last() {
                let last: &Span = last;
                self.joins += 1;
                if last.edges[3] != span.edges[0] {
                    self.adjacent += 1;
                    self.fail(context, i, "shared edges differ");
                }
                if !separated(&last.edges, &span.edges) && spans_overlap(last, &span) {
                    self.adjacent += 1;
                    self.fail(context, i, "neighbouring segments overlap");
                }
            }
            if count_distant {
                for earlier in spans.iter().take(spans.len().saturating_sub(1)) {
                    if spans_overlap(earlier, &span) {
                        self.distant += 1;
                        self.centre_crossings += u64::from(crosses_centreline(earlier, &span));
                    }
                }
            }
            spans.push(span);
        }
    }
}
fn check_scenario(
    name: &str,
    input: WorldInput,
    ticks: &[u64],
    fraction_intervals: usize,
    event: Option<Event>,
    mode: Section,
    stats: &mut Counts,
) {
    stats.reported_for_scene = 0;
    let previous_triangles = stats.triangles;
    let previous_brackets = stats.brackets;
    let mut world = Ribbons::new(input.clone());
    if let Some(e) = event {
        world.inject(e).unwrap();
    }
    let mut buf = packing::Packets::new();
    let mut decoded = Vec::new();
    let mut next = 0;
    for tick in 0..=*ticks.last().unwrap() {
        if tick != ticks[next] {
            world.step(tick + 1);
            continue;
        }
        let lo = Arc::new(world.snapshot());
        world.step(tick + 1);
        let hi = Arc::new(world.snapshot());
        for output in &input.layout.outputs {
            let (knots, count) = buf.export(
                &lo,
                &hi,
                output.rect,
                (tick * 1_000_000_000 / 30) as i64,
                0.,
            );
            buf.decode(knots, &mut decoded);
            for fraction in 0..=fraction_intervals {
                let alpha = fraction as f32 / fraction_intervals as f32;
                stats.brackets += 1;
                for (index, d) in buf.draws[..count]
                    .iter()
                    .enumerate()
                    .filter(|(_, d)| d.motes == 0)
                {
                    let label = format!(
                        "{name} tick {tick} output {} alpha {alpha} ribbon {index}",
                        output.id
                    );
                    let i = d.first as usize;
                    stats.check(
                        &decoded[i..i + d.count as usize],
                        d,
                        alpha,
                        mode,
                        &label,
                        true,
                    );
                }
            }
        }
        if ticks.len() > 1000 && tick % 1800 == 0 {
            println!(
                "{name} at tick {tick}: {} triangles, {} joins",
                stats.triangles, stats.joins
            );
        }
        next += 1;
        if next == ticks.len() {
            break;
        }
    }
    assert!(
        stats.triangles > previous_triangles,
        "{name} emitted no strip triangles"
    );
    assert_eq!(
        stats.brackets - previous_brackets,
        ((fraction_intervals + 1) * ticks.len() * input.layout.outputs.len()) as u64
    );
}
fn profile(name: &str, seed: u64, hd: bool, maximum: bool) -> WorldInput {
    let mut input = inputs(false, false);
    if name != "Maximum spanning" && name != "Balance-spanning" {
        input.mode = MonitorMode::Independent;
        input.seed = output_seed(seed, 4, "Virtual-0");
        let mut output = input.layout.outputs[0].clone();
        if hd {
            output.rect.width = 1920.;
            output.rect.height = 1080.;
        }
        input.layout = Layout::new(vec![output], 1).unwrap();
    } else {
        input.seed = seed;
    }
    if maximum || name == "G" {
        input.speed = 3.;
        input.density = if maximum { 100. } else { 80. };
        input.scale = 2.;
        input.trails = 1.;
        if maximum {
            input.parameters = (1..=3)
                .map(|id| Value {
                    id,
                    kind: 1,
                    payload: 100,
                })
                .collect();
        }
        if name == "G" {
            input.palette = 2;
        }
    }
    input
}

// The native fixture and this triangle certificate consume the same original
// knot pairs. Later lifecycle changes can move the live A/60 strand.
fn historical_crossing() -> Vec<VertexPair> {
    let vertex = |f: &[f32]| Vertex {
        pos: [f[0], f[1]],
        normal: [f[2], f[3]],
        half_width: f[4],
        facing: f[5],
        pulse: f[6],
        alpha: f[7],
        ..Vertex::default()
    };
    include_str!("a60-knots.inc")
        .lines()
        .filter(|line| !line.starts_with("//"))
        .map(|line| {
            let fields: Vec<f32> = line
                .strip_prefix("RIBBON_KNOT(")
                .unwrap()
                .strip_suffix(')')
                .unwrap()
                .split(',')
                .map(|f| f.trim().trim_end_matches('f').parse().unwrap())
                .collect();
            assert_eq!(fields.len(), 17);
            VertexPair {
                lower: vertex(&fields[..8]),
                upper: vertex(&fields[8..16]),
                tag: fields[16],
            }
        })
        .collect()
}
// Exercise the two real release witnesses in ordinary CI as well as in the
// exhaustive capture gate. A genuine knot must survive local fold prevention.
#[test]
fn release_bend_and_genuine_knot() {
    let mut knot = Counts::default();
    check_scenario(
        "A",
        profile("A", 7, false, false),
        &[1800],
        64,
        None,
        Section::Mixed,
        &mut knot,
    );
    assert_eq!((knot.flipped, knot.adjacent), (0, 0), "{:?}", knot.first);
    let records = historical_crossing();
    assert_eq!(records.len(), 33);
    let mut draw = Draw {
        material: records[0].tag as u32 % 32,
        ..Draw::default()
    };
    draw.silk.halo[3] = 0.35;
    let span = |original: usize| {
        let i = original - 16;
        make_span(records[i], records[i + 1], &draw, 0., Section::Mixed).unwrap()
    };
    assert!(crosses_centreline(&span(19), &span(45)));
    for step in 0..=64 {
        knot.check(
            &records,
            &draw,
            step as f32 / 64.,
            Section::Mixed,
            "historical A/60",
            true,
        );
    }
    assert_eq!((knot.flipped, knot.adjacent), (0, 0), "{:?}", knot.first);
    assert!(knot.centre_crossings > 0);
    let mut bend = Counts::default();
    check_scenario(
        "G",
        profile("G", 7, false, false),
        &[1199, 4346, 8773, 10324, 18000, 18043],
        64,
        None,
        Section::Mixed,
        &mut bend,
    );
    assert_eq!((bend.flipped, bend.adjacent), (0, 0), "{:?}", bend.first);
}

#[test]
#[ignore = "native-scale expanded-triangle certificate over the seeded capture profiles"]
fn certify_folds() {
    let derivative = std::env::var_os("PVS_RIBBON_DERIVATIVE_TRIAL").is_some();
    let mode = if derivative {
        Section::Derivative
    } else {
        Section::Mixed
    };
    let mut stats = Counts::default();
    for (name, seed, seconds, hd, maximum) in [
        ("E-max", 3, vec![60, 180], true, true),
        ("G", 7, vec![600], false, false),
        ("A", 7, vec![4, 60, 300], false, false),
        ("Maelstrom", 23, vec![121, 130, 142, 150], false, false),
        ("Braid", 7, vec![11, 48, 60, 75], true, false),
        ("Still water", 23, vec![415, 417], false, false),
        ("Gust", 23, vec![601, 604], false, false),
        ("Balance-spanning", 31, vec![300], false, false),
        ("Maximum spanning", 31, vec![], false, true),
    ] {
        let input = profile(name, seed, hd, maximum);
        let event = match name {
            "Maelstrom" => Some(Event {
                id: 2,
                at_ns: 120_000_000_000,
                anchor: [1130., 715.],
                strength: 1.,
            }),
            "Still water" => Some(Event {
                id: 3,
                at_ns: 400_000_000_000,
                anchor: [1280., 720.],
                strength: 1.,
            }),
            "Gust" => Some(Event {
                id: 1,
                at_ns: 600_000_000_000,
                anchor: [0., 720.],
                strength: 1.,
            }),
            "Braid" => Some(Event {
                id: 2,
                at_ns: 30_000_000_000,
                anchor: [950., 530.],
                strength: 1.,
            }),
            _ => None,
        };
        let mut ticks: Vec<_> = seconds.into_iter().map(|sec| sec * 30).collect();
        match name {
            "E-max" => ticks.extend(1800..=1860),
            "G" => {
                ticks.extend([1199, 4346, 8773, 10324, 13890]);
                ticks.extend(18000..=18090);
            }
            "Maelstrom" => {
                ticks.extend(3900..=3960);
                ticks.extend(4283..=4290);
                ticks.extend((4350..=4800).step_by(15));
            }
            "Braid" => {
                ticks.extend([322, 480, 1023, 1579, 1580]);
                ticks.extend((1731..=2121).step_by(15));
            }
            "Balance-spanning" => ticks.extend((150..=9000).step_by(150)),
            "Maximum spanning" => {
                if std::env::var_os("PVS_RIBBON_CERT_SAMPLED").is_some() {
                    ticks.extend(0..=120);
                    ticks.extend(9717..=9720);
                    ticks.extend((150..=18000).step_by(150));
                } else {
                    ticks.extend(0..=18000);
                }
            }
            _ => {}
        }
        ticks.sort_unstable();
        ticks.dedup();
        if std::env::var_os("PVS_RIBBON_CERT_SAMPLED").is_none() {
            ticks = (0..=*ticks.last().unwrap()).collect();
        }
        check_scenario(name, input, &ticks, 2, event, mode, &mut stats);
        println!(
            "{name}: cumulative {} triangles, {} flipped, {} local errors, {} distant overlaps",
            stats.triangles, stats.flipped, stats.adjacent, stats.distant
        );
    }
    println!(
        "{} viewport/fraction samples, {} shared joints; {} non-adjacent centreline crossings; first failure {:?}",
        stats.brackets, stats.joins, stats.centre_crossings, stats.first
    );
    assert_eq!((stats.flipped, stats.adjacent), (0, 0));
}
