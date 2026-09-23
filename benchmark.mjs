import { performance } from "node:perf_hooks";
import { createSimulation } from "./index.js";

const n = Number(process.env.FORCE_BENCH_N ?? 2048);
const ticks = Number(process.env.FORCE_BENCH_TICKS ?? 20);
const repetitions = Number(process.env.FORCE_BENCH_REPS ?? 5);
const shape = process.env.FORCE_BENCH_SHAPE ?? "uniform";

let seed = 0x13579bdf;
const random = () => {
  seed = (seed * 1664525 + 1013904223) >>> 0;
  return seed / 0x100000000;
};
const state = new Float64Array(n * 6 + 1);
for (let i = 0; i < n; i++) {
  const b = i * 6;
  const scale = shape === "clustered" ? 40 : shape === "extreme" ? 1e5 : 1000;
  const inHub = shape === "skewed" && i >= n - Math.ceil(n * 0.02);
  state[b] = inHub ? 0 : (random() - 0.5) * scale;
  state[b + 1] = inHub ? 0 : (random() - 0.5) * scale;
  state[b + 2] = inHub ? 0 : (random() - 0.5) * scale;
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
  const options = algorithm ? { ...opts, algorithm } : opts;
  const samplesMs = [];
  let outputType;
  for (let repetition = 0; repetition < repetitions; repetition++) {
    const simulation = createSimulation(links, n);
    let current = new Float64Array(state);
    for (let i = 0; i < 3; i++) current = simulation.tick(current, options);
    const start = performance.now();
    for (let i = 0; i < ticks; i++) current = simulation.tick(current, options);
    samplesMs.push(performance.now() - start);
    outputType = current.constructor.name;
  }
  samplesMs.sort((a, b) => a - b);
  const p50Ms = samplesMs[Math.floor(samplesMs.length / 2)];
  return { algorithm: algorithm ?? "fast", p50Ms, perTickMs: p50Ms / ticks, samplesMs, outputType };
}
console.log(JSON.stringify({
  package: "force-rs",
  n,
  shape,
  edges: links.length / 2,
  ticks,
  repetitions,
  results: [measure(), measure("linear")],
}, null, 2));
