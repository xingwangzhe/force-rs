[English](./README.md) | [中文](./README_CN.md)

# @xingwangzhe/force-rs

基于 **Rust + 连续内存 Barnes-Hut 八叉树** 的快速 3D 力导向图布局，设计用于在构建期替代 `d3-force-3d`。

### 为什么需要这个包

`d3-force-3d` 在交互式可视化中表现优异，但在构建期（SSG/SSR）处理 47K+ 节点、89K+ 边时，JS 运行时成为瓶颈 — 数百个 tick、每个约 1s 太慢了。`force-rs` 用 Rust 实现基于 d3 的斥力、度数偏置弹簧力和质心平移。实际耗时取决于图形分布、参数、硬件与线程数；下方提供真实消费项目的完整测量。

### 力模型

| 力                   | 实现方式                                                   |
| -------------------- | ---------------------------------------------------------- |
| 多体斥力 (many-body) | Barnes-Hut 八叉树，O(n log n)，含 `√(4/k)` 修正因子        |
| 弹簧力 (link)        | 基于度数的偏置强度：`1/min(deg, deg)`，bias = 对方度数占比 |
| 中心力 (center)      | 质心向原点平移（与 d3-force `forceCenter` 一致）           |
| 近距离软化           | `√(d_min² × d²)` 平滑过渡（与 d3-force 一致）              |

### 其他特性

