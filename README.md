[English](./README.md) | [中文](./README_CN.md)

# @xingwangzhe/force-rs

Fast force-directed 3D graph layout powered by **Rust + a contiguous Barnes-Hut octree**, designed as a build-time replacement for `d3-force-3d`.

### Why this exists

`d3-force-3d` is excellent for interactive visualizations, but at build time (SSG/SSR) with 47K+ nodes and 89K+ edges, the JS runtime becomes a bottleneck — hundreds of ticks at ~1s each is too slow. `force-rs` implements the **exact same force model** in Rust, achieving **~0.01s/tick** on 16 cores.

### Force model (fully d3-force compatible)

| Force          | Implementation                                           |
|----------------|----------------------------------------------------------|
| Many-body repulsion | Barnes-Hut octree, O(n log n), with `√(4/k)` correction |
| Link (spring)  | Degree-biased strength: `1/min(deg, deg)`, bias = other's degree ratio |
| Center (translational) | Shift centroid toward origin (same as d3-force `forceCenter`) |
| Distance softening | Smooth `√(d_min² × d²)` below 1.0 (same as d3-force) |

### Other features

- **16-core** parallel via [Rayon](https://github.com/rayon-rs/rayon) — auto-fallback to sequential on single-core
- **Compatible state format**: `[x, y, z, vx, vy, vz, ...alpha]` — same as d3-force
- Built with [NAPI-RS](https://napi.rs/) with a typed-array fast path

## Installation

```bash
npm install @xingwangzhe/force-rs
```

## API

### `simTick(state, links, n, options)`

Execute one tick of force-directed simulation.

```ts
import { simTick } from '@xingwangzhe/force-rs';

const opts = {
  repulsion: 3000,      // many-body charge strength
  linkDistance: 500,    // natural spring length
  centerStrength: 0.005,// translational center gravity
  theta: 0.8,           // Barnes-Hut approximation threshold
  velocityDecay: 0.60,  // velocity damping per tick
  alphaDecay: 0.02,     // cooling rate per tick
};

// state: n*6 + 1 floats [x0,y0,z0,vx0,vy0,vz0, ..., alpha]
// links: 2*m u32 [src0,tgt0, src1,tgt1, ...]
const newState = simTick(state, links, n, opts);
```

For repeated ticks, preprocess links once and use typed arrays:

```ts
import { createSimulation } from '@xingwangzhe/force-rs';

const simulation = createSimulation(new Uint32Array(links), n);
const nextState = simulation.tick(new Float64Array(state), opts);
```

**ForceOptions:**

| Field           | Type   | Description                            |
|-----------------|--------|----------------------------------------|
| `repulsion`      | number | Many-body charge strength              |
| `linkDistance`   | number | Natural spring length                  |
| `centerStrength` | number | Translational center gravity           |
| `theta`          | number | Barnes-Hut approximation threshold     |
| `velocityDecay`  | number | Velocity damping factor per tick       |
| `alphaDecay`     | number | Alpha cooling rate per tick            |
| `algorithm`      | string | `fast` (default), `linear` experimental, or `legacy` comparison path |
| `distanceMax`    | number | Optional finite repulsion radius |
| `algorithm`      | string | `fast` (default) or `legacy` comparison path |
| `distanceMax`    | number | Optional finite repulsion radius |

**Returns:** `Array<number>` for `simTick`, or `Float64Array` for the prepared typed-array API.

The default `fast` path uses a reusable SoA state layout, precomputed CSR links, and the fastest validated tree path for the current graph size. Set `algorithm: "linear"` to benchmark the contiguous Morton-ordered octree, or `algorithm: "legacy"` when comparing with the previous pointer-based tree.

## Usage Example

```ts
import { simTick } from '@xingwangzhe/force-rs';

// 3 nodes: positions + velocities + alpha
const state = [
  0, 0, 0, 0, 0, 0,    // node 0
  100, 0, 0, 0, 0, 0,  // node 1
  0, 100, 0, 0, 0, 0,  // node 2
  1.0                    // alpha
];
const links = [0, 1, 0, 2]; // node 0 connected to 1 and 2

const opts = {
  repulsion: 3000,
  linkDistance: 500,
  centerStrength: 0.005,
  theta: 0.8,
  velocityDecay: 0.60,
  alphaDecay: 0.02,
};

let s = state;
while (s[s.length - 1] > 0.001) {
  s = simTick(s, links, 3, opts);
}
// s now contains converged 3D positions
```

## Real-world Performance

| Platform | 47K nodes × 89K edges | Notes |
|----------|----------------------|-------|
| 16-core  | **~0.01s/tick**      | Rayon `par_iter` across 16 threads |
| 1-core   | ~0.15s/tick          | Auto-fallback to sequential |

Measured on AMD EPYC / Intel Xeon build servers. The Barnes-Hut tree is custom-built in `src/lib.rs` (no external tree crate dependency); nodes are stored contiguously to reduce pointer chasing and allocator pressure.

## License

MIT
