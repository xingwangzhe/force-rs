#![deny(clippy::all)]

use napi::bindgen_prelude::{Float64Array, Uint32Array};
#[cfg(feature = "pgo")]
use napi::{Error, Result, Status};
use napi_derive::napi;
use rayon::prelude::*;
use std::time::Instant;

type F = f64;
const D: usize = 3;
const MIN_CELL: F = 1.0;
const DISTANCE_MIN2: F = 1.0;

#[napi(object)]
pub struct ForceOptions {
    pub repulsion: f64,
    pub link_distance: f64,
    pub center_strength: f64,
    pub theta: f64,
    pub velocity_decay: f64,
    pub alpha_decay: f64,
    pub algorithm: Option<String>,
    pub distance_max: Option<f64>,
}

#[derive(Clone)]
struct LinkGraph {
    adj: Vec<u32>,
    offsets: Vec<u32>,
    strengths: Vec<F>,
    biases: Vec<F>,
}

impl LinkGraph {
    fn new(links: &[u32], n: usize) -> Self {
        let mut deg = vec![0u32; n];
        for pair in links.as_chunks::<2>().0 {
            let s = pair[0] as usize;
            let t = pair[1] as usize;
            if s < n && t < n {
                deg[s] += 1;
                deg[t] += 1;
            }
        }
        let mut offsets = vec![0u32; n + 1];
        for i in 0..n {
            offsets[i + 1] = offsets[i] + deg[i];
        }
        let mut adj = vec![0u32; offsets[n] as usize];
        let mut cursor = offsets[..n].to_vec();
        for pair in links.as_chunks::<2>().0 {
            let s = pair[0] as usize;
            let t = pair[1] as usize;
            if s < n && t < n {
                adj[cursor[s] as usize] = t as u32;
                cursor[s] += 1;
                adj[cursor[t] as usize] = s as u32;
                cursor[t] += 1;
            }
        }
        // Both coefficients depend only on graph topology, not simulation state.
        let mut strengths = vec![0.0; adj.len()];
        let mut biases = vec![0.0; adj.len()];
        for node in 0..n {
            let my_degree = deg[node].max(1) as F;
            for edge in offsets[node] as usize..offsets[node + 1] as usize {
                let other_degree = deg[adj[edge] as usize].max(1) as F;
                strengths[edge] = 1.0 / my_degree.min(other_degree).max(1.0);
                biases[edge] = other_degree / (my_degree + other_degree);
            }
        }
        Self {
            adj,
            offsets,
            strengths,
            biases,
        }
    }
}

#[derive(Clone)]
struct LinearNode {
    c: [F; D],
    hw: F,
    mass: F,
    com: [F; D],
    children: [i32; 8],
    particle: i32,
    child_count: u8,
}

impl LinearNode {
    fn new(c: [F; D], hw: F) -> Self {
        Self {
            c,
            hw,
            mass: 0.0,
            com: [0.0; D],
            children: [-1; 8],
            particle: -1,
            child_count: 0,
        }
    }

    #[inline]
    fn branched(&self) -> bool {
        self.child_count != 0
    }

    #[inline]
    fn oct(p: &[F; D], ctr: &[F; D]) -> usize {
        ((p[0] >= ctr[0]) as usize)
            | (((p[1] >= ctr[1]) as usize) << 1)
            | (((p[2] >= ctr[2]) as usize) << 2)
    }

    #[inline]
    fn child_center(&self, oct: usize, hw: F) -> [F; D] {
        [
            self.c[0] + hw * (if oct & 1 != 0 { 1.0 } else { -1.0 }),
            self.c[1] + hw * (if oct & 2 != 0 { 1.0 } else { -1.0 }),
            self.c[2] + hw * (if oct & 4 != 0 { 1.0 } else { -1.0 }),
        ]
    }
}

struct LinearTree {
    nodes: Vec<LinearNode>,
}

impl LinearTree {
    fn new() -> Self {
        Self {
            nodes: vec![LinearNode::new([0.0; D], 1e5)],
        }
    }

    fn new_child(&mut self, parent: usize, oct: usize) -> usize {
        let child_hw = self.nodes[parent].hw / 2.0;
        let center = self.nodes[parent].child_center(oct, child_hw);
        let index = self.nodes.len();
        self.nodes.push(LinearNode::new(center, child_hw));
        self.nodes[parent].children[oct] = index as i32;
        self.nodes[parent].child_count += 1;
        index
    }

