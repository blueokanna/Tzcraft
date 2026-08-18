# tzcraft 维基首页

本维基介绍 **tzcraft**——一个基于单一想法的 Rust 日期时间库：*时间只有一根轴，剩下的都是投影*。

## 页面

| 页面 | 内容 |
| --- | --- |
| [设计与架构](Design-and-Architecture-CN) | 四大设计前提、类型模型，以及每个设计决策存在的原因 |
| [no_std 与特性](no_std-and-Features-CN) | 特性矩阵、免分配器构建、`write_*` 缓冲区 API |
| [Y2038 与量程](Y2038-CN) | 为什么 2038 年问题不可能发生，以及锁定的边界测试 |
| [迁移指南](Migration-Guide-CN) | 从 `chrono`、`time`、`rustix` 迁入——以及迁出 |
| [安全与测试](Safety-and-Testing-CN) | 审计方法、对抗性测试与保证 |
| [发布](Publishing-CN) | 维护者视角的 crates.io 与 docs.rs 配置 |

## 基准对比

[`benchmark.md`](https://github.com/blueokanna/Tzcraft/blob/main/benchmark.md)
是 CI 生成的对比报告：tzcraft 对 `chrono` / `time` / `jiff`（性能、防 panic 模糊测试、依赖与 `unsafe` 足迹）。基准程序在 `benchmarks/` 包（`publish = false`）里；三个对比库绝不进入 tzcraft 的依赖图。

English readers: see [Home](Home).

## 一句话版本

- `Ticks` 是唯一的瞬时类型：Unix 纪元起的有符号 128 位纳秒计数。量程约 ±2920 亿年，完整纳秒精度。
- `Date`、`TimeOfDay`、`CivilDateTime` 是这根时间轴的纯投影，不持有自己的算术。
- 所有公历计算都是 `const fn`；时区是 `const` 数据（`Zone::Utc` 或固定 `Offset`）。
- crate 在任何配置下都是 `#![no_std]`，`--no-default-features` 时完全不依赖分配器。
- 编解码（`nextjson` 文本、`rustbinary` 二进制）是一套实现、两种线格式。
- `tzcraft::migration` 记录单向路径：`chrono` / `time` / `rustix` 的代码**迁入** `tzcraft` 很容易；crate 不链接其中任何一个库。

## 快速链接

- 仓库：<https://github.com/blueokanna/Tzcraft>
- 文档：<https://docs.rs/tzcraft>
- crates.io：<https://crates.io/crates/tzcraft>
