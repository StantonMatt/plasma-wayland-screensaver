// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{Point, World, MAX_FOOD, MAX_SEGMENTS, MAX_SNAKES};
use std::ops::RangeInclusive;
pub(super) const CELLS: usize = 256 * 128;
pub(super) const FILL_LIMIT: usize = 512;
const WORDS:usize=4*128;
const SPACE_SLOTS:usize=32;
#[derive(Clone,Copy,Default)]
struct SpaceKey {mask:u16,time:f64,epoch:u32}
const NARROW_PROOF:u16=1<<15;
#[derive(Clone,Copy,Default)]
struct RoomProof {time:f64,epoch:u32,count:u16,mask:u16}
#[derive(Clone,Copy,Default)]
struct WordProof {cells:u64,proof:RoomProof}
#[derive(Clone)]
struct RoomScratch {reached:[u64;WORDS],seeds:[u64;WORDS],dirty:[u64;WORDS/64],queued:[u64;WORDS/64],pending:[usize;WORDS],touched:[usize;WORDS]}
impl RoomScratch {
    fn new()->Self {Self {reached:[0;WORDS],seeds:[0;WORDS],dirty:[0;WORDS/64],queued:[0;WORDS/64],pending:[0;WORDS],touched:[0;WORDS]}}
}

/// Bound a narrow-phase contact plus its safety margin and possible motion.
/// Pass maximum physical widths for broad queries, exact tapered widths for
/// per-record tests. Effect reserves scale the margin as well as the contact.
#[inline]
pub(super) fn query_radius(contact: f64, margin: f64, scale: f64, motion: f64) -> f64 {
    (contact + margin) * scale + motion
}