    fn insert(&mut self, positions: &[[F; D]], node_index: usize, particle: usize) {
        let p = &positions[particle];
        if self.nodes[node_index].hw <= MIN_CELL {
            let node = &mut self.nodes[node_index];
            node.mass += 1.0;
            for (sum, coordinate) in node.com.iter_mut().zip(p) {
                *sum += *coordinate;
            }
            node.particle = -2;
            return;
        }

        let has_children = self.nodes[node_index].branched();
        if !has_children && self.nodes[node_index].mass == 0.0 {
            let node = &mut self.nodes[node_index];
            node.mass = 1.0;
            node.com = *p;
            node.particle = particle as i32;
            return;
        }

        if !has_children {
            let old_particle = self.nodes[node_index].particle;
            self.nodes[node_index].particle = -1;
            let old_position = &positions[old_particle as usize];
            let old_oct = LinearNode::oct(old_position, &self.nodes[node_index].c);
            let old_child = self.new_child(node_index, old_oct);
            if old_particle >= 0 {
                self.insert(positions, old_child, old_particle as usize);
            }
        }

        {
            let node = &mut self.nodes[node_index];
            node.mass += 1.0;
            for (sum, coordinate) in node.com.iter_mut().zip(p) {
                *sum += *coordinate;
            }
        }
        let oct = LinearNode::oct(p, &self.nodes[node_index].c);
        let child = if self.nodes[node_index].children[oct] < 0 {
            self.new_child(node_index, oct)
        } else {
            self.nodes[node_index].children[oct] as usize
        };
        self.insert(positions, child, particle);
    }

    fn rebuild(&mut self, positions: &[[F; D]], morton_order: bool) {
        self.nodes.clear();
        self.nodes.push(LinearNode::new([0.0; D], 1e5));
        if morton_order && positions.len() >= 4096 {
            let mut order: Vec<usize> = (0..positions.len()).collect();
            order.sort_unstable_by_key(|&i| morton_key(&positions[i]));
            for particle in order {
                self.insert(positions, 0, particle);
            }
        } else {
            // Original insertion order preserves the legacy tree and summation.
            for particle in 0..positions.len() {
                self.insert(positions, 0, particle);
            }
        }
    }
}

#[inline]
fn morton_key(p: &[F; D]) -> u64 {
    const SCALE: F = ((1u64 << 21) - 1) as F;
    const ORIGIN: F = 1e5;
    const WIDTH: F = 2e5;
    let quantize = |value: F| (((value + ORIGIN) / WIDTH).clamp(0.0, 1.0) * SCALE) as u32;
    #[inline]
    fn spread(value: u32) -> u64 {
        let mut bits = value as u64 & 0x1f_ffff;
        bits = (bits | bits << 32) & 0x001f_0000_0000_ffff;
        bits = (bits | bits << 16) & 0x001f_0000_ff00_00ff;
        bits = (bits | bits << 8) & 0x100f_00f0_0f00_f00f;
        bits = (bits | bits << 4) & 0x10c3_0c30_c30c_30c3;
        (bits | bits << 2) & 0x1249_2492_4924_9249
    }
    spread(quantize(p[0])) | spread(quantize(p[1])) << 1 | spread(quantize(p[2])) << 2
}

// Kept as an isolated compatibility path. It lets downstream users compare
// the new contiguous tree against the previous pointer tree and provides a
// safe rollback switch for sensitive layouts.
struct Oc {
    c: [F; D],
    hw: F,
    mass: F,
    com: [F; D],
    has: bool,
    ch: [Option<Box<Oc>>; 8],
    cb: bool,
    sf: F,
}

impl Oc {
    fn new(c: [F; D], hw: F) -> Self {
        Self {
            c,
            hw,
            mass: 0.0,
            com: [0.0; D],
            has: false,
            ch: Default::default(),
            cb: false,
            sf: 1.0,
        }
    }

    fn oct(p: &[F; D], ctr: &[F; D]) -> usize {
        ((p[0] >= ctr[0]) as usize)
            | (((p[1] >= ctr[1]) as usize) << 1)
            | (((p[2] >= ctr[2]) as usize) << 2)
    }

