// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{Point, World, MAX_FOOD, MAX_SEGMENTS, MAX_SNAKES};
use std::ops::RangeInclusive;
pub(super) const CELLS: usize = 128 * 128;
pub(super) const FILL_LIMIT: usize = 512;
/// Shared by body safety, food discovery, route search and area estimates.
/// Storage is allocated once, independent of arena resize/population changes.
#[derive(Clone)]
pub(super) struct Spatial {
    pub heads: Vec<i32>,
    pub next: Vec<i32>,
    pub food_heads: Vec<i32>,
    pub food_next: [i32; MAX_FOOD],
    pub weight: Vec<f64>,
    pub occupied: Vec<u16>,
    navigable: Vec<u16>,
    release: Vec<[f32; MAX_SNAKES]>,
    space_release: Vec<[f32; MAX_SNAKES]>,
    visited: Vec<u32>,
    area_seen: Vec<u32>,
    future: Vec<u32>,
    future_stamp: u32,
    future_active: bool,
    area_label: Vec<usize>,
    area_stamp: u32,
    area_mask: u16,
    area_count: usize,
    area_limit: usize,
    area_time: f64,
    area_result: [(usize,bool); 32],
    stamp: u32,
    queue: [usize; FILL_LIMIT],
    parent: [usize; FILL_LIMIT],
    depth: [u16; FILL_LIMIT],
    pub cols: usize,
    pub rows: usize,
    pub dx: f64,
    pub dy: f64,
    pub max_motion: f64,
    wrap: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangular_queries_cover_unique_buckets_on_both_axes() {
        let mut grid=Spatial::new();
        let bounds=[(-1,1),(-2,2),(-7,8),(2,13),(-12,-7),(0,0),(2,1)];
        for wrap in [false,true] {grid.wrap=wrap;
            for cols in 1..=6 {for rows in 1..=6 {
                grid.cols=cols;grid.rows=rows;
                for root in 0..cols*rows {
                    for (x0,x1) in bounds {for (y0,y1) in bounds {
                        let mut expected=[false;36];
                        for y in y0..=y1 {for x in x0..=x1 {
                            if let Some(k)=grid.offset(root,x,y) {expected[k]=true;}
                        }}
                        let mut seen=[false;36];
                        let (xs,ys)=grid.spans(root,x0,x1,y0,y1);
                        for y in ys {for x in xs.clone() {
                            let k=grid.offset(root,x,y).unwrap();
                            assert!(!seen[k],"wrap={wrap} {cols}x{rows}, root={root}, bounds={x0}..{x1}/{y0}..{y1}");
                            seen[k]=true;
                        }}
                        assert_eq!(seen,expected);
                    }}
                }
            }}
        }
    }

    #[test]
    fn discovery_rings_cover_every_cell_once_including_even_extents() {
        let mut grid=Spatial::new();
        for wrap in [false,true] {grid.wrap=wrap;
            for cols in 1..=6 {for rows in 1..=6 {
                grid.cols=cols;grid.rows=rows;
                for root in 0..cols*rows {
                    let mut seen=[false;36];
                    for ring in 0..=cols.max(rows) as isize {
                        for (x,y) in grid.ring_offsets(ring) {
                            if let Some(k)=grid.offset(root,x,y) {
                                assert!(!seen[k],"wrap={wrap} {cols}x{rows}, root={root}, ring={ring}");
                                seen[k]=true;
                            }
                        }
                    }
                    assert!(seen[..cols*rows].iter().all(|&v|v));
                }
            }}
        }
    }

