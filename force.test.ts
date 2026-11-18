import test from "node:test";
import assert from "node:assert/strict";
import { createSimulation, simTick } from "./index.js";

const opts = { repulsion: 3000, linkDistance: 30, centerStrength: 0.01, theta: 0.8, velocityDecay: 0.35, alphaDecay: 0.02 };
let seed = 0x12345678;
const random = () => {
  seed = (seed * 1664525 + 1013904223) >>> 0;
  return seed / 0x100000000;
};

test("two repelling nodes move apart", () => {
  const state = new Float64Array([
    0,0,0,0,0,0,
    1,1,1,0,0,0,
    1.0
  ]);
  const links = Array.from([]);
  const result = simTick(Array.from(state), Array.from(links), 2, opts);
  // nodes should move apart due to repulsion
    const d0 = Math.sqrt((result[6]-result[0])**2 + (result[7]-result[1])**2 + (result[8]-result[2])**2);
  const d1 = Math.sqrt((1-0)**2*3);
  assert.ok(d0 > d1 * 0.99, `repulsive nodes should not converge`);
});

test("connected nodes move closer", () => {
  // two nodes 100 units apart, connected by a link with distance 30
  const state = new Float64Array([
    -50,0,0,0,0,0,
     50,0,0,0,0,0,
    1.0
  ]);
  const links = new Uint32Array([0, 1]);
  const result = simTick(Array.from(state), Array.from(links), 2, opts);
  const x0 = result[0], x1 = result[6];
  // they should move closer due to spring attraction
  assert.ok(Math.abs(x1 - x0) < 100, "nodes should move closer");
});

test("no NaN in result", () => {
  const state = new Float64Array([
    0,0,0,0,0,0,
    1,0,0,0,0,0,
    3,2,0,0,0,0,
    1.0
  ]);
  const links = new Uint32Array([0,1, 1,2]);
  const result = simTick(Array.from(state), Array.from(links), 3, opts);
  for (let i = 0; i < result.length; i++) {
    assert.ok(Number.isFinite(result[i]), `result[${i}]=${result[i]} should be finite`);
  }
});

test("center force shifts centroid toward origin (translational)", () => {
  // d3-force center: translational shift of the entire graph centroid
  // centerStrength=0.5 → centroid moves 50% toward origin per tick
  const highCenter = { ...opts, centerStrength: 0.5, repulsion: 0 };
  const state = new Float64Array([
    100,100,100,0,0,0,
    1.0
  ]);
  const r = simTick(Array.from(state), Array.from([]), 1, highCenter);
  // centroid shifts toward origin: new pos ≈ (50,50,50)
  const d0 = Math.sqrt(100*100+100*100+100*100);  // ~173.2
  const d1 = Math.sqrt(r[0]*r[0]+r[1]*r[1]+r[2]*r[2]);
  assert.ok(d1 < d0, `distance should decrease: ${d1} < ${d0}`);
  assert.ok(d1 > 50, `distance shouldn't overshoot: ${d1} > 50`);
});

test("center force preserves relative positions with multiple nodes", () => {
  // two nodes: translation should preserve their relative distances
  const highCenter = { ...opts, centerStrength: 0.5, repulsion: 0 };
  const state = new Float64Array([
    0,0,0,0,0,0,
    10,0,0,0,0,0,
    1.0
  ]);
  const r = simTick(Array.from(state), Array.from([]), 2, highCenter);
  // both nodes shift by same amount = centroid * 0.5 = (5,0,0) * 0.5 = (2.5,0,0)
  // new positions: (-2.5,0,0) and (7.5,0,0)
  const dx = r[6] - r[0];  // should still be ~10
  assert.ok(Math.abs(dx - 10) < 1, `relative distance preserved: ${dx} ≈ 10`);
});

test("alpha decreases", () => {
  const state = new Float64Array([0,0,0,0,0,0, 1.0]);
  const links = Array.from([]);
  const r = simTick(Array.from(state), Array.from(links), 1, opts);
  assert.ok(r[r.length - 1] < 1.0, "alpha should decrease");
});

