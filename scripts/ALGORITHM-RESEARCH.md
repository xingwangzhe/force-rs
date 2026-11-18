# Barnes–Hut implementation research (2026-10-18)

The consumer benchmark is the actual FriendLinks graph: 8,570 source sites,
49,064 layout nodes, and 107,527 undirected edges. The exported fixture fixes
node order, edge order, the initial state, and the simulation options. A dependency
or algorithm is eligible only if it preserves the existing force model, including
the 1/d displacement kernel, minimum-distance treatment, degree-biased springs,
occupancy-dependent internal-cell strength, and translational center force.
Neither a larger theta nor fewer convergence ticks counts as an implementation
speedup.

## Primary research

- Barnes and Hut, 1986, [A hierarchical O(N log N) force-calculation algorithm](https://doi.org/10.1038/324446a0).
  The current octree approximation remains useful for nonuniform 3D graph layouts.
- Karras, HPG 2012, [Maximizing Parallelism in the Construction of BVHs, Octrees, and k-d Trees](https://research.nvidia.com/publication/2012-06_maximizing-parallelism-construction-bvhs-octrees-and-k-d-trees).
  Morton ordering enables parallel construction; it introduces quantization,
  sorting and a different accumulation order. The existing experimental `linear`
  option retains Morton sorting for comparison.
- [Portable Barnes–Hut implementations, IA3/SC24](https://research-information.bris.ac.uk/files/427731757/barnes_hut_ia3_sc24_v2.pdf).
  Discusses flat trees and stackless DFS traversal. The optimized default follows
  the general locality and subtree-skipping approach while preserving this
  package's insertion order, force law and tree topology.

## crates.io candidates checked

Versions below were verified with the crates.io API on the date above. The
`barnes-hut-tree` and `particular` crate sources were also downloaded and inspected.

| Candidate | Version | Compatibility assessment |
| --- | --- | --- |
| [barnes-hut-tree](https://docs.rs/barnes-hut-tree/0.1.2/barnes_hut_tree/) | 0.1.2 | Good reusable Morton arena and custom force kernel. Rebuild chooses dynamic per-axis bounds and quantization; opening criterion differs. `ForceKernel` receives mass and leaf status, but no occupied-child count, so it cannot reproduce the current internal-cell multiplier. A direct replacement changes the model. |
| [particular](https://docs.rs/particular/0.7.0/particular/) | 0.7.0 | Supports Rayon and GPU N-body methods. Its built-in gravitational interaction differs from the d3-style displacement kernel; custom work is required, and its tree geometry and mass model differ. |
| [ferreus_bbfmm](https://crates.io/crates/ferreus_bbfmm) | 0.2.0 | Kernel-independent black-box FMM with interpolation and low-rank translation. The package's occupancy-scaled cell summaries are not a pairwise kernel, so replacing them with FMM does not preserve the same model. |
| [kifmm](https://github.com/bempp/kifmm) | source reviewed | Kernel-independent FMM, but its Unix-only FFTW/BLAS/LAPACK dependencies do not fit the existing cross-platform npm binaries. |
| [rayon](https://crates.io/crates/rayon) | 1.12.0 | Already the newest release; retained. Thread count and work granularity must be measured on the real graph. |

A generic external tree is not automatically faster than a specialized traversal.
The compatibility failures above exclude direct substitutions before timing:
measuring a different force law would not answer the performance question.

## Compatible implementation changes

The default `fast` path uses an arena rebuilt in original input order, then emits
a preorder walk buffer. Each cell stores its center of mass, pre-scaled charge,
opening threshold and the next cell after its subtree. Accepting a cell jumps to
that next index; rejecting it advances to its first child. This removes per-cell
Box allocations, per-particle traversal-stack allocations, and repeated child
scans. The original `legacy` implementation remains available as a numerical
reference and rollback option.

The prepared simulation retains position and SoA buffers. Spring coefficients
that depend only on the immutable graph are computed once, and predicted node
positions are computed once per tick. Arithmetic evaluation order is preserved
for each force contribution. Parallel centroid reductions retain their existing
floating-point reduction behavior.

Use `scripts/real-graph-benchmark.ts` for a consumer-exported JSON fixture. Set
`FORCE_BINDING_PATH` to a preserved baseline or candidate addon, and control
`RAYON_NUM_THREADS`, `FORCE_BENCH_TICKS`, `FORCE_BENCH_REPS`, and `FORCE_ALGORITHM`.
Benchmark processes must run without a competing compiler or full consumer build.

## Rejected measured experiment

A four-query coherent packet traversal was prototyped and differential-tested. It
shared the preorder walk with an active-lane mask, retaining every lane's opening
criterion and addition order. The real fixture's initial-state ABBA comparison
was about 19% slower on average; the late-state comparison was about 6% slower.
The extra scalar mask/restore work outweighed shared node loads on this CPU, so
the prototype and public option were removed. This was not promoted on the
strength of an algorithmic or vectorization claim alone.

The contiguous builder now tracks occupied children during insertion and combines
center-of-mass finalization, charge precomputation and preorder emission into one
pass. Bounded `Simulation.run` batches use two reusable native output buffers;
intermediate tick results need not become separate JS external arrays.
