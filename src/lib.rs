#![deny(clippy::all)]

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
}

fn sf(v: f64) -> f64 {
    if v.is_finite() {
        v.clamp(-1e10, 1e10)
    } else {
        0.0
    }
}

struct Oc {
    c: [F; D],                  // center of cell
    hw: F,                      // half-width
    mass: F,                    // total mass (node count)
    com: [F; D],                // center of mass (average position)
    has: bool,                  // has exactly one particle
    ch: [Option<Box<Oc>>; 8],   // children
    cb: bool,                   // has been branched (has children)
    sf: F,                      // Barnes-Hut correction factor sqrt(4/numChildren)
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
        let mut s: Vec<&mut Oc> = vec![self];
        while let Some(n) = s.pop() {
            if n.mass > 0.0 {
                n.com[0] /= n.mass;
                n.com[1] /= n.mass;
                n.com[2] /= n.mass;
            }
            if n.cb {
                let occupied = n.ch.iter().flatten().filter(|c| c.mass > 0.0).count();
                n.sf = if occupied > 0 {
                    (4.0 / occupied as f64).sqrt()
                } else {
                    1.0
                };
            }
            for c in n.ch.iter_mut().flatten() {
                s.push(c);
            }
        }
    }
}

/// Barnes-Hut repulsion force on particle at `np` from octree `t`.
/// Uses d3-force compatible softening and Barnes-Hut correction.
fn bh_force(np: &[F; D], t: &Oc, rep: f64, theta: f64) -> [F; D] {
    let theta2 = theta * theta;
    let mut f = [0.0; D];
    let mut st: Vec<&Oc> = Vec::with_capacity(64);
    st.push(t);
    while let Some(n) = st.pop() {
        if n.mass <= 0.0 {
            continue;
        }
        let dx = np[0] - n.com[0];
        let dy = np[1] - n.com[1];
        let dz = np[2] - n.com[2];
        let d2 = dx * dx + dy * dy + dz * dz;

        let cell_width = n.hw * 2.0;
        let can_approximate = n.hw <= MIN_CELL
            || (n.cb && (cell_width * cell_width / theta2) < d2)
            || (!n.cb && n.has);

        if can_approximate {
            // d3-force distance softening: if l < distanceMin2, l = sqrt(distanceMin2 * l)
            let l = if d2 < DISTANCE_MIN2 {
                (DISTANCE_MIN2 * d2).sqrt().max(1e-10)
            } else {
                d2
            };
            // Barnes-Hut correction factor for internal nodes
            let mass = if n.cb { n.mass * n.sf } else { n.mass };
            let ff = (rep * mass / l).min(1e10);
            f[0] += ff * dx;
            f[1] += ff * dy;
            f[2] += ff * dz;
        } else if n.cb {
            for c in n.ch.iter().rev().flatten() {
                if c.mass > 0.0 {
                    st.push(c);
                }
            }
        }
    }
    f
}

/// Spring (link) force — matches d3-force link force exactly:
/// - strength = 1 / min(deg[source], deg[target])
/// - bias = other_deg / (my_deg + other_deg), i.e. the OTHER node's
///   degree proportion controls THIS node's movement
fn spring_force(
    node: usize,
    pos: &[[F; D]],
    vel: &[[F; D]],
    adj: &[Vec<u32>],
    deg: &[u32],
    ld: f64,
) -> [F; D] {
    let p = pos[node];
    let v = vel[node];
    let mut a = [0.0; D];
    let my_deg = deg[node].max(1) as f64;
    for &n in &adj[node] {
        let ni = n as usize;
        if ni == node {
            continue;
        }
        let other_deg = deg[ni].max(1) as f64;
        // d3-force: "look-ahead" with velocity
        let dx = (pos[ni][0] + vel[ni][0]) - (p[0] + v[0]);
        let dy = (pos[ni][1] + vel[ni][1]) - (p[1] + v[1]);
        let dz = (pos[ni][2] + vel[ni][2]) - (p[2] + v[2]);
        let d = (dx * dx + dy * dy + dz * dz).sqrt().max(1.0);
        let strength = 1.0 / (my_deg.min(other_deg).max(1.0));
        let ff = sf((d - ld) / d * strength);
        // d3-force bias: this node gets force scaled by the OTHER node's degree ratio
        let bias = other_deg / (my_deg + other_deg);
        a[0] += dx * ff * bias;
        a[1] += dy * ff * bias;
        a[2] += dz * ff * bias;
    }
    a
}

