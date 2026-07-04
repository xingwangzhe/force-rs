[English](./README.md) | [中文](./README_CN.md)

# @xingwangzhe/force-rs

基于 **Rust + Barnes-Hut 八叉树** 的快速 3D 力导向图布局，设计用于在构建期替代 `d3-force-3d`。

### 为什么需要这个包

`d3-force-3d` 在交互式可视化中表现优异，但在构建期（SSG/SSR）处理 47K+ 节点、89K+ 边时，JS 运行时成为瓶颈 — 数百个 tick、每个约 1s 太慢了。`force-rs` 用 Rust 实现了**完全相同的力模型**，在 16 核上仅需 **~0.01s/tick**。

### 力模型（完全兼容 d3-force）

| 力              | 实现方式                                                  |
|-----------------|----------------------------------------------------------|
| 多体斥力 (many-body) | Barnes-Hut 八叉树，O(n log n)，含 `√(4/k)` 修正因子 |
| 弹簧力 (link)   | 基于度数的偏置强度：`1/min(deg, deg)`，bias = 对方度数占比 |
| 中心力 (center) | 质心向原点平移（与 d3-force `forceCenter` 一致）         |
| 近距离软化      | `√(d_min² × d²)` 平滑过渡（与 d3-force 一致）           |

### 其他特性

- **16 核**并行，基于 [Rayon](https://github.com/rayon-rs/rayon) — 单核自动降级为串行
- **兼容的状态格式**: `[x, y, z, vx, vy, vz, ...alpha]` — 与 d3-force 一致
- 基于 [NAPI-RS](https://napi.rs/) 构建，零拷贝 Node.js 互操作

## 安装

```bash
npm install @xingwangzhe/force-rs
```

## API

### `simTick(state, links, n, options)`

执行一次力导仿真 tick。

```ts
import { simTick } from '@xingwangzhe/force-rs';

const opts = {
  repulsion: 3000,      // 多体电荷斥力强度
  linkDistance: 500,    // 弹簧自然长度
  centerStrength: 0.005,// 质心平移引力
  theta: 0.8,           // Barnes-Hut 近似阈值
  velocityDecay: 0.60,  // 速度衰减系数
  alphaDecay: 0.02,     // 降温速率
};

// state: n*6 + 1 个 f64 [x0,y0,z0,vx0,vy0,vz0, ..., alpha]
// links: 2*m 个 u32 [src0,tgt0, src1,tgt1, ...]
const newState = simTick(state, links, n, opts);
```

**ForceOptions:**

| 参数              | 类型   | 说明                |
|-------------------|--------|---------------------|
| `repulsion`       | number | 多体电荷斥力强度     |
| `linkDistance`    | number | 弹簧自然长度         |
| `centerStrength`  | number | 质心平移引力         |
| `theta`           | number | Barnes-Hut 近似阈值  |
| `velocityDecay`   | number | 速度衰减系数         |
| `alphaDecay`      | number | 降温速率             |

**返回:** `Array<number>` — 新状态（格式与输入相同，alpha 已更新）。

## 使用示例

```ts
import { simTick } from '@xingwangzhe/force-rs';

// 3 个节点：位置 + 速度 + alpha
const state = [
  0, 0, 0, 0, 0, 0,    // 节点 0
  100, 0, 0, 0, 0, 0,  // 节点 1
  0, 100, 0, 0, 0, 0,  // 节点 2
  1.0                    // alpha
];
const links = [0, 1, 0, 2]; // 节点 0 与 1、2 相连

const opts = {
  repulsion: 3000,
  linkDistance: 500,
  centerStrength: 0.005,
  theta: 0.8,
  velocityDecay: 0.60,
  alphaDecay: 0.02,
};

let s = state;
while (s[s.length - 1] > 0.001) {
  s = simTick(s, links, 3, opts);
}
// s 中即收敛后的 3D 坐标
```

## 实测性能

| 平台      | 47K 节点 × 89K 边 | 说明                           |
|-----------|-------------------|-------------------------------|
| 16 核     | **~0.01s/tick**   | Rayon `par_iter` 16 线程并行    |
| 1 核      | ~0.15s/tick       | 自动降级串行                    |

在 AMD EPYC / Intel Xeon 构建服务器上实测。Barnes-Hut 八叉树为 `src/lib.rs` 中自实现（无外部树结构 crate 依赖）。

## 协议

MIT
