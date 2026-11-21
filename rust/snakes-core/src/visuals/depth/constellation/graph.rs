use super::{Star, space::*};
#[derive(Clone, Copy, Debug)]
pub struct Edge {
    pub a: usize,
    pub b: usize,
    pub born: f32,
    pub end: f32,
    pub cut_at: [Point; 2],
    pub wave: f32,
    pub reverse: bool,
    pub figure: usize,
}
impl Edge {
    pub fn live(&self) -> bool {
        self.end == f32::INFINITY
    }
}
fn conflict(p: Point, q: Point, r: Point, s: Point, angle_cos2: f32) -> bool {
    let u = q - p;
    let v = s - r;
    let lens = u.len2() * v.len2();
    let dot = u.dot(v);
    for (a, b, c, d) in [(p, q, r, s), (p, q, s, r), (q, p, r, s), (q, p, s, r)] {
        if (a - c).len2() < 400. {
            // Use one representative for an unresolved double. It shares the
            // degree budget as well as the angle budget of the apparent node.
            return a != c
                || (b - d).len2() < 400.
                || ((b - a).dot(d - c) > 0. && dot * dot > angle_cos2 * lens);
        }
    }
    let cross = |a: Point, b: Point| a.x * b.y - a.y * b.x;
    let w = r - p;
    let t = s - p;
    let left = cross(u, w);
    let right = cross(u, t);
    let ru = w.dot(u);
    let su = t.dot(u);
    let overlap = ru.min(su) <= u.len2() && ru.max(su) >= 0.;
    if overlap && left * right <= 0. && cross(v, p - r) * cross(v, q - r) <= 0. {
        return true;
    }
    // Signed distance at both ends of the projected overlap. Lines can
    // converge inside the overlap even when their far endpoints are clear.
    if overlap && dot * dot > 0.9330127 * lens && (su - ru).abs() > 0.001 {
        let distance = |x: f32| left + (right - left) * ((x - ru) / (su - ru));
        let d0 = distance(ru.min(su).max(0.));
        let d1 = distance(ru.max(su).min(u.len2()));
        return d0 * d1 <= 0. || d0.abs().min(d1.abs()).powi(2) < 1600. * u.len2();
    }
    false
}
pub struct Graph {
    pub edges: Vec<Edge>,
    index: Grid,
    degree: Vec<u8>,
    last: Vec<f32>,
    parent: Vec<usize>,
    members: Vec<u16>,
    adjacency: Vec<[usize; 5]>,
    visited: Vec<bool>,
    queue: Vec<usize>,
    candidates: Vec<(f32, usize)>,
    cycles: Vec<bool>,
    hue_sites: Vec<(Point, usize)>,
    axial: Vec<u8>,
    pub scale: f32,
    pub spectrum: bool,
    floor: f32,
    height: f32,
    left: f32,
    width: f32,
    pub reach: f32,
    pub cap: u8,
    components: usize,
    area: f32,
    credit: f32,
    cursor: usize,
    pub changes: u64,
}
impl Graph {
    pub fn new(b: super::Rect, n: usize, reach: f32, lines: f32, area: f32) -> Self {
        Self {
            edges: Vec::with_capacity(n),
            index: Grid::new(b, reach * 1.35),
            degree: vec![0; n],
            last: vec![-1000.; n],
            parent: (0..n).collect(),
            members: vec![1; n],
            adjacency: vec![[0; 5]; n],
            visited: vec![false; n],
            queue: Vec::with_capacity(n),
            candidates: Vec::with_capacity(n),
            cycles: vec![false; n],
            hue_sites: Vec::with_capacity(n / 4),
            axial: vec![0; n],
            scale: 1.,
            spectrum: false,
            floor: b.y as f32,
            height: b.height as f32,
            left: b.x as f32,
            width: b.width as f32,
            reach,
            cap: if lines <= 0. {
                0
            } else {
                (2. + lines * 3.).round() as u8
            },
            components: 0,
            area,
            credit: 0.,
            cursor: 0,
            changes: 0,
        }
    }
    pub(super) fn root(&self, mut i: usize) -> usize {
        while self.parent[i] != i {
            i = self.parent[i];
        }
        i
    }
    fn fits_figure(&self, a: usize, b: usize) -> bool {
        let (a, b) = (self.root(a), self.root(b));
        // Leave space between neighbouring asterisms. A large figure can close
        // a loop, while small associations stay readable as open chains.
        if a == b {
            self.members[a] >= 4 && !self.cycles[a]
        } else {
            self.members[a] + self.members[b] <= 10 && !(self.cycles[a] && self.cycles[b])
        }
    }
    fn cycle_path(&self, node: usize, dest: usize, path: &mut [usize; 5], depth: usize) -> usize {
        path[depth] = node;
        if node == dest {
            return depth + 1;
        }
        if depth + 1 < path.len() {
            for &edge in &self.adjacency[node][..self.degree[node] as usize] {
                let e = self.edges[edge];
                let next = e.a + e.b - node;
                if depth == 0 || next != path[depth - 1] {
                    let n = self.cycle_path(next, dest, path, depth + 1);
                    if n > 0 {
                        return n;
                    }
                }
            }
        }
        0
    }
    // Before a first cycle closes this component is a tree. Follow only its
    // unique return path; branches beyond the polygon do not count as sides.
    fn varied_cycle(&self, a: usize, b: usize, stars: &[Star]) -> bool {
        let mut path = [0; 5];
        let n = self.cycle_path(a, b, &mut path, 0);
        if n < 4 {
            return false;
        }
        let mut shortest = f32::INFINITY;
        let mut longest = 0_f32;
        let mut all_angles_regular = true;
        let mut winding = 0.;
        for j in 0..n {
            let p = stars[path[j]].p;
            let u = stars[path[(j + n - 1) % n]].p - p;
            let v = stars[path[(j + 1) % n]].p - p;
            let len = v.len2();
            shortest = shortest.min(len);
            longest = longest.max(len);
            let cross = u.x * v.y - u.y * v.x;
            let cos = u.dot(v) / (u.len2() * len).sqrt().max(0.0001);
            // A reflex corner necessarily lies outside the interior-angle band.
            all_angles_regular &=
                (-0.76604444..=-0.17364818).contains(&cos) && (j == 0 || cross * winding > 0.);
            winding = cross;
        }
        longest >= 1.8_f32.powi(2) * shortest && !all_angles_regular
    }
    fn accepts(&self, a: usize, b: usize, stars: &[Star], grid: &Grid, forming: bool) -> bool {
        let (p, q) = (stars[a].p, stars[b].p);
        let lo = pt(p.x.min(q.x), p.y.min(q.y));
        let hi = pt(p.x.max(q.x), p.y.max(q.y));
        let d = q - p;
        let length = d.len2();
        let (ra, rb) = (self.root(a), self.root(b));
        if ra == rb && !self.varied_cycle(a, b, stars) {
            return false;
        }
        if ra != rb && self.axial[ra] + self.axial[rb] > 2 {
            return false;
        }
        if d.x.abs().min(d.y.abs()).powi(2) < length * 0.002739 {
            let existing = self.axial[ra] + if ra != rb { self.axial[rb] } else { 0 };
            if existing >= if forming { 1 } else { 2 } {
                return false;
            }
        }
        if d.x == 0. || (forming && d.x.abs() < 0.0005 * d.y.abs()) {
            return false;
        }
        let edge_column = (p.x - self.left).min(self.left + self.width - p.x) < self.width * 0.075
            && d.x.abs() < 0.0524 * d.y.abs();
        let clearance = if forming { 6. } else { 3. };
        for cell in grid.rect(lo - pt(16., 16.), hi + pt(16., 16.)) {
            for &i in &grid.cells[cell] {
                let star = &stars[i];
                if i == a || i == b || !star.alive || star.mag >= 4.5 {
                    continue;
                }
                let offset = star.p - p;
                let along = offset.dot(d);
                let gap = clearance + (0.72 + 2.2 * star.flux.sqrt()) * self.scale;
                if along > 0.
                    && along < length
                    && (d.x * offset.y - d.y * offset.x).powi(2) < gap * gap * length
                {
                    return false;
                }
            }
        }
        let angle = if self.root(a) == self.root(b) {
            0.5868241
        } else {
            0.6710101
        };
        self.index
            .rect(lo - pt(40., 40.), hi + pt(40., 40.))
            .all(|cell| {
                self.index.cells[cell].iter().all(|&i| {
                    let e = self.edges[i];
                    let other = stars[e.b].p - stars[e.a].p;
                    if edge_column
                        && (e.a == a || e.a == b || e.b == a || e.b == b)
                        && other.x.abs() < 0.0524 * other.y.abs()
                    {
                        return false;
                    }
                    !conflict(p, q, stars[e.a].p, stars[e.b].p, angle)
                })
            })
    }
    fn index_edge(&mut self, i: usize, s: &[Star]) {
        let e = self.edges[i];
        for c in self.index.rect(s[e.a].p, s[e.b].p) {
            self.index.add_to_cell(c, i)
        }
        let (ra, rb) = (self.root(e.a), self.root(e.b));
        if ra != rb {
            if self.degree[e.a] == 0 && self.degree[e.b] == 0 {
                self.components += 1;
            }
            if self.degree[e.a] > 0 && self.degree[e.b] > 0 {
                self.components -= 1;
            }
            let (big, small) = if self.members[ra] >= self.members[rb] {
                (ra, rb)
            } else {
                (rb, ra)
            };
            self.parent[small] = big;
            self.members[big] += self.members[small];
            self.cycles[big] |= self.cycles[small];
            self.axial[big] += self.axial[small];
        } else {
            self.cycles[ra] = true;
        }
        let d = s[e.b].p - s[e.a].p;
        let root = self.root(e.a);
        self.axial[root] += u8::from(d.x.abs().min(d.y.abs()).powi(2) < d.len2() * 0.002739);
        for (a, _) in [(e.a, e.b), (e.b, e.a)] {
            self.adjacency[a][self.degree[a] as usize] = i;
            self.degree[a] += 1;
        }
    }
    pub fn cut(&mut self, i: usize, t: f32, stars: &[Star]) {
        let e = &mut self.edges[i];
        if e.live() {
            e.end = t;
            e.cut_at = [stars[e.a].p, stars[e.b].p];
            self.last[e.a] = t;
            self.last[e.b] = t;
            self.changes += 1
        }
    }
    // Capture cue: open one outer stroke of a settled chart. Extension
    // and the subsequent 20-second recognition use the ordinary graph rules.
    pub fn open_figure(&mut self, anchor: Point, time: f32, stars: &[Star]) {
        let leaf = self
            .edges
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                e.live()
                    && e.wave < time
                    && self.members[self.root(e.a)] >= 5
                    && (self.degree[e.a] == 1 || self.degree[e.b] == 1)
            })
            .min_by(|(_, a), (_, b)| {
                (stars[a.a].p - anchor)
                    .len2()
                    .total_cmp(&(stars[b.a].p - anchor).len2())
            })
            .map(|(i, _)| i);
        if let Some(i) = leaf {
            self.cut(i, time, stars);
        }
    }
    pub fn sever(&mut self, id: usize, t: f32, stars: &[Star]) {
        for i in 0..self.edges.len() {
            let e = self.edges[i];
            if e.a == id || e.b == id {
                self.cut(i, t, stars)
            }
        }
    }
    pub fn update(&mut self, s: &[Star], grid: &Grid, time: f32, dt: f32, pace: f32, still: bool) {
        if self.cap == 0 {
            return;
        }
        self.edges.retain(|e| time - e.end < 0.8);
        // Motion can split one mature figure into two. When the atlas is
        // full, let the smaller fragments unwind before drawing fresh ink.
        let budget = (5.6 * self.area).round().max(1.) as usize;
        if self.components > budget {
            self.queue.clear();
            self.queue
                .extend((0..s.len()).filter(|&a| self.parent[a] == a && self.members[a] > 1));
            self.queue
                .sort_unstable_by_key(|&a| (time - s[a].born < 30., self.members[a], a));
            self.visited.fill(false);
            for &a in self.queue.iter().take(self.components - budget) {
                self.visited[a] = true;
            }
            for i in 0..self.edges.len() {
                let e = self.edges[i];
                if e.live() && self.visited[self.root(e.a)] {
                    self.cut(i, time, s);
                }
            }
        }
        if !still {
            for i in 0..self.edges.len() {
                let e = self.edges[i];
                if e.live()
                    && self.degree[e.a] == 1
                    && self.degree[e.b] == 1
                    && time - self.last[e.a].max(self.last[e.b]) > 30.
                {
                    self.cut(i, time, s);
                }
            }
        }
        self.index.clear();
        self.degree.fill(0);
        self.components = 0;
        self.members.fill(1);
        self.cycles.fill(false);
        self.axial.fill(0);
        for (i, p) in self.parent.iter_mut().enumerate() {
            *p = i;
        }
        for i in 0..self.edges.len() {
            let e = self.edges[i];
            if !e.live() {
                continue;
            }
            let length = (s[e.a].p - s[e.b].p).len2();
            if !s[e.a].linkable()
                || !s[e.b].linkable()
                || length > (self.reach * 1.3).powi(2)
                || length < 1024. * self.scale
                || !self.fits_figure(e.a, e.b)
                || !self.accepts(e.a, e.b, s, grid, false)
            {
                self.cut(i, time, s);
            } else {
                self.index_edge(i, s);
            }
        }
        self.credit =
            (self.credit + dt * 1.5 * self.area * pace).min(1. + dt * 1.5 * self.area * pace);
        if self.credit < 1. {
            return;
        }
        // Give a newborn its place in the atlas while the gas is still lit.
        // Parent strokes spend the same ink credit and pass the same geometry
        // checks as any other edge, even if the previous sibling just traced.
        for (a, star) in s.iter().enumerate() {
            let b = star.parent;
            if time - star.born > 28.
                || time - star.born < 1.5
                || !star.linkable()
                || b >= s.len()
                || !s[b].linkable()
                || s[b].group != star.group
                || time - s[b].born < 1.5
                || self.degree[a] >= self.cap
                || self.degree[b] >= self.cap
            {
                continue;
            }
            let d = (star.p - s[b].p).len2();
            if d >= 2025. * self.scale
                && d <= self.reach.powi(2)
                && self.fits_figure(a, b)
                && self.accepts(a, b, s, grid, true)
            {
                self.trace(b, a, time, s);
                if self.credit < 1. {
                    return;
                }
            }
        }
        for _ in 0..s.len() {
            let a = self.cursor;
            self.cursor = (a + 1) % s.len();
            if time - s[a].born < 1.5
                || !s[a].linkable()
                || self.degree[a] >= self.cap
                || time - self.last[a] < 3. / pace
            {
                continue;
            }
            self.candidates.clear();
            for b in grid.near(s[a].p, self.reach) {
                if a == b
                    || time - s[b].born < 1.5
                    || !s[b].linkable()
                    || self.degree[b] >= self.cap
                    || time - self.last[b] < 3. / pace
                {
                    continue;
                }
                if s[a].group != s[b].group && (time - s[a].born < 24. || time - s[b].born < 24.) {
                    continue;
                }
                let d = (s[a].p - s[b].p).len2();
                if d <= self.reach * self.reach && d >= 2025. * self.scale {
                    let lone = self.degree[a] == 0 && self.degree[b] == 0;
                    if lone
                        && (self.components >= budget
                            || s[a].mag.min(s[b].mag) >= 2.
                            || ((s[a].p.y + s[b].p.y) * 0.5 > self.floor + self.height * 0.85
                                && (time * 0.5).floor() as i64 % 2 == 0))
                    {
                        continue;
                    }
                    self.candidates
                        .push((if lone { d + self.reach * self.reach } else { d }, b))
                }
            }
            self.candidates
                .sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            if let Some(&(_, b)) = self
                .candidates
                .iter()
                .find(|&&(_, b)| self.fits_figure(a, b) && self.accepts(a, b, s, grid, true))
            {
                self.trace(a, b, time, s);
                if self.credit < 1. {
                    break;
                }
            }
        }
        // A saturated sky needs no more candidate searches until the next credit.
        self.credit = self.credit.min(0.99 - dt.clamp(0.02, 0.5));
    }
    fn trace(&mut self, a: usize, b: usize, time: f32, s: &[Star]) {
        self.edges.push(Edge {
            a,
            b,
            born: time,
            cut_at: [Point::default(); 2],
            end: f32::INFINITY,
            wave: f32::INFINITY,
            reverse: false,
            figure: a.min(b),
        });
        self.last[a] = time;
        self.last[b] = time;
        self.changes += 1;
        self.index_edge(self.edges.len() - 1, s);
        self.credit -= 1.;
    }
    pub fn mark_linked(&self, stars: &mut [Star]) {
        for (star, &degree) in stars.iter_mut().zip(&self.degree) {
            star.linked = degree > 0;
        }
    }
    pub fn recognise(&mut self, s: &mut [Star], t: f32) {
        self.visited.fill(false);
        self.hue_sites.clear();
        for i in 0..s.len() {
            if self.visited[i] || self.degree[i] == 0 {
                continue;
            }
            self.queue.clear();
            self.queue.push(i);
            self.visited[i] = true;
            let mut last = self.last[i];
            let mut brightest = i;
            let mut j = 0;
            while j < self.queue.len() {
                let a = self.queue[j];
                j += 1;
                last = last.max(self.last[a]);
                if s[a].mag < s[brightest].mag {
                    brightest = a
                }
                for &k in &self.adjacency[a][..self.degree[a] as usize] {
                    let e = self.edges[k];
                    let b = e.a + e.b - a;
                    if !self.visited[b] {
                        self.visited[b] = true;
                        self.queue.push(b)
                    }
                }
            }
            let recognized = (4..=10).contains(&self.queue.len()) && t - last >= 20.;
            let hue = if self.spectrum {
                let site = s[brightest].p;
                let mut distance = [1200_f32.powi(2); 6];
                for &(p, h) in &self.hue_sites {
                    distance[h] = distance[h].min((site - p).len2());
                }
                // Keep a figure's previous colour whenever its neighbours allow it.
                let first = s[brightest].figure;
                let mut hue = first;
                for step in 1..6 {
                    let candidate = (first + step) % 6;
                    if distance[candidate] > distance[hue] {
                        hue = candidate;
                    }
                }
                self.hue_sites.push((site, hue));
                hue
            } else {
                brightest
            };
            for &a in &self.queue {
                s[a].figure = hue;
                for &k in &self.adjacency[a][..self.degree[a] as usize] {
                    let e = &mut self.edges[k];
                    e.figure = brightest;
                    e.wave = f32::INFINITY;
                }
            }
            if recognized {
                for &a in &self.queue {
                    self.visited[a] = false
                }
                self.queue.clear();
                self.queue.push(brightest);
                self.visited[brightest] = true;
                let mut j = 0;
                let mut edge_seq = 0;
                while j < self.queue.len() {
                    let a = self.queue[j];
                    j += 1;
                    for &k in &self.adjacency[a][..self.degree[a] as usize] {
                        let e = &mut self.edges[k];
                        if !e.wave.is_finite() {
                            e.wave = last + 20. + edge_seq as f32 * 0.25;
                            e.reverse = e.b == a;
                            let arrival = e.a + e.b - a;
                            s[arrival].flare = e.wave + 0.25;
                            if edge_seq == 0 {
                                s[a].flare = e.wave;
                            }
                            edge_seq += 1
                        }
                        let b = e.a + e.b - a;
                        if !self.visited[b] {
                            self.visited[b] = true;
                            self.queue.push(b)
                        }
                    }
                }
            }
        }
    }
    #[cfg(test)]
    pub fn verify(&self, s: &[Star]) {
        let mut actual = vec![0u8; s.len()];
        for (i, e) in self.edges.iter().enumerate().filter(|(_, e)| e.live()) {
            let (p, q) = (s[e.a].p, s[e.b].p);
            let d = q - p;
            let length = d.len();
            assert!(
                length + 0.001 >= 32. * self.scale.sqrt() && length <= 1.3 * self.reach + 0.001
            );
            for (j, star) in s.iter().enumerate().filter(|(_, s)| s.alive && s.mag < 4.5) {
                let along = (star.p - p).dot(d) / d.len2();
                if j != e.a && j != e.b && along > 0. && along < 1. {
                    let clearance = 3. + (0.72 + 2.2 * star.flux.sqrt()) * self.scale;
                    assert!((star.p - (p + d * along)).len() + 0.001 >= clearance);
                }
            }
            actual[e.a] += 1;
            actual[e.b] += 1;
            for f in self.edges[..i].iter().filter(|f| f.live()) {
                assert!(!conflict(p, q, s[f.a].p, s[f.b].p, 0.6710101));
                if let Some(shared) = [e.a, e.b].into_iter().find(|k| *k == f.a || *k == f.b) {
                    let u = (s[e.a + e.b - shared].p - s[shared].p).unit();
                    let v = (s[f.a + f.b - shared].p - s[shared].p).unit();
                    assert!(u.dot(v).clamp(-1., 1.).acos().to_degrees() >= 34.999);
                } else {
                    let delta = |a: usize, b: usize| {
                        (
                            f64::from(s[a].p.x) - f64::from(s[b].p.x),
                            f64::from(s[a].p.y) - f64::from(s[b].p.y),
                        )
                    };
                    let cross = |a: (f64, f64), b: (f64, f64)| a.0 * b.1 - a.1 * b.0;
                    let u = delta(e.b, e.a);
                    let v = delta(f.b, f.a);
                    let d = delta(f.a, e.a);
                    let determinant = cross(u, v);
                    if determinant != 0. {
                        let ta = cross(d, v) / determinant;
                        let tb = cross(d, u) / determinant;
                        assert!(!((0.0..=1.0).contains(&ta) && (0.0..=1.0).contains(&tb)));
                    } else if cross(d, u) == 0. {
                        let axis = |p: Point| if u.0.abs() >= u.1.abs() { p.x } else { p.y };
                        let (a, b, c, d) = (
                            axis(s[e.a].p),
                            axis(s[e.b].p),
                            axis(s[f.a].p),
                            axis(s[f.b].p),
                        );
                        assert!(a.max(b) < c.min(d) || c.max(d) < a.min(b));
                    }
                }
            }
        }
        assert_eq!(actual, self.degree);
        assert!(actual.into_iter().all(|d| d <= self.cap));
    }
}

