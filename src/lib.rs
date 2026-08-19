#![deny(clippy::all)]

use napi::bindgen_prelude::{Float64Array, Uint32Array};
use napi_derive::napi;
use rayon::prelude::*;

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
    deg: Vec<u32>,
}

impl LinkGraph {
    fn new(links: &[u32], n: usize) -> Self {
        let mut deg = vec![0u32; n];
        for pair in links.chunks_exact(2) {
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
        for pair in links.chunks_exact(2) {
            let s = pair[0] as usize;
            let t = pair[1] as usize;
            if s < n && t < n {
                adj[cursor[s] as usize] = t as u32;
                cursor[s] += 1;
                adj[cursor[t] as usize] = s as u32;
                cursor[t] += 1;
            }
        }
        Self { adj, offsets, deg }
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
    sf: F,
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
            sf: 1.0,
        }
    }

    #[inline]
    fn branched(&self) -> bool {
        self.children.iter().any(|&c| c >= 0)
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

    fn rebuild(&mut self, positions: &[[F; D]]) {
        self.nodes.clear();
        self.nodes.push(LinearNode::new([0.0; D], 1e5));
        let mut order: Vec<usize> = (0..positions.len()).collect();
        if positions.len() >= 4096 {
            order.sort_unstable_by_key(|&i| morton_key(&positions[i]));
        }
        for particle in order {
            self.insert(positions, 0, particle);
        }
        self.finalize();
    }

    fn finalize(&mut self) {
        for index in (0..self.nodes.len()).rev() {
            let children = self.nodes[index].children;
            let occupied = children
                .iter()
                .filter(|&&child| child >= 0 && self.nodes[child as usize].mass > 0.0)
                .count();
            let node = &mut self.nodes[index];
            if node.mass > 0.0 {
                node.com[0] /= node.mass;
                node.com[1] /= node.mass;
                node.com[2] /= node.mass;
            }
            node.sf = if occupied > 0 {
                (4.0 / occupied as f64).sqrt()
            } else {
                1.0
            };
        }
    }
}

#[inline]
fn morton_key(p: &[F; D]) -> u64 {
    const SCALE: F = ((1u64 << 21) - 1) as F;
    const ORIGIN: F = 1e5;
    const WIDTH: F = 2e5;
    let quantize = |value: F| (((value + ORIGIN) / WIDTH).clamp(0.0, 1.0) * SCALE) as u32;
    let x = quantize(p[0]);
    let y = quantize(p[1]);
    let z = quantize(p[2]);
    let mut key = 0u64;
    for bit in 0..21 {
        key |= ((x as u64 >> bit) & 1) << (bit * 3);
        key |= ((y as u64 >> bit) & 1) << (bit * 3 + 1);
        key |= ((z as u64 >> bit) & 1) << (bit * 3 + 2);
    }
    key
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

fn bh_force_linear(
    np: &[F; D],
    tree: &LinearTree,
    repulsion: F,
    theta: F,
    distance_max2: F,
) -> [F; D] {
    let theta2 = theta.max(1e-6).powi(2);
    let mut force = [0.0; D];
    let mut stack = [0usize; 256];
    let mut stack_len = 1usize;
    stack[0] = 0;
    while stack_len > 0 {
        stack_len -= 1;
        let node = &tree.nodes[stack[stack_len]];
        if node.mass <= 0.0 {
            continue;
        }
        let dx = np[0] - node.com[0];
        let dy = np[1] - node.com[1];
        let dz = np[2] - node.com[2];
        let d2 = dx * dx + dy * dy + dz * dz;
        let cell_width = node.hw * 2.0;
        let approximate = node.hw <= MIN_CELL
            || (node.branched() && (cell_width * cell_width / theta2) < d2)
            || (!node.branched() && node.particle >= 0);
        if approximate {
            let mass = if node.branched() {
                node.mass * node.sf
            } else {
                node.mass
            };
            let contribution = pair_force(dx, dy, dz, mass, repulsion, distance_max2);
            force[0] += contribution[0];
            force[1] += contribution[1];
            force[2] += contribution[2];
        } else if node.branched() {
            for &child in node.children.iter().rev() {
                if child >= 0 && stack_len < stack.len() {
                    stack[stack_len] = child as usize;
                    stack_len += 1;
                }
            }
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
    let my_deg = graph.deg[node].max(1) as F;
    for &neighbor in &graph.adj[graph.offsets[node] as usize..graph.offsets[node + 1] as usize] {
        let ni = neighbor as usize;
        if ni == node {
            continue;
        }
        let other_deg = graph.deg[ni].max(1) as F;
        let dx = (state.x[ni] + state.vx[ni]) - (state.x[node] + state.vx[node]);
        let dy = (state.y[ni] + state.vy[ni]) - (state.y[node] + state.vy[node]);
        let dz = (state.z[ni] + state.vz[ni]) - (state.z[node] + state.vz[node]);
        let d = (dx * dx + dy * dy + dz * dz).sqrt().max(1.0);
        let strength = 1.0 / my_deg.min(other_deg).max(1.0);
        let ff = sf((d - link_distance) / d * strength);
        let bias = other_deg / (my_deg + other_deg);
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
}

struct SimulationCore {
    n: usize,
    graph: LinkGraph,
    tree: LinearTree,
}

impl SimulationCore {
    fn new(links: &[u32], n: usize) -> Self {
        Self {
            n,
            graph: LinkGraph::new(links, n),
            tree: LinearTree::new(),
        }
    }

    fn tick(&mut self, state: &[F], opts: &ForceOptions) -> Vec<F> {
        assert!(
            state.len() > self.n * 6,
            "state must contain n * 6 + 1 values"
        );
        let alpha = state[self.n * 6];
        let next_alpha = alpha * (1.0 - opts.alpha_decay);
        let mut state_soa = StateSoA {
            x: vec![0.0; self.n],
            y: vec![0.0; self.n],
            z: vec![0.0; self.n],
            vx: vec![0.0; self.n],
            vy: vec![0.0; self.n],
            vz: vec![0.0; self.n],
        };
        for i in 0..self.n {
            let b = i * 6;
            state_soa.x[i] = sf(state[b]);
            state_soa.y[i] = sf(state[b + 1]);
            state_soa.z[i] = sf(state[b + 2]);
            state_soa.vx[i] = state[b + 3];
            state_soa.vy[i] = state[b + 4];
            state_soa.vz[i] = state[b + 5];
        }
        let positions: Vec<[F; D]> = (0..self.n)
            .map(|i| [state_soa.x[i], state_soa.y[i], state_soa.z[i]])
            .collect();
        let distance_max2 = opts.distance_max.unwrap_or(F::INFINITY).max(0.0).powi(2);
        let use_linear_tree = matches!(opts.algorithm.as_deref(), Some("linear"));
        let linear_tree = if use_linear_tree {
            self.tree.rebuild(&positions);
            Some(&self.tree)
        } else {
            None
        };
        let mut legacy_tree = if use_linear_tree {
            None
        } else {
            Some(Oc::new([0.0; D], 1e5))
        };
        if let Some(tree) = legacy_tree.as_mut() {
            for p in &positions {
                tree.ins(p, 1.0);
            }
            tree.fin();
        }

        let mut result = vec![0.0; self.n * 6 + 1];
        let repulsion = opts.repulsion;
        let theta = opts.theta;
        let link_distance = opts.link_distance;
        let velocity_decay = opts.velocity_decay;
        let compute = |i: usize, out: &mut [F]| {
            let position = [state_soa.x[i], state_soa.y[i], state_soa.z[i]];
            let rep = if let Some(tree) = linear_tree.as_ref() {
                bh_force_linear(&position, tree, repulsion, theta, distance_max2)
            } else {
                bh_force_legacy(
                    &position,
                    legacy_tree.as_ref().expect("legacy tree"),
                    repulsion,
                    theta,
                    distance_max2,
                )
            };
            let spring = spring_force(i, &state_soa, &self.graph, link_distance);
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
        if rayon::current_num_threads() < 2 {
            for (i, out) in result[..self.n * 6].chunks_exact_mut(6).enumerate() {
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
            for p in result[..self.n * 6].chunks_exact_mut(6) {
                p[0] = sf(p[0] - shift_x);
                p[1] = sf(p[1] - shift_y);
                p[2] = sf(p[2] - shift_z);
            }
        }
        result[self.n * 6] = next_alpha;
        result
    }
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