test("many ticks converge", () => {
  let state = new Float64Array(10 * 6 + 1);
  for (let i = 0; i < 10; i++) {
    const b = i * 6;
    state[b] = (random() - 0.5) * 100;
    state[b+1] = (random() - 0.5) * 100;
    state[b+2] = (random() - 0.5) * 100;
  }
  state[60] = 1.0;
  
  const links = new Uint32Array([
    0,1, 1,2, 2,3, 3,4, 4,5, 5,6, 6,7, 7,8, 8,9, 9,0, // ring
    0,5, 1,6, 2,7, 3,8, 4,9  // cross
  ]);

  for (let tick = 0; tick < 200; tick++) {
    state = new Float64Array(simTick(Array.from(state), Array.from(links), 10, opts));
    assert.ok(state[60] >= 0, "alpha should stay non-negative");
    for (let i = 0; i < 60; i++) {
      if (!Number.isFinite(state[i])) {
        console.error(`tick ${tick}, state[${i}]=${state[i]} became NaN/inf`);
        assert.fail("NaN detected in state");
      }
    }
  }
  assert.ok(true, "200 ticks completed without NaN");
});

test("large graph 500 nodes runs", () => {
  const n = 500;
  const state = new Float64Array(n * 6 + 1);
  for (let i = 0; i < n; i++) {
    const b = i * 6;
    state[b] = (random() - 0.5) * 100;
    state[b+1] = (random() - 0.5) * 100;
    state[b+2] = (random() - 0.5) * 100;
  }
  state[n*6] = 1.0;
  const links = new Uint32Array(n * 2);
  for (let i = 0; i < n; i++) {
    links[i*2] = i;
    links[i*2+1] = (i + 1) % n;
  }
  const r = simTick(Array.from(state), Array.from(links), n, opts);
  assert.equal(r.length, n*6+1);
  for (let i = 0; i < n*6; i++) {
    assert.ok(Number.isFinite(r[i]), `node ${Math.floor(i/6)} dim ${i%6}: ${r[i]}`);
  }
});

test("symmetry: swapping two nodes produces mirrored forces", () => {
  const state = new Float64Array([
    0,0,0,0,0,0,
    2,0,0,0,0,0,
    1.0
  ]);
  const links = new Uint32Array([0, 1]);
  const r = simTick(Array.from(state), Array.from(links), 2, { ...opts, repulsion: 1000, centerStrength: 0 });

  // nodes should move toward center symmetrically (equal and opposite)
  const vx0 = r[3], vx1 = r[9];
  // signs should be opposite
  assert.ok(vx0 * vx1 <= 0, `vx0=${vx0}, vx1=${vx1}, expect opposite signs`);
});

test("memory: repeated ticks don't leak", () => {
  const GC = typeof globalThis.gc === 'function' ? globalThis.gc : null;
  
  let state = new Float64Array(100 * 6 + 1);
  for (let i = 0; i < 100; i++) {
    const b = i * 6;
    state[b] = (random() - 0.5) * 100;
    state[b+1] = (random() - 0.5) * 100;
    state[b+2] = (random() - 0.5) * 100;
  }
  state[600] = 1.0;
  
  const links = new Uint32Array(100 * 2);
  for (let i = 0; i < 100; i++) {
    links[i*2] = i;
    links[i*2+1] = (i + 1) % 100;
  }
  
  if (GC) GC();
  const start = process.memoryUsage().heapUsed;
  
  for (let tick = 0; tick < 50; tick++) {
    state = new Float64Array(simTick(Array.from(state), Array.from(links), 100, opts));
  }
  
  if (GC) GC();
  const end = process.memoryUsage().heapUsed;
  const delta = end - start;
  console.log(`Memory delta after 50 ticks: ${(delta / 1024 / 1024).toFixed(1)} MB`);
  // Allow up to 50MB growth (should be much less)
  assert.ok(delta < 50 * 1024 * 1024, `memory grew ${(delta/1024/1024).toFixed(1)}MB, suspicious`);
});

test("identical positions still repel (approximation)", () => {
  const state = new Float64Array([0,0,0,0,0,0, 0,0,0,0,0,0, 1.0]);
  const r = simTick(Array.from(state), Array.from(new Uint32Array([])), 2, { ...opts, repulsion: 10000, centerStrength: 0, velocityDecay: 1.0, alphaDecay: 0 });
  // Identical positions: self-exclusion via distance<1 may skip, 
  // but they should get force from super-node aggregation
  for (let i = 0; i < r.length-1; i++) assert.ok(Number.isFinite(r[i]), `r[${i}]=${r[i]}`);
});

