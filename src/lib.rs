#![deny(clippy::all)]

use napi_derive::napi;
use rayon::prelude::*;
use zhifeng_impl_barnes_hut_tree::BarnesHutTree;

type Fnum = f64;
const DIM: usize = 3;

#[napi(object)]
pub struct ForceOptions {
    pub repulsion: f64,
    pub link_distance: f64,
    pub center_strength: f64,
    pub theta: f64,
    pub velocity_decay: f64,
    pub alpha_decay: f64,
}

/// 执行一次力导仿真 tick
/// state: [x0,y0,z0,vx0,vy0,vz0, ...] 共 n*6 个 f64，最后一位是 alpha
/// links: [src0,tgt0, ...] 共 2*m 个 u32
/// 返回: 新的 state（同格式）+ new_alpha
#[napi]
pub fn sim_tick(
    state: Vec<f64>,
    links: Vec<u32>,
    n: u32,
    opts: ForceOptions,
) -> Vec<f64> {
    let n_usize = n as usize;
    let alpha = state[state.len() - 1];
    let new_alpha = alpha * (1.0 - opts.alpha_decay);

    // 提取位置
    let mut pos: Vec<[Fnum; DIM]> = Vec::with_capacity(n_usize);
    let mut vel: Vec<f64> = Vec::with_capacity(n_usize * DIM);
    for i in 0..n_usize {
        let b = i * 6;
        pos.push([state[b], state[b + 1], state[b + 2]]);
        vel.push(state[b + 3]);
        vel.push(state[b + 4]);
        vel.push(state[b + 5]);
    }

    // Barnes-Hut 构建
    let mut tree: BarnesHutTree<DIM> = BarnesHutTree::new();
    for p in &pos {
        tree.push(p);
    }

    let repulsion = opts.repulsion;
    let theta = opts.theta;
    let link_dist = opts.link_distance;
    let center_str = opts.center_strength;

    // 并行计算力
    let force_updates: Vec<([Fnum; DIM], [Fnum; DIM])> = if rayon::current_num_threads() < 2 {
        (0..n_usize)
            .map(|i| {
                compute_forces(
                    i, &pos, &links, &tree, repulsion, theta, link_dist, center_str, n_usize,
                )
            })
            .collect()
    } else {
        (0..n_usize)
            .into_par_iter()
            .map(|i| {
                compute_forces(
                    i, &pos, &links, &tree, repulsion, theta, link_dist, center_str, n_usize,
                )
            })
            .collect()
    };

    // 应用力 + 积分
    let mut result = Vec::with_capacity(n_usize * 6 + 1);
    for i in 0..n_usize {
        let (rep, attr) = &force_updates[i];
        vel[i * 3] = (vel[i * 3] + rep[0] * alpha + attr[0]).clamp(-1000.0, 1000.0) * opts.velocity_decay;
        vel[i * 3 + 1] = (vel[i * 3 + 1] + rep[1] * alpha + attr[1]).clamp(-1000.0, 1000.0) * opts.velocity_decay;
        vel[i * 3 + 2] = (vel[i * 3 + 2] + rep[2] * alpha + attr[2]).clamp(-1000.0, 1000.0) * opts.velocity_decay;

        let new_x = pos[i][0] + vel[i * 3] * alpha;
        let new_y = pos[i][1] + vel[i * 3 + 1] * alpha;
        let new_z = pos[i][2] + vel[i * 3 + 2] * alpha;

        result.extend_from_slice(&[new_x, new_y, new_z, vel[i * 3], vel[i * 3 + 1], vel[i * 3 + 2]]);
    }
    result.push(new_alpha);
    result
}

fn compute_forces(
    node: usize,
    pos: &[[Fnum; DIM]],
    links: &[u32],
    tree: &BarnesHutTree<DIM>,
    repulsion: f64,
    theta: f64,
    link_dist: f64,
    center_str: f64,
    _n: usize,
) -> ([Fnum; DIM], [Fnum; DIM]) {
    let p = pos[node];

    // Barnes-Hut 斥力
    let mut rep = [0.0; DIM];
    let moved_node = node;

    tree.calc_force_on_value(
        moved_node,
        |_v, _w, h| {
            let d = h as f64;
            d >= theta
        },
        |_vi, _wi, _idx, out: &mut [Fnum; DIM]| {
            let dx = p[0] - _vi[0];
            let dy = p[1] - _vi[1];
            let dz = p[2] - _vi[2];
            let d2 = dx * dx + dy * dy + dz * dz;
            if d2 < 1.0 { return; }
            let d = d2.sqrt();
            let f = repulsion / d2;
            out[0] += f * dx / d;
            out[1] += f * dy / d;
            out[2] += f * dz / d;
        },
        &mut rep,
    );

    // 弹簧引力
    let mut attr = [0.0; DIM];
    for l in (0..links.len()).step_by(2) {
        let src = links[l] as usize;
        let tgt = links[l + 1] as usize;
        if src == node || tgt == node {
            let other = if src == node { tgt } else { src };
            let dx = pos[other][0] - p[0];
            let dy = pos[other][1] - p[1];
            let dz = pos[other][2] - p[2];
            let d = (dx * dx + dy * dy + dz * dz).sqrt().max(1.0);
            let f = (d - link_dist) / d;
            attr[0] += dx * f;
            attr[1] += dy * f;
            attr[2] += dz * f;
        }
    }

    // 中心力
    let cd = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt().max(1.0);
    attr[0] += -p[0] * center_str / cd;
    attr[1] += -p[1] * center_str / cd;
    attr[2] += -p[2] * center_str / cd;

    (rep, attr)
}
