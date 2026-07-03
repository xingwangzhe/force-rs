[English](./README.md) | [中文](./README_CN.md)

# @xingwangzhe/force-rs

Fast force-directed graph layout powered by Rust + Barnes-Hut N-body simulation, designed as a drop-in replacement for `d3-force-3d` in build-time pipelines.

- **16-core** parallel `simTick`: 47K nodes × 89K edges in **~0.012s/tick**
- **Single-core** auto-fallback to sequential iteration
- **Compatible state format**: `[x, y, z, vx, vy, vz, ...alpha]`

## Installation

```bash
npm install @xingwangzhe/force-rs
```

## API

### simTick(state, links, n, options)

Execute one tick of force-directed simulation.

```ts
import { simTick } from '@xingwangzhe/force-rs';

const opts = {
  repulsion: 3000,      // charge force strength
  linkDistance: 30,     // natural spring length
  centerStrength: 0.01, // center gravity pull
  theta: 0.8,           // Barnes-Hut approximation threshold
  velocityDecay: 0.35,  // velocity damping per tick
  alphaDecay: 0.02,     // cooling rate per tick
};

// state: n*6 + 1 floats [x0,y0,z0,vx0,vy0,vz0, ..., alpha]
// links: 2*m u32 [src0,tgt0, src1,tgt1, ...]
const newState = simTick(state, links, n, opts);
```

**ForceOptions:**
| Field           | Type   | Description                       |
|----------------|--------|-----------------------------------|
| repulsion      | number | Charge force strength             |
| linkDistance   | number | Natural spring length             |
| centerStrength | number | Center gravity pull               |
| theta          | number | Barnes-Hut approximation threshold|
| velocityDecay  | number | Velocity damping per tick         |
| alphaDecay     | number | Cooling rate per tick             |

**Returns:** `Float64Array` — new state (same format as input, with updated alpha).

## Usage Example

```ts
import { simTick } from '@xingwangzhe/force-rs';

const state = new Float64Array([
  0, 0, 0, 0, 0, 0,   // node 0 position + velocity
  1, 1, 0, 0, 0, 0,   // node 1
  1.0                   // alpha
]);
const links = new Uint32Array([0, 1]);

let s = state;
const opts = { repulsion: 3000, linkDistance: 30, centerStrength: 0.01, theta: 0.8, velocityDecay: 0.35, alphaDecay: 0.02 };

while (s[s.length - 1] > 0.001) {
  s = simTick(s, links, 2, opts);
}
```

## Performance

| Platform   | 47K nodes × 89K edges | Notes                          |
|------------|----------------------|--------------------------------|
| 16-core    | **~0.012s/tick**     | Rayon `par_iter` across 16 threads |
| 1-core     | ~0.15s/tick          | auto-fallback to sequential     |

Built with [`zhifeng_impl_barnes_hut_tree`](https://crates.io/crates/zhifeng_impl_barnes_hut_tree) for O(n log n) repulsive force computation.

## License

MIT OR Apache-2.0