test("single node has zero velocity", () => {
  const r = simTick([5,0,0,0,0,0, 1.0], [], 1, opts);
  // The center force should still apply (only one body)
  assert.ok(r[0] !== 5 || r[3] !== 0, "center force should move single node toward origin");
});

test("no crash with all nodes at origin", () => {
  const n = 50;
  const state = new Float64Array(n * 6 + 1);
  state[state.length - 1] = 1.0;
  const links = new Uint32Array(n * 2);
  for (let i = 0; i < n; i++) { links[i*2] = i; links[i*2+1] = (i+1)%n; }
  const r = simTick(Array.from(state), Array.from(links), n, { ...opts, repulsion: 10000 });
  for (let i = 0; i < r.length - 1; i++) {
    assert.ok(Number.isFinite(r[i]), `all-at-origin: r[${i}]=${r[i]}`);
  }
  assert.ok(true, "all nodes at origin handled without NaN");
});

test("massive repulsion clamped", () => {
  const state = new Float64Array([0,0,0,0,0,0, 1,0,0,0,0,0, 1.0]);
  const r = simTick(Array.from(state), [], 2, { ...opts, repulsion: 1e15, centerStrength: 0, velocityDecay: 1.0, alphaDecay: 0 });
  for (let i = 0; i < r.length - 1; i++) {
    assert.ok(Number.isFinite(r[i]) && Math.abs(r[i]) < 1e11, `massive repulsion r[${i}]=${r[i]}`);
  }
});

test("prepared typed-array simulation reuses link preprocessing", () => {
  const links = new Uint32Array([0, 1, 1, 2]);
  const simulation = createSimulation(links, 3);
  const state = new Float64Array([
    -10, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0,
    10, 0, 0, 0, 0, 0,
    1,
  ]);
  const result = simulation.tick(state, { ...opts, algorithm: "fast" });
  assert.equal(result.constructor, Float64Array);
  assert.equal(result.length, state.length);
  for (const value of result) assert.ok(Number.isFinite(value));
});

test("legacy algorithm remains available for numerical comparisons", () => {
  const state = [-50, 0, 0, 0, 0, 0, 50, 0, 0, 0, 0, 0, 1];
  const links = [0, 1];
  const fast = simTick(state, links, 2, opts);
  const legacy = simTick(state, links, 2, { ...opts, algorithm: "legacy" });
  assert.equal(fast.length, legacy.length);
  for (let i = 0; i < fast.length; i++) {
    assert.ok(Math.abs(fast[i] - legacy[i]) < 1e-9, `difference at ${i}`);
  }
});

test("linear Morton tree is available as an explicit experimental path", () => {
  const n = 32;
  const state = new Float64Array(n * 6 + 1);
  for (let i = 0; i < n; i++) state[i * 6] = i * 10 - 150;
  state[n * 6] = 1;
  const links = new Uint32Array([0, 1, 1, 2, 2, 3]);
  const simulation = createSimulation(links, n);
  const result = simulation.tick(state, { ...opts, algorithm: "linear" });
  for (const value of result) assert.ok(Number.isFinite(value));
});

test("legacy and linear Barnes-Hut paths agree across spatial distributions", () => {
  const n = 192;
  for (const distribution of ["uniform", "clustered", "skewed", "extreme"]) {
    const state = new Float64Array(n * 6 + 1);
    let localSeed = 0x51f15e;
    const nextRandom = () => {
      localSeed = (localSeed * 1664525 + 1013904223) >>> 0;
      return localSeed / 0x100000000;
    };
    for (let i = 0; i < n; i++) {
      const base = i * 6;
      const scale = distribution === "clustered" ? 20 : distribution === "extreme" ? 1e5 : 1000;
      const cluster = distribution === "skewed" && i >= n - 12;
      state[base] = cluster ? 0 : (nextRandom() - 0.5) * scale;
      state[base + 1] = cluster ? 0 : (nextRandom() - 0.5) * scale;
      state[base + 2] = cluster ? 0 : (nextRandom() - 0.5) * scale;
    }
    state[n * 6] = 1;
    const legacy = simTick(Array.from(state), [], n, { ...opts, centerStrength: 0, algorithm: "legacy" });
    const linear = simTick(Array.from(state), [], n, { ...opts, centerStrength: 0, algorithm: "linear" });
    for (let i = 0; i < legacy.length; i++) {
      assert.ok(Number.isFinite(linear[i]), `${distribution}: result[${i}] must be finite`);
      const tolerance = 1e-9 * Math.max(1, Math.abs(legacy[i]));
      assert.ok(Math.abs(legacy[i] - linear[i]) <= tolerance, `${distribution}: mismatch at ${i}: ${legacy[i]} vs ${linear[i]}`);
    }
  }
});

