[English](./README.md) | [中文](./README_CN.md)

# @xingwangzhe/force-rs

Fast force-directed 3D graph layout powered by **Rust + a contiguous Barnes-Hut octree**, designed as a build-time replacement for `d3-force-3d`.

### Why this exists

`d3-force-3d` is excellent for interactive visualizations, but at build time (SSG/SSR) with 47K+ nodes and 89K+ edges, the JS runtime becomes a bottleneck — hundreds of ticks at ~1s each is too slow. `force-rs` implements d3-style repulsion, degree-biased springs and translational centering in Rust. Runtime depends on graph geometry, force options, hardware and thread count; measured consumer results are documented below.

### Force model

| Force                  | Implementation                                                         |
| ---------------------- | ---------------------------------------------------------------------- |
| Many-body repulsion    | Barnes-Hut octree, O(n log n), with `√(4/k)` correction                |
| Link (spring)          | Degree-biased strength: `1/min(deg, deg)`, bias = other's degree ratio |
| Center (translational) | Shift centroid toward origin (same as d3-force `forceCenter`)          |
| Distance softening     | Smooth `√(d_min² × d²)` below 1.0 (same as d3-force)                   |

### Other features

- **16-core** parallel via [Rayon](https://github.com/rayon-rs/rayon) — auto-fallback to sequential on single-core
- **Compatible state format**: `[x, y, z, vx, vy, vz, ...alpha]` — same as d3-force
- Built with [NAPI-RS](https://napi.rs/) with a typed-array fast path

## Installation

```bash
npm install @xingwangzhe/force-rs
```

The root package contains only the ESM loader and types. CI publishes each native binary as a scoped platform package, and npm installs only the matching optional dependency.

## API

### `simTick(state, links, n, options)`

Execute one tick of force-directed simulation.

```ts
import { simTick } from "@xingwangzhe/force-rs";

const opts = {
  repulsion: 3000, // many-body charge strength
  linkDistance: 500, // natural spring length
  centerStrength: 0.005, // translational center gravity
  theta: 0.8, // Barnes-Hut approximation threshold
  velocityDecay: 0.6, // velocity damping per tick
  alphaDecay: 0.02, // cooling rate per tick
};

// state: n*6 + 1 floats [x0,y0,z0,vx0,vy0,vz0, ..., alpha]
// links: 2*m u32 [src0,tgt0, src1,tgt1, ...]
const newState = simTick(state, links, n, opts);
```

For repeated ticks, preprocess links once and use typed arrays:

```ts
import { createSimulation } from "@xingwangzhe/force-rs";

const simulation = createSimulation(new Uint32Array(links), n);
const nextState = simulation.tick(new Float64Array(state), opts);
```

**ForceOptions:**

| Field            | Type   | Description                                                          |
| ---------------- | ------ | -------------------------------------------------------------------- |
| `repulsion`      | number | Many-body charge strength                                            |
| `linkDistance`   | number | Natural spring length                                                |
| `centerStrength` | number | Translational center gravity                                         |
| `theta`          | number | Barnes-Hut approximation threshold                                   |
| `velocityDecay`  | number | Velocity damping factor per tick                                     |
| `alphaDecay`     | number | Alpha cooling rate per tick                                          |
| `algorithm`      | string | `fast` (default), experimental `ordered` / `linear`, or `legacy` reference |
| `distanceMax`    | number | Optional finite repulsion radius                                     |

**Returns:** `Array<number>` for `simTick`, or `Float64Array` for the prepared typed-array API.

The default `fast` path reuses SoA state, precomputed CSR spring coefficients and a contiguous octree with stackless preorder traversal. It preserves the original insertion order and force contribution order. Experimental `ordered` evaluates independent nodes in Morton order and restores the public node order; `linear` changes tree insertion order. Use `legacy` to compare with the previous pointer-based tree. Parallel centroid reductions may produce ordinary floating-point roundoff differences.

### Bounded batched simulation

```ts
const result = simulation.run(new Float64Array(state), opts, 500, 0.001, 14 * 60 * 1000);
// result.state: Float64Array; result.ticks: number; result.stoppedByTime: boolean
```

`run` performs at most `maxTicks` ticks, checks `alpha < alphaMin` after every
completed tick, and optionally stops when `timeLimitMs` has elapsed. It returns
the exact completed tick count and reports a time cutoff separately from cooling
completion. A zero tick limit returns a copy of the input. Inputs are not mutated.
Two native result buffers are reused throughout the batch, so intermediate states
do not each become a JS external typed array. Use smaller batches for progress
reporting or cancellation between calls. A time cutoff is an incomplete layout;
the consumer decides whether it can be used or cached.

## Usage Example

```ts
import { simTick } from "@xingwangzhe/force-rs";

// 3 nodes: positions + velocities + alpha
const state = [
  0,
  0,
  0,
  0,
  0,
  0, // node 0
  100,
  0,
  0,
  0,
  0,
  0, // node 1
  0,
  100,
  0,
  0,
  0,
  0, // node 2
  1.0, // alpha
];
const links = [0, 1, 0, 2]; // node 0 connected to 1 and 2

const opts = {
  repulsion: 3000,
  linkDistance: 500,
  centerStrength: 0.005,
  theta: 0.8,
  velocityDecay: 0.6,
  alphaDecay: 0.02,
};

let s = state;
while (s[s.length - 1] > 0.001) {
  s = simTick(s, links, 3, opts);
}
// s now contains converged 3D positions
```

## Real-world Performance

A 2026-10-18 cross-repository test used the real FriendLinks graph: **49,064 nodes,
107,527 edges, 342 cooling ticks**, identical options and initial state, portable
release builds, and 16 Rayon threads on an Intel Core i7-1260P.

| Implementation / call mode | Complete 342 ticks | Process peak RSS |
| --- | --- | --- |
| Published 0.2.3 | 42.99 s | Not recorded in the standalone control |
| 0.3 candidate, individual `tick` calls | 21.07 s | 840.8 MiB |
| 0.3 candidate, batches of 10, two independent runs | 23.02 / 21.61 s | 156.1 / 156.1 MiB |
| 0.3 candidate, one batch of 342 | 22.94 s | 76.9 MiB |

These are complete isolated native-run measurements, not universal throughput
guarantees. The arena and stackless traversal provide the CPU improvement;
batching reduces intermediate JS arrays and peak RSS. The batch-of-10 peak RSS
was 81.4% lower than individual calls in this runner, while CPU times do not show
a batching speedup. Small differences between call modes are sensitive to CPU
scheduling and temperature.

All coordinates remained finite, final alpha and graph geometry summary metrics
matched the published control. The graph, cooling count, theta and force law were
unchanged. Experimental Morton evaluation order had a modest exploratory gain
and remains opt-in. A coherent packet prototype was slower and removed. Local
PGO improved the balanced corpus geometric mean by only 1.1%, below the existing
3% selection threshold, so the unprofiled release was selected on this machine.

The tree remains specialized code in `src/lib.rs`, with no external numerical
tree crate. See [raw measurements and reproduction](reports/build-performance-2026-10-18/README.md),
[algorithm and crate research](scripts/ALGORITHM-RESEARCH.md), and the consumer
fixture runner `scripts/real-graph-benchmark.ts`.

## License

MIT

## Runtime-focused release builds

Native CI artifacts use full LTO, one code-generation unit, and O3, while preserving the existing CPU instruction baseline. CI trains a profile-guided optimization (PGO) candidate using representative graph workloads and publishes it only when the runtime comparison passes; otherwise it publishes the unprofiled release. `bun run build:pgo` produces the optimized addon; `bun run benchmark:pgo` compares it with an unprofiled release. Install `llvm-tools-preview` with the same Rust toolchain first. Details and measurement limits: [scripts/PERFORMANCE.md](scripts/PERFORMANCE.md).