- **16 核**并行，基于 [Rayon](https://github.com/rayon-rs/rayon) — 单核自动降级为串行
- **兼容的状态格式**: `[x, y, z, vx, vy, vz, ...alpha]` — 与 d3-force 一致
- 基于 [NAPI-RS](https://napi.rs/) 构建，支持 typed-array 高性能路径

## 安装

```bash
npm install @xingwangzhe/force-rs
```

主包只包含 ESM 加载器和类型声明。CI 将各平台原生二进制发布为独立的作用域子包，npm 只安装当前平台对应的可选依赖。

## API

### `simTick(state, links, n, options)`

执行一次力导仿真 tick。

```ts
import { simTick } from "@xingwangzhe/force-rs";

const opts = {
  repulsion: 3000, // 多体电荷斥力强度
  linkDistance: 500, // 弹簧自然长度
  centerStrength: 0.005, // 质心平移引力
  theta: 0.8, // Barnes-Hut 近似阈值
  velocityDecay: 0.6, // 速度衰减系数
  alphaDecay: 0.02, // 降温速率
};

// state: n*6 + 1 个 f64 [x0,y0,z0,vx0,vy0,vz0, ..., alpha]
// links: 2*m 个 u32 [src0,tgt0, src1,tgt1, ...]
const newState = simTick(state, links, n, opts);
```

重复执行 tick 时，建议预处理 links，并使用 typed array：

```ts
import { createSimulation } from "@xingwangzhe/force-rs";

const simulation = createSimulation(new Uint32Array(links), n);
const nextState = simulation.tick(new Float64Array(state), opts);
```

**ForceOptions:**

| 参数             | 类型   | 说明                                                    |
| ---------------- | ------ | ------------------------------------------------------- |
| `repulsion`      | number | 多体电荷斥力强度                                        |
| `linkDistance`   | number | 弹簧自然长度                                            |
| `centerStrength` | number | 质心平移引力                                            |
| `theta`          | number | Barnes-Hut 近似阈值                                     |
| `velocityDecay`  | number | 速度衰减系数                                            |
| `alphaDecay`     | number | 降温速率                                                |
| `algorithm`      | string | 默认 `fast`，可选实验性的 `ordered` / `linear` 或 `legacy` 对照路径 |
| `distanceMax`    | number | 可选的有限斥力半径                                      |

**返回:** `simTick` 返回 `Array<number>`；prepared typed-array API 返回 `Float64Array`。

默认 `fast` 复用 SoA 状态、CSR 弹簧系数和连续内存八叉树，并使用无栈前序遍历；保留原始插入顺序和每个节点的力求和顺序。实验性的 `ordered` 仅按 Morton 顺序计算独立节点，再恢复公开节点顺序；`linear` 会改变树的插入顺序。`legacy` 保留旧指针树用于数值对照。并行质心归约仍可能产生浮点舍入差异。

### 有界批量仿真

```ts
const result = simulation.run(new Float64Array(state), opts, 500, 0.001, 14 * 60 * 1000);
// result.state: Float64Array；result.ticks: number；result.stoppedByTime: boolean
```

`run` 最多执行 `maxTicks` 次，每次完成 tick 后检查 `alpha < alphaMin`，
并可选地在 `timeLimitMs` 毫秒后停止。返回实际完成次数，并区分冷却完成与时间截断。
次数为零时返回输入副本，不修改输入数组。批次内部复用两个原生结果缓冲，避免为每个
中间状态创建 JS 外部数组。需要进度或取消时可分批调用；时间截断属于不完整布局，
消费方应自行决定是否使用或缓存。

## 使用示例

```ts
import { simTick } from "@xingwangzhe/force-rs";

// 3 个节点：位置 + 速度 + alpha
const state = [
  0,
  0,
  0,
  0,
  0,
  0, // 节点 0
  100,
  0,
  0,
  0,
  0,
  0, // 节点 1
  0,
  100,
  0,
  0,
  0,
  0, // 节点 2
  1.0, // alpha
];
const links = [0, 1, 0, 2]; // 节点 0 与 1、2 相连

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
// s 中即收敛后的 3D 坐标
```

## 实测性能

2026-10-18 使用真实 FriendLinks 图：**49,064 节点、107,527 边、342 个冷却 tick**。
初始状态与所有力参数相同；Intel Core i7-1260P、16 个 Rayon 线程、可移植发布构建。

| 实现 / 调用方式 | 完整 342 tick | 进程峰值 RSS |
| --- | --- | --- |
| 已发布的 0.2.3 | 42.99 秒 | 独立基线未记录 |
| 0.3 候选，逐次 `tick` | 21.07 秒 | 840.8 MiB |
| 0.3 候选，每批 10 次，两个独立进程 | 23.02 / 21.61 秒 | 156.1 / 156.1 MiB |
| 0.3 候选，单批 342 次 | 22.94 秒 | 76.9 MiB |

以上为隔离运行的完整原生测量，不能视为所有硬件与输入的吞吐保证。arena 与无栈遍历
降低 CPU 计算成本；批处理减少中间 JS 外部数组，10 次一批的峰值 RSS 在此入口降低
81.4%。CPU 时间没有证明批处理更快；小幅差异容易受调度与温度影响。

全部坐标有限，最终 alpha 与图形几何摘要一致。没有减少 tick、增大 theta 或修改力
模型。Morton 计算顺序仅有有限的探索性收益，保留为显式实验选项；四查询共享遍历
原型实测更慢，已移除。本机 PGO 在平衡语料上的几何平均收益仅 1.1%，未达到已有
3% 选择阈值，因此选择无 PGO 的发布构建。

树实现仍位于 `src/lib.rs`，不依赖外部数值树库。详见
[原始测量与复现方法](reports/build-performance-2026-10-18/README.md)、
[算法与 crates 调研](scripts/ALGORITHM-RESEARCH.md)，真实 fixture 压测入口为
`scripts/real-graph-benchmark.ts`。

## 协议

MIT

## Runtime-focused release builds

Native CI artifacts use full LTO, one code-generation unit, and O3, while preserving the existing CPU instruction baseline. CI trains a profile-guided optimization (PGO) candidate using representative graph workloads and publishes it only when the runtime comparison passes; otherwise it publishes the unprofiled release. `bun run build:pgo` produces the optimized addon; `bun run benchmark:pgo` compares it with an unprofiled release. Install `llvm-tools-preview` with the same Rust toolchain first. Details and measurement limits: [scripts/PERFORMANCE.md](scripts/PERFORMANCE.md).