    fn cc(&self, oct: usize, hw: F) -> [F; D] {
        [
            self.c[0] + hw * (if oct & 1 != 0 { 1.0 } else { -1.0 }),
            self.c[1] + hw * (if oct & 2 != 0 { 1.0 } else { -1.0 }),
            self.c[2] + hw * (if oct & 4 != 0 { 1.0 } else { -1.0 }),
        ]
    }

    fn ins(&mut self, p: &[F; D], v: F) {
        if self.hw <= MIN_CELL {
            self.mass += v;
            self.com[0] += p[0] * v;
            self.com[1] += p[1] * v;
            self.com[2] += p[2] * v;
            return;
        }
        if !self.has && !self.cb {
            self.has = true;
            self.com = *p;
            self.mass = v;
            return;
        }
        if !self.cb {
            let op = self.com;
            let om = self.mass;
            self.has = false;
            self.cb = true;
            let o = Self::oct(&op, &self.c);
            let hw2 = self.hw / 2.0;
            self.ch[o] = Some(Box::new(Oc::new(self.cc(o, hw2), hw2)));
            self.ch[o].as_mut().unwrap().ins(&op, om);
        }
        self.mass += v;
        self.com[0] += p[0] * v;
        self.com[1] += p[1] * v;
        self.com[2] += p[2] * v;
        let o = Self::oct(p, &self.c);
        let hw2 = self.hw / 2.0;
        let cc = self.cc(o, hw2);
        let ch = self.ch[o].get_or_insert_with(|| Box::new(Oc::new(cc, hw2)));
        ch.ins(p, v);
    }

    fn fin(&mut self) {
        let mut stack: Vec<*mut Oc> = vec![self];
        while let Some(ptr) = stack.pop() {
            // The tree owns every child and is not mutated while finalising;
            // raw pointers avoid recursively growing a call stack here.
            let node = unsafe { &mut *ptr };
            if node.mass > 0.0 {
                node.com[0] /= node.mass;
                node.com[1] /= node.mass;
                node.com[2] /= node.mass;
            }
            if node.cb {
                let occupied = node.ch.iter().flatten().filter(|c| c.mass > 0.0).count();
                node.sf = if occupied > 0 {
                    (4.0 / occupied as f64).sqrt()
                } else {
                    1.0
                };
            }
            for child in node.ch.iter_mut().flatten() {
                stack.push(child.as_mut() as *mut Oc);
            }
        }
    }
}

#[inline]
fn sf(v: f64) -> f64 {
    if v.is_finite() {
        v.clamp(-1e10, 1e10)
    } else {
        0.0
    }
}

#[inline]
fn pair_force(dx: F, dy: F, dz: F, mass: F, repulsion: F, distance_max2: F) -> [F; D] {
    let d2 = dx * dx + dy * dy + dz * dz;
    if d2 > distance_max2 {
        return [0.0; D];
    }
    let l = if d2 < DISTANCE_MIN2 {
        (DISTANCE_MIN2 * d2).sqrt().max(1e-10)
    } else {
        d2
    };
    let ff = (repulsion * mass / l).min(1e10);
    [ff * dx, ff * dy, ff * dz]
}

// A preorder rope: an accepted cell jumps over its entire subtree; a rejected
// cell advances to its first child. Traversal needs neither a heap allocation
// nor a fixed-size stack that could silently drop deep-tree interactions.
#[derive(Clone, Copy)]
struct WalkNode {
    com: [F; D],
    mass: F,
    opening2: F,
    next: u32,
}

fn flatten_tree(
    tree: &LinearTree,
    source: usize,
    theta2: F,
    repulsion: F,
    walk: &mut Vec<WalkNode>,
) {
    let node = &tree.nodes[source];
    let index = walk.len();
    let width = node.hw * 2.0;
    walk.push(WalkNode {
        com: if node.mass > 0.0 {
            node.com.map(|value| value / node.mass)
        } else {
            node.com
        },
        mass: repulsion
            * if node.branched() {
                node.mass * (4.0 / node.child_count as F).sqrt()
            } else {
                node.mass
            },
        opening2: if node.branched() && node.hw > MIN_CELL {
            width * width / theta2
        } else {
            -1.0
        },
        next: 0,
    });
    for &child in &node.children {
        if child >= 0 {
            flatten_tree(tree, child as usize, theta2, repulsion, walk);
        }
    }
    walk[index].next = walk.len() as u32;
}