#[cfg(test)]
mod geometry_tests {
    use super::super::{Pcg32, Rect};
    use super::*;

    fn fixture(coords: &[(f32, f32)]) -> (Graph, Vec<Star>, Grid) {
        let mut rng = Pcg32::new(7, 1);
        let bounds = Rect {
            x: 0.,
            y: 0.,
            width: 500.,
            height: 500.,
        };
        let graph = Graph::new(bounds, coords.len(), 160., 1., 1.);
        let mut grid = Grid::new(bounds, 80.);
        let stars = coords
            .iter()
            .enumerate()
            .map(|(i, &(x, y))| {
                let mut star = Star::new(pt(x, y), &mut rng, true);
                star.set_magnitude(1.5);
                grid.insert(star.p, i);
                star
            })
            .collect();
        (graph, stars, grid)
    }
    fn edge(a: usize, b: usize, born: f32) -> Edge {
        Edge {
            a,
            b,
            born,
            end: f32::INFINITY,
            cut_at: [Point::default(); 2],
            wave: f32::INFINITY,
            reverse: false,
            figure: a,
        }
    }
    #[test]
    fn merged_nodes_railways_crossings_and_angle_boundaries() {
        let (a, b) = (pt(40., 60.), pt(180., 60.));
        for (c, d) in [
            (pt(50., 67.), pt(190., 68.)),  // same apparent nodes
            (pt(70., 88.), pt(210., 93.)),  // overlapping tracks, different nodes
            (pt(108., 19.), pt(109., 99.)), // crossing
            (a, a + Point::polar(34_f32.to_radians()) * 100.),
        ] {
            assert!(conflict(a, b, c, d, 0.6710101));
        }
        assert!(!conflict(
            a,
            b,
            a,
            a + Point::polar(36_f32.to_radians()) * 100.,
            0.6710101
        ));
        assert!(conflict(
            a,
            b,
            a,
            a + Point::polar(39_f32.to_radians()) * 100.,
            0.5868241
        ));
        assert!(!conflict(a, b, pt(80., 105.), pt(220., 105.), 0.6710101));
    }
    #[test]
    fn underage_lines_retract_in_still_night_on_length_crossing_or_occlusion() {
        for (q, r, s) in [
            ((259., 50.), (400., 300.), (400., 420.)), // >1.3R
            ((78., 50.), (400., 300.), (400., 420.)),  // less than 32 px
            ((185., 50.), (100., 53.), (110., 130.)),  // bright star in ink
        ] {
            let (mut graph, stars, grid) = fixture(&[(50., 50.), q, r, s]);
            graph.edges.push(edge(0, 1, 0.));
            graph.update(&stars, &grid, 0.1, 0., 1., true);
            assert!(!graph.edges[0].live());
        }
        let (mut graph, stars, grid) =
            fixture(&[(50., 80.), (180., 80.), (100., 20.), (100., 155.)]);
        graph.edges.extend([edge(0, 1, 0.), edge(2, 3, 0.05)]);
        graph.update(&stars, &grid, 0.1, 0., 1., true);
        assert!(graph.edges[0].live() && !graph.edges[1].live());
    }
    #[test]
    fn loops_require_an_irregular_outline_of_at_most_five_stars() {
        let check = |coords: &[(f32, f32)]| {
            let (mut graph, stars, _) = fixture(coords);
            for i in 1..stars.len() {
                graph.edges.push(edge(i - 1, i, 0.));
                graph.index_edge(i - 1, &stars);
            }
            graph.varied_cycle(0, stars.len() - 1, &stars)
        };
        for n in [4, 5, 6] {
            let coords: Vec<_> = (0..n)
                .map(|i| {
                    let d = Point::polar(i as f32 * std::f32::consts::TAU / n as f32) * 80.;
                    (150. + d.x, 150. + d.y)
                })
                .collect();
            assert!(!check(&coords));
        }
        // Irregular side lengths alone cannot redeem five regular turns.
        let mut p = pt(200., 200.);
        let coords: Vec<_> = [1., 2., 0.381966, 2., 1.]
            .into_iter()
            .enumerate()
            .map(|(i, len)| {
                let a = p;
                p += Point::polar(i as f32 * std::f32::consts::TAU / 5.) * (len * 50.);
                (a.x, a.y)
            })
            .collect();
        assert!(!check(&coords));
        assert!(check(&[(40., 40.), (95., 40.), (200., 120.), (45., 185.)]));
    }
    #[test]
    fn forming_lines_need_room_for_star_gaps_and_never_add_a_second_cycle() {
        let (mut g, stars, grid) = fixture(&[(40., 40.), (78., 40.), (95., 48.)]);
        g.update(&stars, &grid, 4., 1., 1., false);
        assert!(
            g.edges
                .iter()
                .all(|e| (stars[e.a].p - stars[e.b].p).len() >= 45.)
        );
        let (mut g, stars, _) = fixture(&[
            (50., 50.),
            (150., 50.),
            (150., 150.),
            (50., 150.),
            (300., 100.),
        ]);
        for (a, b) in [(0, 1), (1, 2), (2, 3), (3, 0), (1, 4)] {
            assert!(g.fits_figure(a, b));
            g.edges.push(edge(a, b, 0.));
            g.index_edge(g.edges.len() - 1, &stars);
        }
        assert!(!g.fits_figure(2, 4));
    }
}