/// Shared by body safety, food discovery, route search and area estimates.
/// Storage is allocated once, independent of arena resize/population changes.
#[derive(Clone)]
pub(super) struct Spatial {
    pub profile_enabled: bool,
    pub profile: [u128;9],
    pub counts: [u64;11],
    word_keys:[SpaceKey;SPACE_SLOTS],
    word_epoch:u32,
    word_next:usize,
    word_seen:Vec<u32>,
    word_open:Vec<u64>,
    word_free_time:Vec<f64>,
    future_words:[u64;WORDS],
    occupied_words:Vec<[u64;MAX_SNAKES]>,
    navigable_words:Vec<[u64;MAX_SNAKES]>,
    word_min:Vec<[f32;MAX_SNAKES]>,
    word_max:Vec<[f32;MAX_SNAKES]>,
    boundary_words:[u64;WORDS],
    word_owners:[u16;WORDS],
    occupied_cells:Vec<u16>,
    navigable_cells:Vec<u16>,
    food_cells:Vec<u16>,
    food_count:usize,
    occupied_count:usize,
    navigable_count:usize,
    proof_epoch:u32,
    proofs:Vec<RoomProof>,
    word_proofs:Vec<[WordProof;2]>,
    proof_next:[u8;WORDS],
    room_scratch:[Option<Box<RoomScratch>>;2],
    cell_word:Vec<u16>,
    cell_bit:Vec<u8>,
    pub heads: Vec<i32>,
    pub next: Vec<i32>,
    pub widths: Vec<f64>,
    lengths: [usize; MAX_SNAKES],
    pub food_heads: Vec<i32>,
    pub food_next: [i32; MAX_FOOD],
    pub weight: Vec<f64>,
    pub occupied: Vec<u16>,
    navigable: Vec<u16>,
    neighbours: Vec<[u16; 4]>,
    clearance_neighbours: Vec<[u16; 9]>,
    boundary: Vec<bool>,
    topology: (usize, usize, bool),
    wall_band: f64,
    boundary_geometry: (f64, f64, f64),
    occupied_max:Vec<f32>,
    navigable_max:Vec<f32>,
    release: Vec<f32>,
    space_release: Vec<f32>,
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
    fn deadly_wall_borders_survive_geometry_rounding_in_both_grids() {
        use crate::{Config, RuleSet};
        let mut grid=Spatial::new();
        for rules in [RuleSet::Classic,RuleSet::V2] {
            for (width,height) in [(80.0,80.0),(1280.0,720.0),(1200.0,800.0),
                (3440.0,1440.0),(7920.0,1440.0),(16384.0,16384.0),
                (80.0,16384.0),(16384.0,80.0)] {
                for scale in [0.0,1.0,70.0,100.0,185.0,200.0,1000.0] {
                    for deadly_walls in [true,false] {
                        let cfg=Config {width,height,scale,rules,deadly_walls,..Config::default()};
                        // A head-only fixture leaves the body-occupancy grid empty.
                        let world=World::diagnostic_arena(cfg,&[(Point{x:width*0.5,y:height*0.5},0.0,1,0.0)],&[]).unwrap();
                        grid.rebuild(&world);
                        for time in [0.0,1.0,99.0] {
                            let slot=grid.space_slot(0,time);
                            for row in 0..grid.rows {for word in 0..grid.cols.div_ceil(64) {
                                let open=grid.open_word(slot,row,word);
                                for col in word*64..((word+1)*64).min(grid.cols) {
                                    let cell=row*grid.cols+col;
                                    let border=col==0 || col+1==grid.cols || row==0 || row+1==grid.rows;
                                    let band=if rules==RuleSet::V2 {cfg.base_radius()*4.5} else {0.0};
                                    let expected=deadly_walls && (border
                                        || col as f64*grid.dx<=band || (col+1) as f64*grid.dx>=width-band
                                        || row as f64*grid.dy<=band || (row+1) as f64*grid.dy>=height-band);
                                    let bit=1<<(col%64);
                                    assert_eq!(grid.boundary[cell],expected,
                                        "{rules:?} {width}x{height}@{scale} walls={deadly_walls} cell={col},{row}");
                                    assert_eq!(grid.boundary_words[row*4+word]&bit!=0,grid.boundary[cell]);
                                    assert_eq!(grid.blocked(cell,0,time),grid.boundary[cell]);
                                    assert_eq!(grid.blocked(cell,u16::MAX,time),grid.boundary[cell]);
                                    assert_eq!(open&bit==0,grid.boundary[cell]);
                                }
                            }}
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn truncated_cell_keys_match_floor_at_boundaries_and_external_values() {
        let mut grid=Spatial::new();grid.cols=237;grid.rows=91;
        for scale in [0.125,1.0,31.337,1.0e30] {
            grid.dx=scale;grid.dy=scale*1.3;
            let reference=|p:Point| {
                let x=((p.x/grid.dx).floor() as isize).clamp(0,grid.cols as isize-1) as usize;
                let y=((p.y/grid.dy).floor() as isize).clamp(0,grid.rows as isize-1) as usize;
                y*grid.cols+x
            };
            for value in [f64::NEG_INFINITY,-1.0e300,-0.0,0.0,1.0e300,f64::INFINITY,f64::NAN] {
                let p=Point{x:value,y:value};assert_eq!(grid.key(p),reference(p));
            }
            for cell in -10..=256 {
                let x=cell as f64*grid.dx;let y=cell as f64*grid.dy;
                for p in [Point{x,y},Point{x:x.next_down(),y:y.next_down()},Point{x:x.next_up(),y:y.next_up()}] {
                    assert_eq!(grid.key(p),reference(p),"scale={scale} p={p:?}");
                }
            }
        }
    }

    #[test]
    fn word_flood_matches_ordered_bfs_across_wrap_release_and_future_trails() {
        let mut fast=Spatial::new();let mut reference=Spatial::new();
        let mut random=0x123456789abcdefu64;
        let mut draw=|| {random^=random<<13;random^=random>>7;random^=random<<17;random};
        for wrap in [false,true] {
            for cols in [1,2,3,63,64,65,127,128,129,255,256] {
                for rows in [1,2,3,17,128] {
                    fast.cols=cols;fast.rows=rows;fast.wrap=wrap;fast.dx=1.0;fast.dy=1.0;
                    fast.prepare_topology();fast.area_count=0;
                    fast.word_keys.fill(SpaceKey::default());
                    for cell in 0..cols*rows {
                        let mask=if draw()%8==0 {1<<(draw()%3)} else {0};
                        fast.occupied[cell]=mask;fast.navigable[cell]=mask;
                        for id in 0..3 {if mask&(1<<id)!=0 {
                            fast.release[id*CELLS+cell]=(draw()%5) as f32*0.25;
                            fast.space_release[id*CELLS+cell]=(draw()%5) as f32*0.25;
                        }}
                    }
                    fast.invalidate_proofs();
                    fast.occupied_count=0;fast.navigable_count=0;
                    for cell in 0..cols*rows {
                        if fast.occupied[cell]!=0 {fast.occupied_cells[fast.occupied_count]=cell as u16;fast.occupied_count+=1;}
                        if fast.navigable[cell]!=0 {fast.navigable_cells[fast.navigable_count]=cell as u16;fast.navigable_count+=1;}
                    }
                    fast.prepare_words();
                    for future in [false,true] {
                        fast.future_active=future;fast.future_stamp=1;fast.future_words.fill(0);
                        for cell in 0..cols*rows {
                            fast.future[cell]=if future && draw()%16==0 {1} else {0};
                            if fast.future[cell]==1 {fast.future_words[(cell/cols)*4+(cell%cols)/64]|=1<<((cell%cols)%64);}
                        }
                        fast.area_count=0;reference.clone_from(&fast);
                        for time in [1.0,0.125,99.0,0.0,0.5,-1.0] {
                            for mask in [u16::MAX,3,1] {
                                for limit in [1,2,7,64,512] {
                                    // Repeated roots exercise component memoization and slot eviction.
                                    for _ in 0..40 {
                                        let root=draw() as usize%(cols*rows);let point=fast.center(root);
                                        assert_eq!(fast.space(point,mask,limit,time),reference.reference_space(point,mask,limit,time),
                                            "{cols}x{rows} wrap={wrap} future={future} time={time} mask={mask} limit={limit} root={root}");
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn dilation_owner_crossing_word_boundary_matches_ordered_bfs() {
        let mut grid=Spatial::new();
        grid.cols=65;grid.rows=1;grid.dx=1.0;grid.dy=1.0;grid.wrap=true;
        grid.prepare_topology();
        grid.occupied[64]=1;grid.release[64]=10.0;
        grid.occupied_cells[0]=64;grid.occupied_count=1;
        grid.navigable[63]=1;grid.space_release[63]=10.0;
        grid.navigable[64]=1;grid.space_release[64]=10.0;
        grid.navigable_cells[0]=63;grid.navigable_cells[1]=64;grid.navigable_count=2;
        grid.prepare_words();
        let mut reference=grid.clone();let root=grid.center(0);
        assert_eq!(grid.space(root,1,64,0.0),reference.reference_space(root,1,64,0.0));
        assert_eq!(grid.space(root,1,64,0.0),(63,false));
    }

    #[test]
    fn sparse_release_initialization_matches_full_table_clears() {
        use crate::{Config,controller::BaselineController};
        let mut world=World::new(Config {width:3440.0,height:1440.0,density:100.0,
            trails:100.0,self_collisions:true,..Config::default()}).unwrap();
        let mut sparse=Spatial::new();let mut cleared=Spatial::new();
        let mut dense_mask=vec![0u16;CELLS];let mut dense_release=vec![[0.0f32;MAX_SNAKES];CELLS];
        // Stale values must be ignored even after geometry/settings change.
        sparse.release.fill(1e20);
        sparse.space_release.fill(1e20);
        for tick in 0..200 {
            if tick==100 {world.reconfigure(Config {width:640.0,height:360.0,
                deadly_walls:false,..world.config()}).unwrap();}
            world.step(&mut BaselineController);
            cleared.heads.fill(-1);cleared.food_heads.fill(-1);cleared.occupied.fill(0);cleared.navigable.fill(0);cleared.weight.fill(0.0);
            cleared.release.fill(0.0);
            cleared.space_release.fill(0.0);
            sparse.rebuild(&world);cleared.rebuild(&world);
            // Independently reconstruct exact word metadata. Production's
            // minimum may be lower, but maxima and owner/cell masks are exact.
            cleared.prepare_words();
            assert_eq!(sparse.occupied_words,cleared.occupied_words);
            assert_eq!(sparse.navigable_words,cleared.navigable_words);
            assert_eq!(sparse.word_owners,cleared.word_owners);
            assert_eq!(sparse.word_max,cleared.word_max);
            for at in 0..sparse.rows*4 {for id in 0..MAX_SNAKES {
                assert!(sparse.word_min[at][id]<=cleared.word_min[at][id]);
            }}
            let n=sparse.cols*sparse.rows;
            dense_mask[..n].fill(0);
            for key in 0..n {
                let mask=cleared.occupied[key];if mask==0 {continue;}
                for cell in cleared.clearance_neighbours[key] {
                    if cell==u16::MAX {continue;}
                    let cell=cell as usize;let previous=dense_mask[cell];dense_mask[cell]|=mask;
                    let mut bits=mask;
                    while bits!=0 {
                        let id=bits.trailing_zeros() as usize;bits&=bits-1;
                        dense_release[cell][id]=if previous&(1<<id)==0 {cleared.release[id*CELLS+key]}
                            else {dense_release[cell][id].max(cleared.release[id*CELLS+key])};
                    }
                }
            }
            assert_eq!(dense_mask[..n],sparse.navigable[..n]);
            assert_eq!(sparse.heads[..n],cleared.heads[..n]);assert_eq!(sparse.food_heads[..n],cleared.food_heads[..n]);
            assert_eq!(sparse.weight[..n],cleared.weight[..n]);
            for cell in 0..n {
                if sparse.occupied[cell]!=0 {let max=(0..MAX_SNAKES).filter(|&id|sparse.occupied[cell]&(1<<id)!=0).map(|id|sparse.release[id*CELLS+cell]).fold(0.0_f32,f32::max);assert_eq!(sparse.occupied_max[cell],max);}
                if sparse.navigable[cell]!=0 {let max=(0..MAX_SNAKES).filter(|&id|sparse.navigable[cell]&(1<<id)!=0).map(|id|sparse.space_release[id*CELLS+cell]).fold(0.0_f32,f32::max);assert_eq!(sparse.navigable_max[cell],max);}
            }
            for cell in 0..n {for id in 0..MAX_SNAKES {if dense_mask[cell]&(1<<id)!=0 {
                assert_eq!(dense_release[cell][id],sparse.space_release[id*CELLS+cell]);
            }}}
            assert_eq!(sparse.occupied[..n],cleared.occupied[..n]);
            assert_eq!(sparse.navigable[..n],cleared.navigable[..n]);
            for key in 0..n {for id in 0..MAX_SNAKES {
                if sparse.occupied[key]&(1<<id)!=0 {
                    assert_eq!(sparse.release[id*CELLS+key],cleared.release[id*CELLS+key]);
                }
                if sparse.navigable[key]&(1<<id)!=0 {
                    assert_eq!(sparse.space_release[id*CELLS+key],cleared.space_release[id*CELLS+key]);
                }
            }}
            for key in (0..n).step_by(37) {
                let start=sparse.center(key);
                assert_eq!(sparse.space(start,u16::MAX,128,1.0),cleared.space(start,u16::MAX,128,1.0));
            }
        }
    }

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
                    grid.prepare_topology();
                    seen.fill(false);
                    for k in grid.clearance_neighbours[root] {
                        if k!=u16::MAX {assert!(!seen[k as usize]);seen[k as usize]=true;}
                    }
                    assert_eq!(seen,expected,"cached clearance wrap={wrap} {cols}x{rows}");
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
        crate::shape::prepare_short_tapers();
        Self { profile_enabled:false,
            profile:[0;9],
            counts:[0;11],
            word_keys:[SpaceKey::default();SPACE_SLOTS],
            word_epoch:0,
            word_next:0,
            word_seen:vec![0;SPACE_SLOTS*WORDS],
            word_open:vec![0;SPACE_SLOTS*WORDS],
            word_free_time:vec![0.0;SPACE_SLOTS*WORDS],
            future_words:[0;WORDS],
            occupied_words:vec![[0;MAX_SNAKES];WORDS],
            navigable_words:vec![[0;MAX_SNAKES];WORDS],
            word_min:vec![[f32::INFINITY;MAX_SNAKES];WORDS],
            word_max:vec![[0.0;MAX_SNAKES];WORDS],
            boundary_words:[0;WORDS],
            word_owners:[0;WORDS],
            occupied_cells:vec![0;CELLS],
            navigable_cells:vec![0;CELLS],
            food_cells:vec![0;CELLS],
            food_count:0,
            occupied_count:0,
            navigable_count:0,
            proof_epoch:0,
            proofs:vec![RoomProof::default();CELLS],
            word_proofs:vec![[WordProof::default();2];WORDS],
            proof_next:[0;WORDS],
            room_scratch:[Some(Box::new(RoomScratch::new())),Some(Box::new(RoomScratch::new()))],
            cell_word:vec![0;CELLS],
            cell_bit:vec![0;CELLS],
            heads: vec![-1; CELLS], next: vec![-1; MAX_SNAKES*MAX_SEGMENTS],
            widths: vec![1.0; MAX_SNAKES*MAX_SEGMENTS], lengths: [0; MAX_SNAKES],
            food_heads: vec![-1; CELLS], food_next: [-1; MAX_FOOD], weight: vec![0.0; CELLS],
            occupied: vec![0; CELLS],
            navigable:vec![0;CELLS],
            neighbours:vec![[u16::MAX;4];CELLS],
            clearance_neighbours:vec![[u16::MAX;9];CELLS],
            boundary:vec![false;CELLS],
            topology:(0,0,false),
            wall_band:0.0,
            boundary_geometry:(-1.0,-1.0,-1.0),
            occupied_max:vec![0.0;CELLS],
            navigable_max:vec![0.0;CELLS],
            release:vec![0.0;CELLS*MAX_SNAKES],
            space_release:vec![0.0;CELLS*MAX_SNAKES], visited: vec![0; CELLS], stamp: 0,
            area_seen: vec![0;CELLS],
            future:vec![0;CELLS],
            future_stamp:0,
            future_active:false,
            area_label:vec![0;CELLS],
            area_stamp:0,
            area_mask:0,
            area_count:0,
            area_limit:FILL_LIMIT,
            area_time:-1.0,
            area_result:[(0,false);32],
            queue: [0; FILL_LIMIT], parent: [0; FILL_LIMIT],
            depth:[0;FILL_LIMIT], cols: 1, rows: 1,
            dx: 1.0, dy: 1.0,
            max_motion:0.0, wrap: false }
    }
    // Arena topology is invariant across ticks. Preserve the original cardinal
    // order and small wrapped-axis deduplication, without dividing each BFS
    // node by the grid width or allocating any search storage.
    fn prepare_topology(&mut self) {
        if self.topology == (self.cols,self.rows,self.wrap) && self.boundary_geometry == (self.dx,self.dy,self.wall_band) { return; }
        self.invalidate_proofs();
        self.word_keys.fill(SpaceKey::default());
        self.word_next=0;
        self.boundary_geometry=(self.dx,self.dy,self.wall_band);
        self.topology = (self.cols,self.rows,self.wrap);
        self.boundary_words.fill(0);
        for key in 0..self.cols*self.rows {
            let x=key%self.cols;let y=key/self.cols;
            self.cell_word[key]=(y*4+x/64) as u16;
            self.cell_bit[key]=(x%64) as u8;
            let mut neighbours = [u16::MAX;4];
            for (index,(x,y)) in self.neighbour_offsets().enumerate() {
                neighbours[index] = self.offset(key,x,y).map_or(u16::MAX,|k|k as u16);
            }
            self.neighbours[key] = neighbours;
            let mut clearance=[u16::MAX;9];let mut count=0;
            let (xs,ys)=self.local_spans(1);
            for y in ys {for x in xs.clone() {
                if let Some(k)=self.offset(key,x,y) {clearance[count]=k as u16;count+=1;}
            }}
            self.clearance_neighbours[key]=clearance;
            let p=self.center(key);
            // Keep the outer ring blocked even when floating-point cell edges
            // round inside the arena. Both cell and word queries share this map.
            self.boundary[key] = !self.wrap && (x==0 || x+1==self.cols || y==0 || y+1==self.rows
                || p.x-self.dx*0.5<=self.wall_band || p.x+self.dx*0.5>=self.dx*self.cols as f64-self.wall_band
                || p.y-self.dy*0.5<=self.wall_band || p.y+self.dy*0.5>=self.dy*self.rows as f64-self.wall_band);
        }
        for cell in 0..self.cols*self.rows {if self.boundary[cell] {self.boundary_words[(cell/self.cols)*4+(cell%self.cols)/64]|=1<<((cell%self.cols)%64);}}
        self.area_count=0;
    }
    #[cfg(test)]
    pub fn rebuild(&mut self, w: &World) {self.rebuild_impl(w,None);}
    pub fn rebuild_with_rivals(&mut self,w:&World,rivals:&[super::Rival;MAX_SNAKES]) {self.rebuild_impl(w,Some(rivals));}
    fn rebuild_impl(&mut self, w: &World,rivals:Option<&[super::Rival;MAX_SNAKES]>) {
        let clock=self.profile_enabled.then(std::time::Instant::now);
        self.invalidate_proofs();
        self.word_keys.fill(SpaceKey::default());
        self.word_next=0;
        self.area_mask=0;
        let c = w.config();
        let modern=c.rules==crate::RuleSet::V2;
        let side = (c.base_radius()*if modern {2.5} else {5.0}).max(28.0);
        self.wall_band=if modern {c.base_radius()*4.5} else {0.0};
        self.cols = (c.width/side).ceil().clamp(1.0,if modern {256.0} else {128.0}) as usize;
        self.rows = (c.height/side).ceil().clamp(1.0,128.0) as usize;
        self.dx = c.width/self.cols as f64; self.dy = c.height/self.rows as f64;
        self.wrap = !c.deadly_walls;
        self.prepare_topology();
        // Clear the previous numeric keys, even on a resize. Untouched cells
        // remain empty; release slots are still guarded by occupancy bits.
        for &cell in &self.occupied_cells[..self.occupied_count] {
            let cell=cell as usize;self.heads[cell]=-1;self.occupied[cell]=0;
        }
        for &cell in &self.navigable_cells[..self.navigable_count] {self.navigable[cell as usize]=0;}
        for &cell in &self.food_cells[..self.food_count] {
            let cell=cell as usize;self.food_heads[cell]=-1;self.weight[cell]=0.0;
        }
        self.occupied_count=0;self.navigable_count=0;self.food_count=0;
        self.clear_words();
        // Release slots are valid only under their occupancy bit. Initialize
        // on first insertion instead of clearing two large sparse tables each
        // tick; every read below checks the corresponding mask first.
        let mut motion_squared=0.0_f64;
        for s in w.snakes().filter(|s|s.alive) {
            let id=s.id as usize;
            let len=s.segments.len();
            let cache_len=if len>1600 {len.div_ceil(64)*64} else {len};
            let cache_len=cache_len.min(MAX_SEGMENTS);
            if w.config().rules==crate::RuleSet::V2 && self.lengths[id]!=cache_len {
                self.lengths[id]=cache_len;
                if len<=crate::shape::SHORT_TAPER_MAX {
                    self.widths[id*MAX_SEGMENTS..id*MAX_SEGMENTS+len]
                        .copy_from_slice(crate::shape::short_tapers(len).0);
                } else {
                    for j in 0..cache_len {
                        self.widths[id*MAX_SEGMENTS+j]=if len>1600 {
                            crate::world::taper::span_radius(1.0,j as f64/(cache_len-1) as f64,
                                j as f64/cache_len.saturating_sub(64).max(1600).saturating_sub(1) as f64)
                        } else {crate::world::taper::body_radius(1.0,j as f64,len)};
                    }
                }
            }
            let (rate,growth_delay)=if let Some(rivals)=rivals {(rivals[id].release_rate,rivals[id].growth_delay)} else {
                let speed=w.motion_limits(id,0.0).unwrap().0;
                (s.radius*1.18/(speed*0.65).max(1.0),w.tail_growth_delay(id).unwrap())
            };
            for (j, seg) in s.segments.iter().enumerate().skip(1) {
                motion_squared=motion_squared.max(w.distance_squared(seg.previous,seg.current));
                let key = self.key(w.canonical_point(seg.current));
                let encoded = s.id as usize*MAX_SEGMENTS+j;
                self.next[encoded] = self.heads[key]; self.heads[key] = encoded as i32;
                // Own neck exclusion is dealt with separately in exact safety.
                // Keep the neck in the occupancy mask for OTHER snakes.
                let bit=1 << s.id;
                if self.occupied[key]==0 {self.occupied_cells[self.occupied_count]=key as u16;self.occupied_count+=1;}
                let first=self.occupied[key]&bit==0;
                self.occupied[key] |= bit;
                let release=((s.segments.len()-j) as f64*rate+growth_delay+0.15) as f32;
                self.release[id*CELLS+key]=if first {release} else {self.release[id*CELLS+key].max(release)};
                self.occupied_max[key]=if self.occupied[key]==bit {self.release[id*CELLS+key]}
                    else {self.occupied_max[key].max(release)};
                if first {
                    let at=self.cell_word[key] as usize;
                    self.word_owners[at]|=bit;
                    self.occupied_words[at][id]|=1<<self.cell_bit[key];
                }
            }
        }
        self.max_motion=motion_squared.sqrt();
        let inserted=clock.map(|c|c.elapsed().as_nanos());
        if let Some(t)=inserted {self.profile[0]+=t;self.counts[0]+=1;}
        // Tapered contacts fit inside this conservative whole-cell turning-room
        // bound. Exact continuous rollouts use the shared radius profile.
        // A one-cell clearance band keeps coarse connectivity from treating a
        // head-width crack as room for a minimum-radius turn. Tail release is
        // conservative, and the continuous rollout still proves motion safety.
        for index in 0..self.occupied_count {
            let key=self.occupied_cells[index] as usize;
            let mut owners=self.occupied[key];
            // An owner's release is identical at every neighbour. Traverse
            // owners once per source cell, keeping each owner's source-cell
            // and neighbour order unchanged, instead of reloading its release
            // and enumerating its bit for every neighbour.
            while owners!=0 {
                let id=owners.trailing_zeros() as usize;owners&=owners-1;
                let bit=1<<id;
                let release=self.release[id*CELLS+key];
                for k in self.clearance_neighbours[key] {
                    if k==u16::MAX {continue;}
                    let k=k as usize;
                    let previous=self.navigable[k];
                    if previous==0 {self.navigable_cells[self.navigable_count]=k as u16;self.navigable_count+=1;self.navigable_max[k]=0.0;}
                    self.navigable[k]|=bit;
                    let first=previous&bit==0;
                    if first || release>self.space_release[id*CELLS+k] {
                        self.space_release[id*CELLS+k]=release;
                        let at=self.cell_word[k] as usize;
                        if first {
                            self.navigable_words[at][id]|=1<<self.cell_bit[k];
                            self.word_owners[at]|=bit;
                            // Contributions only increase this cell's
                            // release. Keeping its initial value in the
                            // word minimum is a conservative lower bound.
                            self.word_min[at][id]=self.word_min[at][id].min(release);
                        }
                        self.word_max[at][id]=self.word_max[at][id].max(release);
                        self.navigable_max[k]=self.navigable_max[k].max(release);
                    }
                }
            }
        }
        let dilated=clock.map(|c|c.elapsed().as_nanos());
        if let Some(t)=dilated {self.profile[1]+=t-inserted.unwrap();}
        for (i,f) in w.foods().enumerate() {
            if f.kind==crate::FoodKind::Meteor || w.food[i].captured_by!=0 {continue;}
            let key = self.key(f.position);
            if self.food_heads[key]==-1 {self.food_cells[self.food_count]=key as u16;self.food_count+=1;}
            self.food_next[i] = self.food_heads[key]; self.food_heads[key] = i as i32;
            self.weight[key] += f.value;
        }
    }
    pub fn key(&self,p: Point) -> usize {
        // Truncation equals floor for nonnegative cells. Negative cells both
        // clamp to zero; saturating casts also retain NaN/infinity behavior.
        let x = (p.x/self.dx) as isize;
        let x = x.clamp(0,self.cols as isize-1) as usize;
        let y = (p.y/self.dy) as isize;
        let y = y.clamp(0,self.rows as isize-1) as usize;
        y*self.cols+x
    }
    #[inline(always)]
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
        let mut weight=0.0;
        for cell in self.clearance_neighbours[key] {
            if cell!=u16::MAX {weight+=self.weight[cell as usize];}
        }
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
        let clock=self.profile_enabled.then(std::time::Instant::now);
        let result=self.space_impl(start,mask,limit,time);
        if let Some(c)=clock {self.profile[2]+=c.elapsed().as_nanos();self.counts[2]+=1;}
        result
    }
    fn space_impl(&mut self,start:Point,mask:u16,limit:usize,time:f64)->(usize,bool) {
        self.prepare_topology();
        let limit=limit.clamp(1,FILL_LIMIT);
        let root=self.key(start);
        if self.area_count==0 || self.area_mask!=mask || self.area_limit!=limit || self.area_time!=time || self.area_count==self.area_result.len() {
            self.area_mask=mask;self.area_limit=limit;self.area_time=time;self.area_count=0;
            self.area_stamp=self.area_stamp.wrapping_add(1);
            if self.area_stamp==0 {self.area_seen.fill(0);self.area_stamp=1;}
        }
        if self.area_seen[root]==self.area_stamp {return self.area_result[self.area_label[root]];}
        let proof=if !self.future_active && time>=0.0 {
            self.ordinary_proof(root,mask,limit,time).unwrap_or(self.proofs[root])
        } else {RoomProof::default()};
        if !self.future_active && time>=0.0 && proof.epoch==self.proof_epoch
            && proof.time<=time && proof.mask&mask==mask
            && (proof.count&!NARROW_PROOF) as usize>limit
            && (proof.count&NARROW_PROOF==0 || self.blocked(root,mask,time)) {
            if self.profile_enabled {self.counts[8]+=1;}
            return (limit,true);
        }
        let root_blocked=self.blocked(root,mask,time);
        if self.profile_enabled && root_blocked {self.counts[9]+=1;}
        // Lease preallocated buffers. A narrow query can perform one ordinary
        // component query; those two levels use distinct buffers.
        let index=usize::from(!root_blocked);
        let mut scratch=self.room_scratch[index].take().expect("bounded room-query nesting");
        let result=self.space_search(root,root_blocked,mask,limit,time,&mut scratch);
        self.room_scratch[index]=Some(scratch);result
    }
    fn space_search(&mut self,root:usize,root_blocked:bool,mask:u16,limit:usize,time:f64,scratch:&mut RoomScratch)->(usize,bool) {
        let slot=self.space_slot(mask,time);
        let RoomScratch {reached,seeds,dirty,queued,pending,touched}=scratch;
        let mut touched_count=0;
        // Capped queries usually touch only a few words. Clear every word
        // written by the preceding query, including pending seeds on an early
        // return and words outside a resized arena, without streaming the
        // entire arena's scratch through cache for each candidate.
        for (block,mask) in dirty.iter_mut().enumerate() {
            let mut bits=*mask;*mask=0;
            while bits!=0 {
                let index=block*64+bits.trailing_zeros() as usize;bits&=bits-1;
                reached[index]=0;seeds[index]=0;
            }
        }
        queued.fill(0);
        let mut read=0;let mut write=0;let mut pending_count=0;let mut count=0;let mut proof_time=0.0_f64;
        let words=self.cols.div_ceil(64);
        // Preserve the original two-hop exception exactly. Once these cells
        // are admitted, every further step must be in ordinary turning space.
        let mut initial=1;
        self.begin_search(root);
        if time>=0.0 && root_blocked {
            let mut at=0;
            while at<initial && self.depth[at]<2 {
                let cell=self.queue[at];let depth=self.depth[at];at+=1;
                for neighbour in self.neighbours[cell] {
                    if neighbour==u16::MAX {continue;}
                    let k=neighbour as usize;
                    if self.visited[k]==self.stamp {continue;}
                    self.visited[k]=self.stamp;
                    if !self.blocked(k,mask,time) || !self.physical_blocked(k,mask,time) {
                        let mut physical_time=0.0_f64;let mut space_time=if self.boundary[k] {f64::INFINITY} else {0.0};
                        let mut owners=self.occupied[k]&mask;
                        if owners==self.occupied[k] && owners!=0 {physical_time=self.occupied_max[k] as f64;}
                        else {while owners!=0 {let id=owners.trailing_zeros() as usize;owners&=owners-1;physical_time=physical_time.max(self.release[id*CELLS+k] as f64);}}
                        let mut owners=self.navigable[k]&mask;
                        if owners==self.navigable[k] && owners!=0 {space_time=space_time.max(self.navigable_max[k] as f64);}
                        else {while owners!=0 {let id=owners.trailing_zeros() as usize;owners&=owners-1;space_time=space_time.max(self.space_release[id*CELLS+k] as f64);}}
                        proof_time=proof_time.max(physical_time.min(space_time));
                        self.queue[initial]=k;self.depth[initial]=depth+1;initial+=1;
                    }
                }
            }
        }
        if root_blocked && time>=0.0 {
            // Most narrow starts leave into one large ordinary component.
            // Measure it once and share its capped connected-set bound with
            // every other entry cell. A blocked root adds at least one cell
            // outside that component, so a component of exactly limit cells
            // also proves the original search must be capped.
            let mut initial_cells=[0usize;13];
            initial_cells[..initial].copy_from_slice(&self.queue[..initial]);
            let mut source=None;
            'source: for &cell in &initial_cells[..initial] {for neighbour in self.neighbours[cell] {
                if neighbour!=u16::MAX && !self.blocked(neighbour as usize,mask,time) {
                    source=Some(neighbour as usize);break 'source;
                }
            }}
            if let Some(source)=source {
                let result=self.space_impl(self.center(source),mask,limit,time);
                if result.0>=limit {
                    if !self.future_active {
                        let proof=self.ordinary_proof(source,mask,limit,time).unwrap_or_default();
                        let threshold=if proof.epoch==self.proof_epoch && proof.count&NARROW_PROOF==0
                            && proof.mask&mask==mask && proof.count as usize>limit {
                            proof.time
                        } else {time};
                        self.proofs[root]=RoomProof {epoch:self.proof_epoch,time:proof_time.max(threshold),mask,count:(limit+1) as u16|NARROW_PROOF};
                    }
                    return (limit,true);
                }
                self.queue[..initial].copy_from_slice(&initial_cells[..initial]);
            }
        }
        if root_blocked {
            for &cell in &self.queue[..initial] {
                let index=self.cell_word[cell] as usize;
                dirty[index/64]|=1<<(index%64);
                reached[index]|=1<<self.cell_bit[cell];
            }
            count=initial;
            if count>limit {return self.finish_words(reached,&touched[..touched_count],root,true,limit,true,proof_time);}
        }
        {
            let mut add=|cell:usize| {
                let index=self.cell_word[cell] as usize;let bit=1<<self.cell_bit[cell];
                if bit&reached[index]!=0 {return;}
                dirty[index/64]|=1<<(index%64);
                seeds[index]|=bit;
                if queued[index/64]&(1<<(index%64))==0 {
                    queued[index/64]|=1<<(index%64);pending[write]=index;write=(write+1)%WORDS;pending_count+=1;
                }
            };
            if root_blocked {
                for &cell in &self.queue[..initial] {for neighbour in self.neighbours[cell] {
                    if neighbour!=u16::MAX {add(neighbour as usize);}
                }}
            } else {add(root);}
        }
        while pending_count!=0 {
            let index=pending[read];read=(read+1)%WORDS;pending_count-=1;
            queued[index/64]&=!(1<<(index%64));
            let x=index%4;let y=index/4;
            let open=self.open_word(slot,y,x);
            proof_time=proof_time.max(self.word_free_time[slot*WORDS+index]);
            let open=open&if self.future_active {!self.future_words[index]} else {u64::MAX};
            let mut spread=seeds[index]&open;seeds[index]=0;
            // Saturate every seeded open horizontal run in two logarithmic passes,
            // rather than revisiting the same word on each individual hop.
            if spread!=0 {
                // Empty arena words usually contain one open run (including
                // clipped edge words). A seed anywhere in that run reaches it
                // all; preserve the general logarithmic flood for holes.
                let first=open & open.wrapping_neg();
                if open & open.wrapping_add(first)==0 {spread=open;}
                else {
                    let mut links=open;
                    for shift in [1,2,4,8,16,32] {spread|=(spread<<shift)&links;links&=links<<shift;}
                    let mut links=open;
                    for shift in [1,2,4,8,16,32] {spread|=(spread>>shift)&links;links&=links>>shift;}
                }
            }
            let added=spread&!reached[index];
            if added==0 {continue;}
            if reached[index]==0 {touched[touched_count]=index;touched_count+=1;}
            reached[index]|=added;count+=added.count_ones() as usize;
            if count>limit {return self.finish_words(reached,&touched[..touched_count],root,root_blocked,limit,true,proof_time);}
            if !root_blocked && !self.future_active && time>=0.0 {
                for hint in self.word_proofs[index] {
                    let proof=hint.proof;
                    if spread&hint.cells!=0 && proof.epoch==self.proof_epoch
                        && proof.mask&mask==mask && proof.time<=time && proof.count as usize>limit {
                        // Each member belongs to a component with this bound.
                        // Reaching any member certifies the connected prefix.
                        proof_time=proof_time.max(proof.time);
                        return self.finish_words(reached,&touched[..touched_count],root,false,limit,true,proof_time);
                    }
                }
            }
            let mut add=|target:usize,bits:u64| {
                let bits=bits&!reached[target];
                if bits==0 {return;}
                dirty[target/64]|=1<<(target%64);
                seeds[target]|=bits;
                if queued[target/64]&(1<<(target%64))==0 {
                    queued[target/64]|=1<<(target%64);pending[write]=target;write=(write+1)%WORDS;pending_count+=1;
                }
            };
            if x>0 {add(index-1,added<<63);}
            if x+1<words {add(index+1,added>>63);}
            if self.wrap {
                let last=(self.cols-1)%64;
                if x==0 {add(y*4+words-1,(added&1)<<last);}
                if x+1==words {add(y*4,(added>>last)&1);}
            }
            if y>0 {add(index-4,added);} else if self.wrap {add((self.rows-1)*4+x,added);}
            if y+1<self.rows {add(index+4,added);} else if self.wrap {add(x,added);}
        }
        self.finish_words(reached,&touched[..touched_count],root,root_blocked,count,false,proof_time)
    }
    fn invalidate_proofs(&mut self) {
        self.proof_next.fill(0);
        self.proof_epoch=self.proof_epoch.wrapping_add(1);
        if self.proof_epoch==0 {self.proofs.fill(RoomProof::default());self.word_proofs.fill([WordProof::default();2]);self.proof_epoch=1;}
    }
    #[inline]
    fn ordinary_proof(&self,cell:usize,mask:u16,limit:usize,time:f64)->Option<RoomProof> {
        let bit=1<<self.cell_bit[cell];
        self.word_proofs[self.cell_word[cell] as usize].iter().find_map(|entry| {
            let proof=entry.proof;
            (entry.cells&bit!=0 && proof.epoch==self.proof_epoch && proof.mask&mask==mask
                && proof.time<=time && proof.count as usize>limit).then_some(proof)
        })
    }
    fn publish_proof(&mut self,index:usize,cells:u64,proof:RoomProof) {
        for entry in &mut self.word_proofs[index] {
            let old=entry.proof;
            if old.epoch==proof.epoch && old.mask==proof.mask && old.time==proof.time {
                if cells&!entry.cells==0 && old.count>=proof.count {return;}
                entry.cells|=cells;entry.proof.count=old.count.min(proof.count);return;
            }
        }
        let slot=self.word_proofs[index].iter().position(|entry|entry.proof.epoch!=self.proof_epoch)
            .unwrap_or(self.proof_next[index] as usize);
        self.word_proofs[index][slot]=WordProof {cells,proof};
        self.proof_next[index]=(1-slot) as u8;
    }
    fn space_slot(&mut self,mask:u16,time:f64)->usize {
        if let Some(slot)=self.word_keys.iter().position(|k|k.epoch!=0 && k.mask==mask && k.time==time) {return slot;}
        let slot=self.word_next;self.word_next=(slot+1)%SPACE_SLOTS;
        self.word_epoch=self.word_epoch.wrapping_add(1);
        if self.word_epoch==0 {self.word_seen.fill(0);self.word_keys.fill(SpaceKey::default());self.word_epoch=1;}
        self.word_keys[slot]=SpaceKey {mask,time,epoch:self.word_epoch};slot
    }
    fn clear_words(&mut self) {
        let clock=self.profile_enabled.then(std::time::Instant::now);
        self.word_owners.fill(0);
        self.occupied_words[..self.rows*4].fill([0;MAX_SNAKES]);
        self.navigable_words[..self.rows*4].fill([0;MAX_SNAKES]);
        self.word_min[..self.rows*4].fill([f32::INFINITY;MAX_SNAKES]);
        self.word_max[..self.rows*4].fill([0.0;MAX_SNAKES]);
        if let Some(c)=clock {self.profile[8]+=c.elapsed().as_nanos();}
    }
    #[cfg(test)]
    fn prepare_words(&mut self) {
        self.clear_words();
        for index in 0..self.occupied_count {
            let cell=self.occupied_cells[index] as usize;
            let at=self.cell_word[cell] as usize;let bit=1<<self.cell_bit[cell];
            let mut owners=self.occupied[cell];self.word_owners[at]|=owners;
            let mut maximum=0.0_f32;
            while owners!=0 {let id=owners.trailing_zeros() as usize;owners&=owners-1;self.occupied_words[at][id]|=bit;maximum=maximum.max(self.release[id*CELLS+cell]);}
            self.occupied_max[cell]=maximum;
        }
        for index in 0..self.navigable_count {
            let cell=self.navigable_cells[index] as usize;
            let at=self.cell_word[cell] as usize;let bit=1<<self.cell_bit[cell];
            let mut owners=self.navigable[cell];self.word_owners[at]|=owners;
            let mut maximum=0.0_f32;
            while owners!=0 {
                let id=owners.trailing_zeros() as usize;owners&=owners-1;self.navigable_words[at][id]|=bit;
                let release=self.space_release[id*CELLS+cell];maximum=maximum.max(release);
                self.word_min[at][id]=self.word_min[at][id].min(release);
                self.word_max[at][id]=self.word_max[at][id].max(release);
            }
            self.navigable_max[cell]=maximum;
        }
    }
    #[inline]
    fn open_word(&mut self,slot:usize,row:usize,word:usize)->u64 {
        let index=row*4+word;let at=slot*WORDS+index;let key=self.word_keys[slot];
        if self.word_seen[at]!=key.epoch {
            let clock=self.profile_enabled.then(std::time::Instant::now);
            if self.profile_enabled {self.counts[1]+=1;}
            let valid=if word*64+64<=self.cols {u64::MAX} else {(1<<(self.cols%64))-1};
            let mut blocked=if key.time<0.0 {0} else {self.boundary_words[index]};
            let mut free_time=0.0_f64;
            let mut owners=key.mask&self.word_owners[index];
            while owners!=0 {
                let id=owners.trailing_zeros() as usize;owners&=owners-1;
                if key.time<0.0 {blocked|=self.occupied_words[index][id];continue;}
                let mut bits=self.navigable_words[index][id];
                if (self.word_min[index][id] as f64)>key.time {blocked|=bits;continue;}
                if (self.word_max[index][id] as f64)<=key.time {free_time=free_time.max(self.word_max[index][id] as f64);continue;}
                bits&=!blocked;
                while bits!=0 {
                    let bit=bits.trailing_zeros() as usize;bits&=bits-1;
                    let release=self.space_release[(id)*CELLS+(row*self.cols+word*64+bit)] as f64;
                    if release>key.time {blocked|=1<<bit;} else {free_time=free_time.max(release);}
                }
            }
            self.word_free_time[at]=free_time;
            self.word_open[at]=valid&!blocked;self.word_seen[at]=key.epoch;
            if let Some(c)=clock {self.profile[7]+=c.elapsed().as_nanos();}
        }
        self.word_open[at]
    }
    fn finish_words(&mut self,reached:&[u64;WORDS],touched:&[usize],root:usize,root_blocked:bool,count:usize,capped:bool,proof_time:f64)->(usize,bool) {
        let clock=self.profile_enabled.then(std::time::Instant::now);
        if self.profile_enabled && capped {self.counts[10]+=1;}
        // A nested ordinary-component search can consume the last label.
        if self.area_count==self.area_result.len() {
            self.area_count=0;self.area_stamp=self.area_stamp.wrapping_add(1);
            if self.area_stamp==0 {self.area_seen.fill(0);self.area_stamp=1;}
        }
        let label=self.area_count;self.area_count+=1;self.area_result[label]=(count,capped);
        if root_blocked {
            self.area_seen[root]=self.area_stamp;self.area_label[root]=label;
            if capped && !self.future_active && self.area_time>=0.0 {
                self.proofs[root]=RoomProof {epoch:self.proof_epoch,time:proof_time,mask:self.area_mask,count:(count+1) as u16|NARROW_PROOF};
            }
        } else {
            for &index in touched {
                let row=index/4;let word=index%4;
                let mut bits=reached[index];
                if capped && !self.future_active && self.area_time>=0.0 {
                    self.publish_proof(index,bits,RoomProof {epoch:self.proof_epoch,time:proof_time,
                        mask:self.area_mask,count:(count+1) as u16});
                    // The word proof already stores this capped result for
                    // every member. Exact/future/physical-only queries retain
                    // the cell-label cache below.
                    continue;
                }
                while bits!=0 {
                    let bit=bits.trailing_zeros() as usize;bits&=bits-1;let cell=row*self.cols+word*64+bit;
                    self.area_seen[cell]=self.area_stamp;self.area_label[cell]=label;
                }
            }
        }
        if let Some(c)=clock {self.profile[3]+=c.elapsed().as_nanos();self.counts[3]+=count as u64;}
        (count,capped)
    }
    #[cfg(test)]
    fn reference_space(&mut self,start:Point,mask:u16,limit:usize,time:f64)->(usize,bool) {
        self.prepare_topology();
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
            for cell in self.neighbours[key] {
                if cell!=u16::MAX {
                    let k=cell as usize;
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
        let clock=self.profile_enabled.then(std::time::Instant::now);
        self.future_stamp=self.future_stamp.wrapping_add(1);
        if self.future_stamp==0 {self.future.fill(0);self.future_stamp=1;}
        self.future_words.fill(0);
        let end=path.len().saturating_sub(neck_ticks+1);
        let begin=path.len().saturating_sub(body_ticks+1);
        if end>begin {
            for &p in path[begin..end].iter().step_by(2) {
                let key=self.key(p);
                for y in -1..=1 {for x in -1..=1 {
                    if let Some(k)=self.offset(key,x,y) {self.future[k]=self.future_stamp;self.future_words[self.cell_word[k] as usize]|=1<<self.cell_bit[k];}
                }}
            }
        }
        if let Some(c)=clock {self.profile[4]+=c.elapsed().as_nanos();self.counts[4]+=1;}
        self.future_active=true;self.area_count=0;
        let result=self.space(start,mask,limit,time);
        self.future_active=false;self.area_count=0;
        result
    }
    #[inline(always)]
    pub(super) fn physical_blocked(&self,key:usize,mask:u16,time:f64)->bool {
        if self.future_active && self.future[key]==self.future_stamp {return true;}
        let mut bits=self.occupied[key]&mask;
        if bits==0 {return false;}
        if bits==self.occupied[key] {return self.occupied_max[key] as f64>time;}
        while bits!=0 {
            let id=bits.trailing_zeros() as usize;bits&=bits-1;
            if self.release[id*CELLS+key] as f64>time {return true;}
        }
        false
    }
    #[inline(always)]
    fn blocked(&self,key:usize,mask:u16,time:f64)->bool {
        if self.future_active && self.future[key]==self.future_stamp {return true;}
        if time<0.0 {return self.occupied[key]&mask!=0;}
        if self.boundary[key] {return true;}
        let mut bits=self.navigable[key]&mask;
        if bits==0 {return false;}
        if bits==self.navigable[key] {return self.navigable_max[key] as f64>time;}
        while bits!=0 {
            let id=bits.trailing_zeros() as usize;bits&=bits-1;
            if self.space_release[id*CELLS+key] as f64>time {return true;}
        }
        false
    }
    #[cfg(test)]
    fn finish_area(&mut self,write:usize,capped:bool)->(usize,bool) {
        let clock=self.profile_enabled.then(std::time::Instant::now);
        let label=self.area_count;self.area_count+=1;
        self.area_result[label]=(write,capped);
        let root=self.queue[0];
        let narrow_start=self.blocked(root,self.area_mask,self.area_time);
        for &key in &self.queue[..write] {
            if key==root || (!narrow_start && !self.blocked(key,self.area_mask,self.area_time)) {
                self.area_seen[key]=self.area_stamp;self.area_label[key]=label;
            }
        }
        if let Some(c)=clock {self.profile[3]+=c.elapsed().as_nanos();self.counts[3]+=write as u64;}
        (write,capped)
    }
    /// Route into the tail's wake; if the current tail is blocked or beyond
    /// the work budget, choose a roomy reachable cell. This guides continuous
    /// rollouts and never certifies a passage or future release.
    pub fn escape_waypoint(&mut self,start:Point,goal:Point,mask:u16)->Option<Point> {
        let clock=self.profile_enabled.then(std::time::Instant::now);
        let result=self.escape_waypoint_impl(start,goal,mask);
        if let Some(c)=clock {self.profile[5]+=c.elapsed().as_nanos();self.counts[5]+=1;}
        result
    }
    fn escape_waypoint_impl(&mut self,start:Point,goal:Point,mask:u16)->Option<Point> {
        self.prepare_topology();
        let root=self.key(start);let target=self.key(goal);
        self.begin_search(root);
        let mut read=0;let mut write=1;let mut best=0;let mut best_room=0;
        while read<write {
            let key=self.queue[read];
            let room=self.neighbours[key].iter().filter(|&&k|k!=u16::MAX && !self.boundary[k as usize] && self.occupied[k as usize]&mask==0).count();
            if read!=0 && (room>best_room || (room==best_room &&
                ((self.center(key).x-goal.x).powi(2)+(self.center(key).y-goal.y).powi(2)) <
                ((self.center(self.queue[best]).x-goal.x).powi(2)+(self.center(self.queue[best]).y-goal.y).powi(2)))) {
                best=read;best_room=room;
            }
            if key==target {best=read;break;}
            for cell in self.neighbours[key] {
                let k=cell as usize;
                if cell!=u16::MAX && self.visited[k]!=self.stamp && !self.boundary[k] && self.occupied[k]&mask==0 {
                    if write==FILL_LIMIT {continue;}
                    self.visited[k]=self.stamp;self.queue[write]=k;self.parent[write]=read;write+=1;
                }
            }
            read+=1;
        }
        if best==0 {return None;}
        let mut at=best;let mut previous=best;
        while self.parent[at]!=0 {previous=at;at=self.parent[at];}
        Some(self.center(self.queue[previous]))
    }
    /// Bounded coarse BFS; the returned waypoint is guidance, never a safety
    /// certificate. Exact continuous rollouts validate every published move.
    pub fn waypoint(&mut self,start:Point,goal:Point,mask:u16) -> Option<Point> {
        let clock=self.profile_enabled.then(std::time::Instant::now);
        let result=self.waypoint_impl(start,goal,mask);
        if let Some(c)=clock {self.profile[6]+=c.elapsed().as_nanos();self.counts[6]+=1;}
        result
    }
    fn waypoint_impl(&mut self,start:Point,goal:Point,mask:u16) -> Option<Point> {
        self.prepare_topology();
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
            for cell in self.neighbours[key] {
                if cell!=u16::MAX {
                    let k=cell as usize;
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
