[English](./README.md) | [中文](./README_CN.md)

# @xingwangzhe/force-rs

基于 Rust + Barnes-Hut N-body 模拟的高性能力导图布局，设计用于在构建期替代 `d3-force-3d`。

- **16 核**并行 `simTick`：47K 节点 × 89K 边约 **0.012s/tick**
- **单核**自动降级为串行
- 兼容 d3-force 状态格式：`[x, y, z, vx, vy, vz, ...alpha]`

## 安装

```bash
npm install @xingwangzhe/force-rs
```

## API

### simTick(state, links, n, options)

执行一次力导仿真 tick。

```ts
import { simTick } from '@xingwangzhe/force-rs';

const opts = {
  repulsion: 3000,      // 电荷斥力强度
  linkDistance: 30,     // 弹簧自然长度
  centerStrength: 0.01, // 中心引力
  theta: 0.8,           // Barnes-Hut 阈值
  velocityDecay: 0.35,  // 速度衰减系数
  alphaDecay: 0.02,     // 降温速率
};

// state: n*6 + 1 个 f64 [x0,y0,z0,vx0,vy0,vz0, ..., alpha]
// links: 2*m 个 u32 [src0,tgt0, src1,tgt1, ...]
const newState = simTick(state, links, n, opts);
```

**ForceOptions:**
| 参数            | 类型   | 说明           |
|----------------|--------|----------------|
| repulsion      | number | 电荷斥力强度    |
| linkDistance   | number | 弹簧自然长度    |
| centerStrength | number | 中心引力       |
| theta          | number | Barnes-Hut 近似阈值 |
| velocityDecay  | number | 速度衰减系数    |
| alphaDecay     | number | 降温速率       |

**返回:** `Float64Array` — 新状态（格式与输入相同，alpha 已更新）。

## 使用示例

```ts
import { simTick } from '@xingwangzhe/force-rs';

const state = new Float64Array([
  0, 0, 0, 0, 0, 0,
  1, 1, 0, 0, 0, 0,
  1.0
]);
const links = new Uint32Array([0, 1]);

let s = state;
const opts = { repulsion: 3000, linkDistance: 30, centerStrength: 0.01, theta: 0.8, velocityDecay: 0.35, alphaDecay: 0.02 };

while (s[s.length - 1] > 0.001) {
  s = simTick(s, links, 2, opts);
}
```

## 性能

| 平台       | 47K 节点 × 89K 边 | 说明                        |
|------------|-------------------|----------------------------|
| 16 核      | **~0.012s/tick**  | Rayon `par_iter` 16 线程并行 |
| 1 核       | ~0.15s/tick       | 自动降级串行                 |

基于 [`zhifeng_impl_barnes_hut_tree`](https://crates.io/crates/zhifeng_impl_barnes_hut_tree) 实现 O(n log n) 斥力计算。

## 协议

MIT OR Apache-2.0