    #[test]
    fn local_clearance_cluster_and_cardinal_neighbours_are_unique() {
        let mut grid=Spatial::new();
        for wrap in [false,true] {grid.wrap=wrap;
            for cols in 1..=6 {for rows in 1..=6 {
                grid.cols=cols;grid.rows=rows;
                for k in 0..cols*rows {grid.weight[k]=(k+1) as f64;}
                for root in 0..cols*rows {
                    let mut expected=[false;36];
                    let mut cardinal=[false;36];
                    for y in -1..=1 {for x in -1..=1 {
                        if let Some(k)=grid.offset(root,x,y) {
                            expected[k]=true;
                            if x*x+y*y==1 && k!=root {cardinal[k]=true;}
                        }
                    }}
                    let mut seen=[false;36];
                    let (xs,ys)=grid.local_spans(1);
                    for y in ys {for x in xs.clone() {
                        if let Some(k)=grid.offset(root,x,y) {
                            assert!(!seen[k]);seen[k]=true;
                        }
                    }}
                    assert_eq!(seen,expected,"clearance wrap={wrap} {cols}x{rows}");
                    let weight=(0..cols*rows).filter(|&k|expected[k]).map(|k|grid.weight[k]).sum::<f64>();
                    assert_eq!(grid.cluster_weight(root),weight,"cluster wrap={wrap} {cols}x{rows}");
                    seen.fill(false);
                    for (x,y) in grid.neighbour_offsets() {
                        if let Some(k)=grid.offset(root,x,y) {
                            assert_ne!(k,root);assert!(!seen[k]);seen[k]=true;
                        }
                    }
                    assert_eq!(seen,cardinal,"cardinal wrap={wrap} {cols}x{rows}");
                }
            }}
        }
    }