#[inline]
fn bh_force_linear(np: &[F; D], walk: &[WalkNode], distance_max2: F) -> [F; D] {
    let mut force = [0.0; D];
    let mut index = 0;
    while index < walk.len() {
        let node = &walk[index];
        let dx = np[0] - node.com[0];
        let dy = np[1] - node.com[1];
        let dz = np[2] - node.com[2];
        let d2 = dx * dx + dy * dy + dz * dz;
        // Leaf thresholds are -1, below every squared distance.
        if node.opening2 < d2 {
            let contribution = pair_force(dx, dy, dz, node.mass, 1.0, distance_max2);
            force[0] += contribution[0];
            force[1] += contribution[1];
            force[2] += contribution[2];
            index = node.next as usize;
        } else {
            index += 1;
        }
    }
    force
}

fn bh_force_legacy(np: &[F; D], tree: &Oc, repulsion: F, theta: F, distance_max2: F) -> [F; D] {
    let theta2 = theta.max(1e-6).powi(2);
    let mut force = [0.0; D];
    let mut stack: Vec<&Oc> = Vec::with_capacity(64);
    stack.push(tree);
    while let Some(node) = stack.pop() {
        if node.mass <= 0.0 {
            continue;
        }
        let dx = np[0] - node.com[0];
        let dy = np[1] - node.com[1];
        let dz = np[2] - node.com[2];
        let d2 = dx * dx + dy * dy + dz * dz;
        let cell_width = node.hw * 2.0;
        let approximate = node.hw <= MIN_CELL
            || (node.cb && (cell_width * cell_width / theta2) < d2)
            || (!node.cb && node.has);
        if approximate {
            let mass = if node.cb {
                node.mass * node.sf
            } else {
                node.mass
            };
            let contribution = pair_force(dx, dy, dz, mass, repulsion, distance_max2);
            force[0] += contribution[0];
            force[1] += contribution[1];
            force[2] += contribution[2];
        } else if node.cb {
            for child in node.ch.iter().rev().flatten() {
                if child.mass > 0.0 {
                    stack.push(child);
                }
            }
        }
    }
    force
}

fn spring_force(node: usize, state: &StateSoA, graph: &LinkGraph, link_distance: F) -> [F; D] {
    let mut force = [0.0; D];
    for edge in graph.offsets[node] as usize..graph.offsets[node + 1] as usize {
        let neighbor = graph.adj[edge] as usize;
        if neighbor == node {
            continue;
        }
        let dx = state.predicted[neighbor][0] - state.predicted[node][0];
        let dy = state.predicted[neighbor][1] - state.predicted[node][1];
        let dz = state.predicted[neighbor][2] - state.predicted[node][2];
        let d = (dx * dx + dy * dy + dz * dz).sqrt().max(1.0);
        let ff = sf((d - link_distance) / d * graph.strengths[edge]);
        let bias = graph.biases[edge];
        force[0] += dx * ff * bias;
        force[1] += dy * ff * bias;
        force[2] += dz * ff * bias;
    }
    force
}

struct StateSoA {
    x: Vec<F>,
    y: Vec<F>,
    z: Vec<F>,
    vx: Vec<F>,
    vy: Vec<F>,
    vz: Vec<F>,
    predicted: Vec<[F; D]>,
}

struct SimulationCore {
    n: usize,
    graph: LinkGraph,
    tree: LinearTree,
    walk: Vec<WalkNode>,
    positions: Vec<[F; D]>,
    state: StateSoA,
    order: Vec<(u64, usize)>,
    ordered_result: Vec<F>,
}

impl SimulationCore {
    fn new(links: &[u32], n: usize) -> Self {
        Self {
            n,
            graph: LinkGraph::new(links, n),
            tree: LinearTree::new(),
            walk: Vec::new(),
            positions: vec![[0.0; D]; n],
            order: Vec::new(),
            ordered_result: Vec::new(),
            state: StateSoA {
                x: vec![0.0; n],
                y: vec![0.0; n],
                z: vec![0.0; n],
                vx: vec![0.0; n],
                vy: vec![0.0; n],
                vz: vec![0.0; n],
                predicted: vec![[0.0; D]; n],
            },
        }
    }