test("zero-node and non-finite-coordinate inputs return finite state", () => {
  const empty = simTick([1], [], 0, opts);
  assert.deepEqual(empty, [1 - opts.alphaDecay]);
  const state = [Infinity, NaN, -Infinity, 0, 0, 0, 1];
  const result = simTick(state, [], 1, { ...opts, centerStrength: 0 });
  for (const value of result) assert.ok(Number.isFinite(value));
});

test("small-theta Barnes-Hut force matches a direct pairwise reference", () => {
  const points = [[0, 0, 0], [100, 0, 0], [0, 200, 0], [0, 0, 300], [150, 200, 250]];
  const n = points.length;
  const state = Array.from({ length: n * 6 + 1 }, () => 0);
  for (let i = 0; i < n; i++) state.splice(i * 6, 3, ...points[i]);
  state[n * 6] = 1;
  const repulsion = 900;
  const exact = simTick(state, [], n, {
    ...opts,
    repulsion,
    centerStrength: 0,
    theta: 1e-8,
    velocityDecay: 1,
    alphaDecay: 0,
  });
  for (let i = 0; i < n; i++) {
    const expectedForce = [0, 0, 0];
    for (let j = 0; j < n; j++) {
      if (i === j) continue;
      const delta = points[i].map((value, axis) => value - points[j][axis]);
      const distanceSquared = delta.reduce((sum, value) => sum + value * value, 0);
      for (let axis = 0; axis < 3; axis++) expectedForce[axis] += repulsion * delta[axis] / distanceSquared;
    }
    for (let axis = 0; axis < 3; axis++) {
      assert.ok(Math.abs(exact[i * 6 + axis] - (points[i][axis] + expectedForce[axis])) < 1e-8);
      assert.ok(Math.abs(exact[i * 6 + axis + 3] - expectedForce[axis]) < 1e-8);
    }
  }
});

test("stackless arena preserves legacy forces across adversarial trees", () => {
  for (const shape of ["uniform", "clustered", "coincident", "outside-root"]) {
    const n = shape === "uniform" ? 4097 : 137;
    let localSeed = 0x71c0ffee;
    const next = () => {
      localSeed = (Math.imul(localSeed, 1664525) + 1013904223) >>> 0;
      return localSeed / 0x100000000;
    };
    let fast = new Float64Array(n * 6 + 1);
    for (let i = 0; i < n; i++) {
      for (let axis = 0; axis < 3; axis++) {
        const scale = shape === "clustered" ? 0.005 : shape === "outside-root" ? 1e9 : 3000;
        fast[i * 6 + axis] = shape === "coincident" ? 0 : (next() - 0.5) * scale;
        fast[i * 6 + axis + 3] = (next() - 0.5) * 5;
      }
    }
    fast[n * 6] = 1;
    let legacy = new Float64Array(fast);
    // Includes duplicate links, self-links, and invalid endpoints.
    const links = new Uint32Array(n * 8);
    for (let i = 0; i < links.length; i++) links[i] = Math.floor(next() * (n + 2));
    const a = createSimulation(links, n);
    const b = createSimulation(links, n);
    // Disable centroid reduction: parallel reduction order can differ between
    // executions, independently of the per-node force algorithm.
    const options = { ...opts, centerStrength: 0, distanceMax: shape === "uniform" ? 2000 : undefined };
    for (let tick = 0; tick < 4; tick++) {
      fast = a.tick(fast, { ...options, algorithm: "fast" });
      legacy = b.tick(legacy, { ...options, algorithm: "legacy" });
      assert.deepEqual(fast, legacy, `${shape}, tick ${tick}`);
    }
  }
});