#[napi]
pub fn sim_tick(state: Vec<f64>, links: Vec<u32>, n: u32, opts: ForceOptions) -> Vec<f64> {
    let nu = n as usize;
    let alpha = state[state.len() - 1];
    let na = alpha * (1.0 - opts.alpha_decay);

    let mut pos: Vec<[F; D]> = Vec::with_capacity(nu);
    let mut vel: Vec<[F; D]> = Vec::with_capacity(nu);
    for i in 0..nu {
        let b = i * 6;
        pos.push([sf(state[b]), sf(state[b + 1]), sf(state[b + 2])]);
        vel.push([state[b + 3], state[b + 4], state[b + 5]]);
    }

    // Build adjacency and degree
    let mut adj: Vec<Vec<u32>> = vec![Vec::new(); nu];
    let mut deg: Vec<u32> = vec![0; nu];
    for l in (0..links.len()).step_by(2) {
        let s = links[l] as usize;
        let t = links[l + 1] as usize;
        if s < nu && t < nu {
            adj[s].push(t as u32);
            adj[t].push(s as u32);
            deg[s] += 1;
            deg[t] += 1;
        }
    }

    // Build Barnes-Hut octree
    let mut tree = Oc::new([0.0; D], 1e5);
    for p in &pos {
        tree.ins(p, 1.0);
    }
    tree.fin();

    let rep = opts.repulsion;
    let th = opts.theta;
    let ld = opts.link_distance;
    let cs = opts.center_strength;
    let mut res = vec![0.0; nu * 6 + 1];

    // Compute forces: repulsion + spring
    if rayon::current_num_threads() < 2 {
        for i in 0..nu {
            let rep_f = bh_force(&pos[i], &tree, rep, th);
            let spr = spring_force(i, &pos, &vel, &adj, &deg, ld);
            let vx = vel[i][0] + sf(rep_f[0] + spr[0]) * alpha;
            let vy = vel[i][1] + sf(rep_f[1] + spr[1]) * alpha;
            let vz = vel[i][2] + sf(rep_f[2] + spr[2]) * alpha;
            let vx = sf(vx * opts.velocity_decay);
            let vy = sf(vy * opts.velocity_decay);
            let vz = sf(vz * opts.velocity_decay);
            let b = i * 6;
            res[b] = sf(pos[i][0] + vx);
            res[b + 1] = sf(pos[i][1] + vy);
            res[b + 2] = sf(pos[i][2] + vz);
            res[b + 3] = vx;
            res[b + 4] = vy;
            res[b + 5] = vz;
        }
    } else {
        let rp = res.as_mut_ptr() as usize;
        (0..nu).into_par_iter().for_each(|i| {
            let rep_f = bh_force(&pos[i], &tree, rep, th);
            let spr = spring_force(i, &pos, &vel, &adj, &deg, ld);
            let vx = vel[i][0] + sf(rep_f[0] + spr[0]) * alpha;
            let vy = vel[i][1] + sf(rep_f[1] + spr[1]) * alpha;
            let vz = vel[i][2] + sf(rep_f[2] + spr[2]) * alpha;
            let vx = sf(vx * opts.velocity_decay);
            let vy = sf(vy * opts.velocity_decay);
            let vz = sf(vz * opts.velocity_decay);
            unsafe {
                let rp2 = rp as *mut f64;
                let p = rp2.add(i * 6);
                *p = sf(pos[i][0] + vx);
                *p.add(1) = sf(pos[i][1] + vy);
                *p.add(2) = sf(pos[i][2] + vz);
                *p.add(3) = vx;
                *p.add(4) = vy;
                *p.add(5) = vz;
            }
        });
    }

    // d3-force center: translational shift of centroid toward origin.
    // Applied AFTER position integration, matching d3-force order.
    if cs > 0.0 {
        let mut sx = 0.0;
        let mut sy = 0.0;
        let mut sz = 0.0;
        for i in 0..nu {
            let b = i * 6;
            sx += res[b];
            sy += res[b + 1];
            sz += res[b + 2];
        }
        sx = sx / nu as f64 * cs;
        sy = sy / nu as f64 * cs;
        sz = sz / nu as f64 * cs;
        for i in 0..nu {
            let b = i * 6;
            res[b] = sf(res[b] - sx);
            res[b + 1] = sf(res[b + 1] - sy);
            res[b + 2] = sf(res[b + 2] - sz);
        }
    }

    res[nu * 6] = na;
    res
}