    fn tick(&mut self, state: &[F], opts: &ForceOptions) -> Vec<F> {
        let mut result = Vec::new();
        self.tick_into(state, opts, &mut result);
        result
    }

    fn tick_into(&mut self, state: &[F], opts: &ForceOptions, result: &mut Vec<F>) {
        assert!(
            state.len() > self.n * 6,
            "state must contain n * 6 + 1 values"
        );
        let alpha = state[self.n * 6];
        let next_alpha = alpha * (1.0 - opts.alpha_decay);
        let state_soa = &mut self.state;
        for i in 0..self.n {
            let b = i * 6;
            state_soa.x[i] = sf(state[b]);
            state_soa.y[i] = sf(state[b + 1]);
            state_soa.z[i] = sf(state[b + 2]);
            state_soa.vx[i] = state[b + 3];
            state_soa.vy[i] = state[b + 4];
            state_soa.vz[i] = state[b + 5];
            self.positions[i] = [state_soa.x[i], state_soa.y[i], state_soa.z[i]];
            state_soa.predicted[i] = [
                state_soa.x[i] + state_soa.vx[i],
                state_soa.y[i] + state_soa.vy[i],
                state_soa.z[i] + state_soa.vz[i],
            ];
        }
        let distance_max2 = opts.distance_max.unwrap_or(F::INFINITY).max(0.0).powi(2);
        let use_legacy_tree = matches!(opts.algorithm.as_deref(), Some("legacy"));
        if !use_legacy_tree {
            self.tree.rebuild(
                &self.positions,
                matches!(opts.algorithm.as_deref(), Some("linear")),
            );
            self.walk.clear();
            flatten_tree(
                &self.tree,
                0,
                opts.theta.max(1e-6).powi(2),
                opts.repulsion,
                &mut self.walk,
            );
        }
        let mut legacy_tree = if use_legacy_tree {
            Some(Oc::new([0.0; D], 1e5))
        } else {
            None
        };
        if let Some(tree) = legacy_tree.as_mut() {
            for p in &self.positions {
                tree.ins(p, 1.0);
            }
            tree.fin();
        }
        let ordered = matches!(opts.algorithm.as_deref(), Some("ordered"));
        if ordered {
            self.order.clear();
            self.order.extend(
                self.positions
                    .iter()
                    .enumerate()
                    .map(|(i, p)| (morton_key(p), i)),
            );
            self.order.sort_unstable_by_key(|pair| pair.0);
            self.ordered_result.resize(self.n * 6, 0.0);
        }
        let walk = &self.walk;

        result.resize(self.n * 6 + 1, 0.0);
        let repulsion = opts.repulsion;
        let theta = opts.theta;
        let link_distance = opts.link_distance;
        let velocity_decay = opts.velocity_decay;
        let compute = |i: usize, out: &mut [F]| {
            let position = [state_soa.x[i], state_soa.y[i], state_soa.z[i]];
            let rep = if !use_legacy_tree {
                bh_force_linear(&position, walk, distance_max2)
            } else {
                bh_force_legacy(
                    &position,
                    legacy_tree.as_ref().expect("legacy tree"),
                    repulsion,
                    theta,
                    distance_max2,
                )
            };
            let spring = spring_force(i, state_soa, &self.graph, link_distance);
            let next_vx = sf((state_soa.vx[i] + sf(rep[0] + spring[0]) * alpha) * velocity_decay);
            let next_vy = sf((state_soa.vy[i] + sf(rep[1] + spring[1]) * alpha) * velocity_decay);
            let next_vz = sf((state_soa.vz[i] + sf(rep[2] + spring[2]) * alpha) * velocity_decay);
            out[0] = sf(state_soa.x[i] + next_vx);
            out[1] = sf(state_soa.y[i] + next_vy);
            out[2] = sf(state_soa.z[i] + next_vz);
            out[3] = next_vx;
            out[4] = next_vy;
            out[5] = next_vz;
        };
        if ordered {
            let order = &self.order;
            self.ordered_result
                .par_chunks_exact_mut(6)
                .enumerate()
                .for_each(|(slot, out)| compute(order[slot].1, out));
            for (slot, &(_, node)) in order.iter().enumerate() {
                result[node * 6..node * 6 + 6]
                    .copy_from_slice(&self.ordered_result[slot * 6..slot * 6 + 6]);
            }
        } else if rayon::current_num_threads() < 2 {
            for (i, out) in result[..self.n * 6]
                .as_chunks_mut::<6>()
                .0
                .iter_mut()
                .enumerate()
            {
                compute(i, out);
            }
        } else {
            result[..self.n * 6]
                .par_chunks_exact_mut(6)
                .enumerate()
                .for_each(|(i, out)| compute(i, out));
        }

        if opts.center_strength > 0.0 && self.n > 0 {
            let (sx, sy, sz) = result[..self.n * 6]
                .par_chunks_exact(6)
                .map(|p| (p[0], p[1], p[2]))
                .reduce(|| (0.0, 0.0, 0.0), |a, b| (a.0 + b.0, a.1 + b.1, a.2 + b.2));
            let shift_x = sx / self.n as F * opts.center_strength;
            let shift_y = sy / self.n as F * opts.center_strength;
            let shift_z = sz / self.n as F * opts.center_strength;
            for p in result[..self.n * 6].as_chunks_mut::<6>().0 {
                p[0] = sf(p[0] - shift_x);
                p[1] = sf(p[1] - shift_y);
                p[2] = sf(p[2] - shift_z);
            }
        }
        result[self.n * 6] = next_alpha;
    }