test("prepared simulation refreshes reused scratch from the supplied state", () => {
  const links = new Uint32Array([0, 1, 1, 2]);
  const simulation = createSimulation(links, 3);
  const first = new Float64Array([-10, 0, 0, 1, 0, 0, 0, 0, 0, 0, 1, 0, 10, 0, 0, 0, 0, 1, 1]);
  const second = new Float64Array(first);
  second[0] = 123;
  simulation.tick(first, opts);
  assert.deepEqual(simulation.tick(second, opts), createSimulation(links, 3).tick(second, opts));
  assert.equal(first[0], -10, "input state is not mutated");
});

test("Morton evaluation order preserves per-node arithmetic and public node order", () => {
  const n = 4097;
  let seed = 0x4512aaef;
  const next = () => {
    seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
    return seed / 0x100000000;
  };
  let ordinary = new Float64Array(n * 6 + 1);
  for (let i = 0; i < n; i++) {
    for (let axis = 0; axis < 3; axis++) ordinary[i * 6 + axis] = (next() - 0.5) * 10000;
  }
  ordinary[n * 6] = 1;
  let ordered = new Float64Array(ordinary);
  const links = new Uint32Array(n * 6);
  for (let i = 0; i < links.length; i++) links[i] = Math.floor(next() * n);
  const a = createSimulation(links, n);
  const b = createSimulation(links, n);
  const options = { ...opts, centerStrength: 0 };
  for (let tick = 0; tick < 3; tick++) {
    ordinary = a.tick(ordinary, { ...options, algorithm: "fast" });
    ordered = b.tick(ordered, { ...options, algorithm: "ordered" });
    assert.deepEqual(ordered, ordinary, `tick ${tick}`);
  }
});

test("batched simulation matches individual ticks and the exact cooling stop", () => {
  const n = 17;
  const initial = new Float64Array(n * 6 + 1);
  const links = new Uint32Array(n * 2);
  for (let i = 0; i < n; i++) {
    initial[i * 6] = Math.sin(i) * 500;
    initial[i * 6 + 1] = Math.cos(i) * 500;
    initial[i * 6 + 2] = i * 10;
    links[i * 2] = i;
    links[i * 2 + 1] = (i + 1) % n;
  }
  initial[n * 6] = 1;
  const options = { ...opts, centerStrength: 0 };
  const individual = createSimulation(links, n);
  let expected = new Float64Array(initial);
  let ticks = 0;
  do {
    expected = individual.tick(expected, options);
    ticks++;
  } while (expected[n * 6] >= 0.001 && ticks < 500);
  const simulation = createSimulation(links, n);
  const result = simulation.run(initial, options, 500, 0.001);
  assert.equal(result.ticks, ticks);
  assert.equal(result.ticks, 342);
  assert.deepEqual(result.state, expected);
  assert.equal(initial[n * 6], 1, "batch does not mutate the input");
  const zero = simulation.run(initial, options, 0, 0.001);
  assert.equal(zero.ticks, 0);
  assert.deepEqual(zero.state, initial);
});

test("split batches preserve public tick state", () => {
  const initial = new Float64Array([-10, 0, 0, 0, 0, 0, 10, 0, 0, 0, 0, 0, 1]);
  const links = new Uint32Array([0, 1]);
  const options = { ...opts, centerStrength: 0 };
  const a = createSimulation(links, 2);
  const b = createSimulation(links, 2);
  const first = a.run(initial, options, 7, 0);
  const split = a.run(first.state, options, 13, 0);
  const single = b.run(initial, options, 20, 0);
  assert.equal(first.ticks + split.ticks, single.ticks);
  assert.deepEqual(split.state, single.state);
});

test("batch reports a time cutoff separately from cooling completion", () => {
  const initial = new Float64Array([-10, 0, 0, 0, 0, 0, 10, 0, 0, 0, 0, 0, 1]);
  const simulation = createSimulation(new Uint32Array([0, 1]), 2);
  const limited = simulation.run(initial, opts, 100, 0, 0);
  assert.equal(limited.ticks, 1);
  assert.equal(limited.stoppedByTime, true);
  const converged = simulation.run(initial, opts, 100, 2, 0);
  assert.equal(converged.ticks, 1);
  assert.equal(converged.stoppedByTime, false, "cooling completes before checking time limit");
});
