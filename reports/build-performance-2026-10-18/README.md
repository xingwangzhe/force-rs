# Force layout cross-repository benchmark, 2026-10-18

This report uses a frozen export of the actual FriendLinks graph: 8,570 source
sites, 49,064 layout nodes and 107,527 edges. Node and edge order, initial state,
force parameters and all 342 cooling ticks remain fixed. The published control
is the npm 0.2.3 Linux x64 GNU addon, not a potentially different local build.
`metadata.json` records both binary hashes, hardware, options and caveats.

## Final measurements

Each row ran in a separate process with 16 Rayon threads and no competing
compiler or benchmark. All final rows use the selected portable, unprofiled 0.3
candidate. Default `fast` remains the specialized Barnes–Hut implementation.

| Call mode | Complete 342 ticks | Process peak RSS (KiB) |
| --- | --- | --- |
| Individual tick calls | 21,071 ms | 861,000 |
| Batches of 10, run 1 | 23,017 ms | 159,816 |
| Batches of 10, run 2 | 21,609 ms | 159,880 |
| One batch of 342 | 22,940 ms | 78,776 |

The published 0.2.3 control took 42,989 ms in the earlier isolated full run.
These standalone native timings are distinct from a complete Astro build.
Batching reduced the runner's peak RSS by 81.4% at size 10; these measurements
show no CPU-time benefit from batching. CPU scheduling and temperature affect
small differences. `run` also returns completed tick count and a time-cutoff flag
so consumers can distinguish incomplete layout from cooling completion.

All final runs produce 147,192 finite position coordinates and alpha
`0.0009984306091742447`. RMS radius is `62939.72345772942`, mean edge length is
`22040.86313754161`, and RMS edge-distance error is `26437.16674461348`, matching
the published control to ordinary floating-point precision. Tiny differences
from parallel centroid reductions are summarized in the final differential file.

## Experiments and selection

- Reusable contiguous cells, fused finalization, precomputed charge/opening tests,
  and stackless traversal replace pointer allocation and per-query stacks.
- Immutable spring coefficients and per-tick predicted positions are reused.
- Morton evaluation order preserves the model but showed a modest, noisy gain;
  it remains an explicit experimental option.
- A four-query coherent walk passed differential tests but was slower: about
  19% on the initial-state comparison and 6% on the late-state comparison. It
  was removed rather than shipped.
- PGO was trained on synthetic shapes including medium phyllotaxis layouts,
  then compared using the existing ABBAAB, 21-sample-per-workload selection
  procedure. Its geometric mean speedup was `1.0110286951`, below `1.03`, so the
  unprofiled binary was selected. The actual graph also remained a separate
  holdout; its noisy short-run results did not justify overriding that decision.

The first single-thread control in the exploratory thread scan overlapped an
unrelated compile. It is explicitly excluded from clean evidence; use the later
balanced comparisons and complete runs. No target-cpu=native or fast-math flags
were used. Versions and primary research sources are documented in
[the algorithm assessment](../../scripts/ALGORITHM-RESEARCH.md).

## Reproduction

The fixture must contain `nodeCount`, `edgeCount`, `linksFlat`, `initialState` and
`options`. `initialState` contains `nodeCount * 6 + 1` doubles. This report does
not commit the consumer's full graph or coordinates.

```bash
RAYON_NUM_THREADS=16 \
FORCE_BENCH_TICKS=342 \
FORCE_BENCH_REPS=1 \
FORCE_BENCH_WARMUP=0 \
FORCE_BATCH_SIZE=10 \
bun scripts/real-graph-benchmark.ts /path/to/fixture.json
```

`FORCE_BINDING_PATH` selects a preserved native addon. `FORCE_ALGORITHM` selects
an algorithm. `FORCE_BENCH_INPUT_STATE` accepts a binary Float64Array for a late
phase; `FORCE_BENCH_SAVE_STATE` and `FORCE_BENCH_SAVE_LATE_STATE` save final and
tick-300 states. The legacy `firstTickMs` field measures the first native call,
so it covers `batchSize` ticks when batching is enabled.

Raw JSON files retain exploratory and final timings, PGO selection, geometry
summaries and exact binary provenance. They contain counts and summaries, not
full graph data or coordinate fixtures.
