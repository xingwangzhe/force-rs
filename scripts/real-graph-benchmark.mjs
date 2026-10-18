// Benchmark a consumer-exported fixture rather than a synthetic graph.
// Example: RAYON_NUM_THREADS=4 bun scripts/real-graph-benchmark.mjs fixture.json
import { readFileSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { performance } from "node:perf_hooks";
import { resolve } from "node:path";

const fixturePath = process.argv[2];
if (!fixturePath) throw new Error("Pass a fixture JSON exported by the consuming project");
const bindingPath = process.env.FORCE_BINDING_PATH;
const native = createRequire(import.meta.url)(bindingPath ? resolve(bindingPath) : "../index.js");
const fixture = JSON.parse(readFileSync(fixturePath, "utf8"));
let initial = Float64Array.from(fixture.initialState);
if (process.env.FORCE_BENCH_INPUT_STATE) {
  const buffer = readFileSync(process.env.FORCE_BENCH_INPUT_STATE);
  initial = new Float64Array(buffer.buffer.slice(buffer.byteOffset, buffer.byteOffset + buffer.byteLength));
  if (initial.length !== fixture.nodeCount * 6 + 1) throw new Error("State length does not match fixture");
}
const links = Uint32Array.from(fixture.linksFlat);
const ticks = Number(process.env.FORCE_BENCH_TICKS ?? 10);
const repetitions = Number(process.env.FORCE_BENCH_REPS ?? 3);
const batchSize = Number(process.env.FORCE_BATCH_SIZE ?? 1);
const algorithm = process.env.FORCE_ALGORITHM ?? "fast";
const options = { ...fixture.options, algorithm };
const samples = [];
const firstTickMs = [];
const blockSamples = [];
const warmup = Number(process.env.FORCE_BENCH_WARMUP ?? 1);
let finalState;
let checksum = 0;
for (let repetition = -warmup; repetition < repetitions; repetition++) {
  const simulation = native.createSimulation(links, fixture.nodeCount);
  let state = new Float64Array(initial);
  const start = performance.now();
  let firstTick = 0;
  let blockStart = start;
  const blocks = [];
  for (let tick = 0; tick < ticks;) {
    const batch = Math.min(batchSize, ticks - tick);
    state = batch > 1 ? simulation.run(state, options, batch, 0).state : simulation.tick(state, options);
    tick += batch;
    if (tick === batch) firstTick = performance.now() - start;
    if (tick % 50 === 0 || tick === ticks) {
      const now = performance.now();
      blocks.push({ throughTick: tick, elapsedMs: now - blockStart });
      blockStart = now;
    }
    if (repetition === repetitions - 1 && tick === 300 && process.env.FORCE_BENCH_SAVE_LATE_STATE) {
      writeFileSync(process.env.FORCE_BENCH_SAVE_LATE_STATE, new Uint8Array(state.buffer, state.byteOffset, state.byteLength));
    }
  }
  const elapsed = performance.now() - start;
  if (repetition >= 0) {
    samples.push(elapsed);
    firstTickMs.push(firstTick);
    blockSamples.push(blocks);
  }
  finalState = state;
  for (let i = 0; i < state.length; i += 997) checksum += state[i];
}
if (process.env.FORCE_BENCH_SAVE_STATE) {
  writeFileSync(process.env.FORCE_BENCH_SAVE_STATE, new Uint8Array(finalState.buffer, finalState.byteOffset, finalState.byteLength));
}
const centroid = [0, 0, 0];
let radiusSquaredSum = 0;
let finiteValues = 0;
for (let i = 0; i < fixture.nodeCount; i++) {
  for (let axis = 0; axis < 3; axis++) {
    const value = finalState[i * 6 + axis];
    if (Number.isFinite(value)) finiteValues++;
    centroid[axis] += value / fixture.nodeCount;
    radiusSquaredSum += value * value;
  }
}
let edgeLengthSum = 0;
let edgeErrorSquaredSum = 0;
for (let edge = 0; edge < links.length; edge += 2) {
  const a = links[edge] * 6;
  const b = links[edge + 1] * 6;
  const distance = Math.hypot(finalState[a] - finalState[b], finalState[a + 1] - finalState[b + 1], finalState[a + 2] - finalState[b + 2]);
  edgeLengthSum += distance;
  edgeErrorSquaredSum += (distance - options.linkDistance) ** 2;
}
const ordered = [...samples].sort((a, b) => a - b);
console.log(JSON.stringify({
  fixture: resolve(fixturePath),
  binding: bindingPath ?? "package default",
  threads: process.env.RAYON_NUM_THREADS ?? "automatic",
  nodes: fixture.nodeCount,
  edges: fixture.edgeCount,
  algorithm,
  ticks,
  batchSize,
  samplesMs: samples,
  firstTickMs,
  blockSamples,
  quality: {
    finiteCoordinates: finiteValues,
    centroid,
    rmsRadius: Math.sqrt(radiusSquaredSum / fixture.nodeCount),
    meanEdgeLength: edgeLengthSum / fixture.edgeCount,
    rmsEdgeDistanceError: Math.sqrt(edgeErrorSquaredSum / fixture.edgeCount),
    finalAlpha: finalState[fixture.nodeCount * 6],
  },
  medianMs: ordered[Math.floor(ordered.length / 2)],
  checksum,
  maxRSSKilobytes: process.resourceUsage().maxRSS,
}));
