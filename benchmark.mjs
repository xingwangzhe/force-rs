import { performance } from "node:perf_hooks";
import { createSimulation } from "./index.js";

const n = Number(process.env.FORCE_BENCH_N ?? 2048);
const ticks = Number(process.env.FORCE_BENCH_TICKS ?? 20);

let seed = 0x13579bdf;
const random = () => {
  seed = (seed * 1664525 + 1013904223) >>> 0;
  return seed / 0x100000000;
};
const state = new Float64Array(n * 6 + 1);
for (let i = 0; i < n; i++) {
  const b = i * 6;
  state[b] = (random() - 0.5) * 1000;
  state[b + 1] = (random() - 0.5) * 1000;
  state[b + 2] = (random() - 0.5) * 1000;
}
state[n * 6] = 1;
const links = new Uint32Array((n * 4) * 2);
for (let i = 0; i < links.length; i += 2) {
  links[i] = Math.floor(random() * n);
  links[i + 1] = Math.floor(random() * n);
}
const opts = {
  repulsion: 3000,
  linkDistance: 30,
  centerStrength: 0.01,
  theta: 0.8,
  velocityDecay: 0.35,
  alphaDecay: 0.02,
};

function measure(algorithm) {
  const simulation = createSimulation(links, n);
  let current = state;
  const options = algorithm ? { ...opts, algorithm } : opts;
  for (let i = 0; i < 3; i++) current = simulation.tick(current, options);
  const start = performance.now();
  for (let i = 0; i < ticks; i++) current = simulation.tick(current, options);
  const elapsedMs = performance.now() - start;
  return { algorithm: algorithm ?? "fast", totalMs: elapsedMs, perTickMs: elapsedMs / ticks, outputType: current.constructor.name };
}
console.log(JSON.stringify({
  package: "force-rs",
  n,
  edges: links.length / 2,
  ticks,
  results: [measure(), measure("linear")],
}, null, 2));