    fn run(
        &mut self,
        state: &[F],
        opts: &ForceOptions,
        max_ticks: u32,
        alpha_min: F,
        time_limit_ms: Option<F>,
    ) -> (Vec<F>, u32, bool) {
        let mut current = state.to_vec();
        let mut next = Vec::with_capacity(self.n * 6 + 1);
        let mut ticks = 0;
        let started = time_limit_ms.map(|_| Instant::now());
        let mut stopped_by_time = false;
        for _ in 0..max_ticks {
            self.tick_into(&current, opts, &mut next);
            std::mem::swap(&mut current, &mut next);
            ticks += 1;
            if current[self.n * 6] < alpha_min {
                break;
            }
            if let (Some(start), Some(limit)) = (started, time_limit_ms) {
                if start.elapsed().as_secs_f64() * 1000.0 > limit {
                    stopped_by_time = true;
                    break;
                }
            }
        }
        (current, ticks, stopped_by_time)
    }
}

#[napi(object)]
pub struct SimulationResult {
    pub state: Float64Array,
    pub ticks: u32,
    pub stopped_by_time: bool,
}

#[napi]
pub struct Simulation {
    core: SimulationCore,
}

#[napi]
impl Simulation {
    #[napi]
    pub fn tick(&mut self, state: Float64Array, opts: ForceOptions) -> Float64Array {
        self.core.tick(state.as_ref(), &opts).into()
    }

    /// Run a bounded batch without exposing intermediate arrays to the JS heap.
    /// Stops after the same completed tick that repeated `tick` calls would stop.
    #[napi]
    pub fn run(
        &mut self,
        state: Float64Array,
        opts: ForceOptions,
        max_ticks: u32,
        alpha_min: f64,
        time_limit_ms: Option<f64>,
    ) -> SimulationResult {
        let (state, ticks, stopped_by_time) =
            self.core
                .run(state.as_ref(), &opts, max_ticks, alpha_min, time_limit_ms);
        SimulationResult {
            state: state.into(),
            ticks,
            stopped_by_time,
        }
    }
}

#[napi]
pub fn create_simulation(links: Uint32Array, n: u32) -> Simulation {
    Simulation {
        core: SimulationCore::new(links.as_ref(), n as usize),
    }
}

#[napi]
pub fn sim_tick(state: Vec<f64>, links: Vec<u32>, n: u32, opts: ForceOptions) -> Vec<f64> {
    let mut core = SimulationCore::new(&links, n as usize);
    core.tick(&state, &opts)
}

#[cfg(feature = "pgo")]
#[napi]
pub fn flush_pgo_profile() -> Result<()> {
    unsafe extern "C" {
        fn __llvm_profile_write_file() -> i32;
    }
    let status = unsafe { __llvm_profile_write_file() };
    if status == 0 {
        Ok(())
    } else {
        Err(Error::new(
            Status::GenericFailure,
            "LLVM profile flush failed",
        ))
    }
}
