import { createRequire } from "node:module";
import { performance } from "node:perf_hooks";
const native = createRequire(import.meta.url)(process.env.PGO_BINDING_PATH);
const training = process.argv[2] === "train";
const results = {};
let sink = 0;
async function measure(name, fn, iterations = 1) {
  for (let i = 0; i < 2; i++) await fn();
  if (!training) {
    const started = performance.now();
    for (let i = 0; i < iterations; i++) await fn();
    const elapsed = Math.max(performance.now() - started, 0.001);
    iterations = Math.max(iterations, Math.ceil((iterations * 30) / elapsed));
  }
  const samples = [];
  for (let sample = 0; sample < (training ? 2 : 7); sample++) {
    const started = performance.now();
    for (let i = 0; i < iterations; i++) await fn();
    samples.push((performance.now() - started) / iterations);
  }
  results[name] = samples;
}
for (const shape of ["uniform", "clustered", "skewed", "extreme", "phyllotaxis"]) {
  const n = shape === "phyllotaxis" ? (training ? 8192 : 10240) : (training ? 1536 : 2048);
  let seed = training ? 0x13579bdf : 0x2468ace;
  const random = () => {
    seed = (seed * 1664525 + 1013904223) >>> 0;
    return seed / 0x100000000;
  };
  const initial = new Float64Array(n * 6 + 1);
  for (let i = 0; i < n; i++) {
    const scale = shape === "clustered" ? 40 : shape === "extreme" ? 1e5 : 1000;
    if (shape === "phyllotaxis") {
      const radius = 10 * Math.cbrt(0.5 + i);
      const roll = i * Math.PI * (3 - Math.sqrt(5));
      const yaw = i * Math.PI * 20 / (9 + Math.sqrt(221));
      initial[i * 6] = radius * Math.sin(roll) * Math.cos(yaw);
      initial[i * 6 + 1] = radius * Math.cos(roll);
      initial[i * 6 + 2] = radius * Math.sin(roll) * Math.sin(yaw);
    } else {
      for (let j = 0; j < 3; j++)
        initial[i * 6 + j] = shape === "skewed" && i > n * 0.98 ? 0 : (random() - 0.5) * scale;
    }
  }
  initial[n * 6] = 1;
  const links = new Uint32Array(n * (shape === "phyllotaxis" ? 4 : 8));
  for (let i = 0; i < links.length; i++) links[i] = Math.floor(random() * n);
  for (const algorithm of ["fast", "linear", "ordered"]) {
    const simulation = native.createSimulation(links, n);
    const opts = {
      repulsion: shape === "phyllotaxis" ? 30000 : 3000,
      linkDistance: shape === "phyllotaxis" ? 500 : 30,
      centerStrength: shape === "phyllotaxis" ? 0.015 : 0.01,
      theta: 0.8,
      velocityDecay: shape === "phyllotaxis" ? 0.1 : 0.35,
      alphaDecay: 0.02,
      algorithm,
    };
    await measure(`${shape}.${algorithm}`, () => {
      let state = new Float64Array(initial);
      for (let tick = 0; tick < 5; tick++) state = simulation.tick(state, opts);
      sink += state[n * 6];
    });
    if (training) {
      simulation.tick(initial, { ...opts, theta: 0.4, distanceMax: 100 });
      native.simTick(Array.from(initial), Array.from(links), n, opts);
      simulation.run(initial, opts, shape === "phyllotaxis" ? 50 : 3, 0);
      simulation.run(initial, opts, 3, 0, 0);
    }
  }
}
if (!Number.isFinite(sink)) throw new Error("Non-finite workload output");
if (training) native.flushPgoProfile();
console.log(JSON.stringify(results));