    #[test]
    fn small_grid_area_and_waypoints_keep_their_coverage_and_caps() {
        let mut grid=Spatial::new();grid.dx=1.0;grid.dy=1.0;
        for wrap in [false,true] {grid.wrap=wrap;
            for cols in 1..=3 {for rows in 1..=3 {
                grid.cols=cols;grid.rows=rows;
                let n=cols*rows;
                for root in 0..n {
                    let start=grid.center(root);
                    grid.area_count=0;
                    assert_eq!(grid.area_bounded(start,u16::MAX,n),(n,false));
                    if n>1 {
                        assert_eq!(grid.area_bounded(start,u16::MAX,n-1),(n-1,true));
                    }
                    for target in 0..n {
                        let waypoint=grid.waypoint(start,grid.center(target),u16::MAX);
                        if target==root {assert_eq!(waypoint,None);} else {
                            let k=grid.key(waypoint.expect("all empty cells are connected"));
                            assert_ne!(k,root);assert!(k<n);
                        }
                    }
                }
            }}
        }
    }
}
impl Spatial {
    pub fn new() -> Self {
        Self { heads: vec![-1; CELLS], next: vec![-1; MAX_SNAKES*MAX_SEGMENTS],
            food_heads: vec![-1; CELLS], food_next: [-1; MAX_FOOD], weight: vec![0.0; CELLS],
            occupied: vec![0; CELLS],navigable:vec![0;CELLS],release:vec![[0.0;MAX_SNAKES];CELLS],space_release:vec![[0.0;MAX_SNAKES];CELLS], visited: vec![0; CELLS], stamp: 0,
            area_seen: vec![0;CELLS],future:vec![0;CELLS],future_stamp:0,future_active:false,area_label:vec![0;CELLS],area_stamp:0,area_mask:0,area_count:0,area_limit:FILL_LIMIT,area_time:-1.0,area_result:[(0,false);32],
            queue: [0; FILL_LIMIT], parent: [0; FILL_LIMIT],depth:[0;FILL_LIMIT], cols: 1, rows: 1,
            dx: 1.0, dy: 1.0,max_motion:0.0, wrap: false }
    }
    pub fn rebuild(&mut self, w: &World) {
        self.area_mask=0;
        let c = w.config();
        let side = (c.base_radius()*5.0).max(28.0);
        self.cols = (c.width/side).ceil().clamp(1.0,128.0) as usize;
        self.rows = (c.height/side).ceil().clamp(1.0,128.0) as usize;
        self.dx = c.width/self.cols as f64; self.dy = c.height/self.rows as f64;
        self.wrap = !c.deadly_walls;
        let n = self.cols*self.rows;
        self.heads[..n].fill(-1); self.food_heads[..n].fill(-1);
        self.occupied[..n].fill(0); self.weight[..n].fill(0.0);
        self.navigable[..n].fill(0);self.release[..n].fill([0.0;MAX_SNAKES]);
        self.space_release[..n].fill([0.0;MAX_SNAKES]);
        let mut motion_squared=0.0_f64;
        for s in w.snakes().filter(|s|s.alive) {
            let speed=w.motion_limits(s.id as usize,0.0).unwrap().0;
            let rate=s.radius*1.18/(speed*0.65).max(1.0);
            let growth_delay=w.tail_growth_delay(s.id as usize).unwrap();
            for (j, seg) in s.segments.iter().enumerate().skip(1) {
                motion_squared=motion_squared.max(w.distance_squared(seg.previous,seg.current));
                let key = self.key(w.canonical_point(seg.current));
                let encoded = s.id as usize*MAX_SEGMENTS+j;
                self.next[encoded] = self.heads[key]; self.heads[key] = encoded as i32;
                // Own neck exclusion is dealt with separately in exact safety.
                // Keep the neck in the occupancy mask for OTHER snakes.
                self.occupied[key] |= 1 << s.id;
                let release=((s.segments.len()-j) as f64*rate+growth_delay+0.15) as f32;
                self.release[key][s.id as usize]=self.release[key][s.id as usize].max(release);
            }
        }
        self.max_motion=motion_squared.sqrt();
        // A one-cell clearance band keeps coarse connectivity from treating a
        // head-width crack as room for a minimum-radius turn. Tail release is
        // conservative, and the continuous rollout still proves motion safety.
        for key in 0..n {
            let mask=self.occupied[key];if mask==0 {continue;}
            let (xs,ys)=self.local_spans(1);
            for y in ys {for x in xs.clone() {
                if let Some(k)=self.offset(key,x,y) {
                    self.navigable[k]|=mask;
                    let mut bits=mask;
                    while bits!=0 {
                        let id=bits.trailing_zeros() as usize;bits&=bits-1;
                        self.space_release[k][id]=self.space_release[k][id].max(self.release[key][id]);
                    }
                }
            }}
        }
        for (i,f) in w.foods().enumerate() {
            let key = self.key(f.position);
            self.food_next[i] = self.food_heads[key]; self.food_heads[key] = i as i32;
            self.weight[key] += f.value;
        }
    }
    pub fn key(&self,p: Point) -> usize {
        let x = ((p.x/self.dx).floor() as isize).clamp(0,self.cols as isize-1) as usize;
        let y = ((p.y/self.dy).floor() as isize).clamp(0,self.rows as isize-1) as usize;
        y*self.cols+x
    }
    pub fn offset(&self,key:usize,x:isize,y:isize) -> Option<usize> {
        let mut cx = (key%self.cols) as isize+x;
        let mut cy = (key/self.cols) as isize+y;
        if self.wrap {
            if cx<0 || cx>=self.cols as isize {cx=cx.rem_euclid(self.cols as isize);}
            if cy<0 || cy>=self.rows as isize {cy=cy.rem_euclid(self.rows as isize);}
        }
        if cx<0 || cy<0 || cx>=self.cols as isize || cy>=self.rows as isize {None}
        else {Some(cy as usize*self.cols+cx as usize)}
    }
    /// Consecutive offsets can visit at most one full wrapped axis. Preserve
    /// the first offset so swept bounds retain their original traversal order.
    pub fn spans(&self,key:usize,x0:isize,x1:isize,y0:isize,y1:isize)
        -> (RangeInclusive<isize>,RangeInclusive<isize>) {
        let (x0,x1,y0,y1)=if self.wrap {(x0,x1,y0,y1)} else {
            let x=(key%self.cols) as isize;let y=(key/self.cols) as isize;
            (x0.max(-x),x1.min(self.cols as isize-1-x),y0.max(-y),y1.min(self.rows as isize-1-y))
        };
        (Self::axis_span(x0,x1,self.cols,self.wrap),Self::axis_span(y0,y1,self.rows,self.wrap))
    }
    fn axis_span(first:isize,last:isize,extent:usize,wrap:bool) -> RangeInclusive<isize> {
        first..=if wrap {last.min(first.saturating_add(extent as isize-1))} else {last}
    }
    /// Use the closest representative of every wrapped cell. The negative
    /// half-axis wins the tie on even extents; one-cell axes use zero only.
    fn local_spans(&self,radius:isize) -> (RangeInclusive<isize>,RangeInclusive<isize>) {
        let x0=if self.wrap {-radius.min(self.cols as isize/2)} else {-radius};
        let y0=if self.wrap {-radius.min(self.rows as isize/2)} else {-radius};
        (Self::axis_span(x0,radius,self.cols,self.wrap),Self::axis_span(y0,radius,self.rows,self.wrap))
    }
    pub fn ring_offsets(&self,ring:isize) -> impl Iterator<Item=(isize,isize)> + use<> {
        let (xs,ys)=self.local_spans(ring);
        (0..if ring==0 {1} else {8*ring}).filter_map(move |edge| {
            let (x,y)=if ring==0 {(0,0)} else if edge<2*ring {(-ring+edge,-ring)}
                else if edge<4*ring {(ring,-ring+edge-2*ring)}
                else if edge<6*ring {(ring-(edge-4*ring),ring)}
                else {(-ring,ring-(edge-6*ring))};
            (xs.contains(&x) && ys.contains(&y)).then_some((x,y))
        })
    }
    fn neighbour_offsets(&self) -> impl Iterator<Item=(isize,isize)> + use<> {
        let (xs,ys)=self.local_spans(1);
        [(1,0),(-1,0),(0,1),(0,-1)].into_iter()
            .filter(move |(x,y)|xs.contains(x) && ys.contains(y))
    }
    pub fn cluster_weight(&self,key:usize) -> f64 {
        let (xs,ys)=self.local_spans(1);
        let mut weight=0.0;
        for y in ys {for x in xs.clone() {
            if let Some(k)=self.offset(key,x,y) {weight+=self.weight[k];}
        }}
        weight
    }
    pub fn center(&self,key:usize) -> Point {
        Point {x:(key%self.cols) as f64*self.dx+self.dx*0.5,
            y:(key/self.cols) as f64*self.dy+self.dy*0.5}
    }
    fn begin_search(&mut self,start:usize) {
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp==0 {self.visited.fill(0); self.stamp=1;}
        self.queue[0]=start; self.parent[0]=0;self.depth[0]=0; self.visited[start]=self.stamp;
    }
    /// Returns a lower bound and a flag for unresolved search. Capped results
    /// are never presented as an exact area. Mask follows self-collision mode.
    #[cfg(test)]
    pub fn area(&mut self,start:Point,mask:u16) -> (usize,bool) {
        self.area_bounded(start,mask,FILL_LIMIT)
    }
    #[cfg(test)]
    pub fn area_bounded(&mut self,start:Point,mask:u16,limit:usize) -> (usize,bool) {
        self.space(start,mask,limit,-1.0)
    }
    pub fn space(&mut self,start:Point,mask:u16,limit:usize,time:f64)->(usize,bool) {
        let limit=limit.clamp(1,FILL_LIMIT);
        let root=self.key(start);
        if self.area_count==0 || self.area_mask!=mask || self.area_limit!=limit || self.area_time!=time || self.area_count==self.area_result.len() {
            self.area_mask=mask;self.area_limit=limit;self.area_time=time;self.area_count=0;
            self.area_stamp=self.area_stamp.wrapping_add(1);
            if self.area_stamp==0 {self.area_seen.fill(0);self.area_stamp=1;}
        }
        if self.area_seen[root]==self.area_stamp {return self.area_result[self.area_label[root]];}
        let narrow_start=time>=0.0 && self.blocked(root,mask,time);
        self.begin_search(root);
        let mut read=0; let mut write=1;
        while read<write {
            let key=self.queue[read];let depth=self.depth[read];read+=1;
            for (x,y) in self.neighbour_offsets() {
                if let Some(k)=self.offset(key,x,y) {
                    // A checked heading may leave a narrow starting cell
                    // through two physically free cells before reaching room
                    // to turn. Do not declare that starting cell a sealed pocket.
                    if self.visited[k]!=self.stamp && (!self.blocked(k,mask,time)
                        || (narrow_start && depth<2 && !self.physical_blocked(k,mask,time))) {
                        if write==limit {return self.finish_area(write,true);}
                        self.visited[k]=self.stamp; self.queue[write]=k;self.depth[write]=depth+1;write+=1;
                    }
                }
            }
        }
        self.finish_area(write,false)
    }
    /// Evaluate the enclosure made by this particular future trajectory.
    /// The newest ten body samples are the collision-exempt neck; old
    /// deposited samples disappear after the whole body has travelled past.
    /// Scratch marks are stamped, so separate candidates never share trails.
    pub fn trajectory_space(&mut self,start:Point,mask:u16,limit:usize,time:f64,
        path:&[Point],neck_ticks:usize,body_ticks:usize)->(usize,bool) {
        self.future_stamp=self.future_stamp.wrapping_add(1);
        if self.future_stamp==0 {self.future.fill(0);self.future_stamp=1;}
        let end=path.len().saturating_sub(neck_ticks+1);
        let begin=path.len().saturating_sub(body_ticks+1);
        if end>begin {
            for &p in path[begin..end].iter().step_by(2) {
                let key=self.key(p);
                for y in -1..=1 {for x in -1..=1 {
                    if let Some(k)=self.offset(key,x,y) {self.future[k]=self.future_stamp;}
                }}
            }
        }
        self.future_active=true;self.area_count=0;
        let result=self.space(start,mask,limit,time);
        self.future_active=false;self.area_count=0;
        result
    }
    fn physical_blocked(&self,key:usize,mask:u16,time:f64)->bool {
        if self.future_active && self.future[key]==self.future_stamp {return true;}
        let mut bits=self.occupied[key]&mask;
        while bits!=0 {
            let id=bits.trailing_zeros() as usize;bits&=bits-1;
            if self.release[key][id] as f64>time {return true;}
        }
        false
    }
    fn blocked(&self,key:usize,mask:u16,time:f64)->bool {
        if self.future_active && self.future[key]==self.future_stamp {return true;}
        if time<0.0 {return self.occupied[key]&mask!=0;}
        if !self.wrap && (key%self.cols==0 || key%self.cols+1==self.cols || key/self.cols==0 || key/self.cols+1==self.rows) {return true;}
        let mut bits=self.navigable[key]&mask;
        while bits!=0 {
            let id=bits.trailing_zeros() as usize;bits&=bits-1;
            if self.space_release[key][id] as f64>time {return true;}
        }
        false
    }
    fn finish_area(&mut self,write:usize,capped:bool)->(usize,bool) {
        let label=self.area_count;self.area_count+=1;
        self.area_result[label]=(write,capped);
        let root=self.queue[0];
        let narrow_start=self.blocked(root,self.area_mask,self.area_time);
        for &key in &self.queue[..write] {
            if key==root || (!narrow_start && !self.blocked(key,self.area_mask,self.area_time)) {
                self.area_seen[key]=self.area_stamp;self.area_label[key]=label;
            }
        }
        (write,capped)
    }
    /// Bounded coarse BFS; the returned waypoint is guidance, never a safety
    /// certificate. Exact continuous rollouts validate every published move.
    pub fn waypoint(&mut self,start:Point,goal:Point,mask:u16) -> Option<Point> {
        let root=self.key(start); let target=self.key(goal);
        if root==target {return None;}
        self.begin_search(root);
        let mut read=0; let mut write=1;
        while read<write {
            let key=self.queue[read];
            if key==target {
                let mut at=read;
                let mut depth=0;let mut nodes=[0usize; FILL_LIMIT];
                while self.parent[at]!=0 {nodes[depth]=at;depth+=1;at=self.parent[at];}
                if depth>=2 {at=nodes[depth-2];}
                return Some(self.center(self.queue[at]));
            }
            for (x,y) in self.neighbour_offsets() {
                if let Some(k)=self.offset(key,x,y) {
                    if self.visited[k]!=self.stamp && self.occupied[k]&mask==0 {
                        if write==FILL_LIMIT {return None;}
                        self.visited[k]=self.stamp; self.queue[write]=k;
                        self.parent[write]=read; write+=1;
                    }
                }
            }
            read+=1;
        }
        None
    }
}
