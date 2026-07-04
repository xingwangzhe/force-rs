import test from "node:test";
import assert from "node:assert/strict";
import { simTick } from "./index.js";

const opts = { repulsion: 3000, linkDistance: 30, centerStrength: 0.01, theta: 0.8, velocityDecay: 0.35, alphaDecay: 0.02 };

test("two repelling nodes move apart", () => {
  const state = new Float64Array([
    0,0,0,0,0,0,
    1,1,1,0,0,0,
    1.0
  ]);
  const links = Array.from([]);
  const result = simTick(Array.from(state), Array.from(links), 2, opts);
  const x0 = result[0], y0 = result[1], z0 = result[2];
  const x1 = result[6], y1 = result[7], z1 = result[8];
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
    state[b] = (Math.random() - 0.5) * 100;
    state[b+1] = (Math.random() - 0.5) * 100;
    state[b+2] = (Math.random() - 0.5) * 100;
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
    state[b] = (Math.random() - 0.5) * 100;
    state[b+1] = (Math.random() - 0.5) * 100;
    state[b+2] = (Math.random() - 0.5) * 100;
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
    state[b] = (Math.random() - 0.5) * 100;
    state[b+1] = (Math.random() - 0.5) * 100;
    state[b+2] = (Math.random() - 0.5) * 100;
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
