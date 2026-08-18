# 安全与测试

## 保证

- **没有 `unsafe`。** 全 crate 开启 `#![deny(unsafe_code)]`，源码中没有任何 `unsafe` 块。
- **任何输入都不得 panic。** 解析器接受不可信字节；契约是任何长度与内容的输入都不得引发 panic、无界分配或非确定行为。畸形输入以带字节偏移的错误拒绝。
- **绝不静默截断。** 小数秒最多 9 位，多了直接拒绝而不是截断。
- **绝不静默溢出。** 所有 `checked_*` 操作都显式报错。单位换算的时长构造器在 `i128` 域计算。`num_*` 返回 `Result<i64>` 而不是回绕。
- **Y2038 构造上安全。** 不存在把秒存进 32 位的代码路径。见 [Y2038](Y2038-CN)。

## 审计方法

底层审计是对以下每一项的人工复查：

- 所有 `as` 窄化转换，
- `i64` 与 `i128` 宽乘法，
- 非测试代码中的 `unwrap` / `expect` / `panic!` / `unreachable!`，
- 时间线算术中的未检查加减。

前几轮审计发现并修复了（均带回归测试）：

1. `Duration::from_days/minutes/hours/weeks` 在 `i64` 域乘法溢出（`i64::MAX` 时 debug panic、release 静默回绕）。修复：改在 `i128` 域计算。
2. `Ticks`/`Zoned::checked_add_days(Days)` 把超过 `i64` 的 `u64` 天数窄化成负数。修复：显式 `i64::try_from`。
3. `Ticks::duration_since` 与 `CivilDateTime::checked_add` 有未检查的 `i128` 加法（`MAX - MIN` 溢出）。修复：`saturating_sub` 与 `checked_add` 链。
4. `%s` 格式化把纳秒窄化成 `i64` 秒，极端瞬时回绕。修复：越界时渲染为空。

非测试代码中剩余的 `unwrap`/`expect`/`panic!` 都是基于不变量的（校验过的 `TimeOfDay` 永远是合法的 `chrono::NaiveTime`/`time::Time`；新 `String` 不会溢出；`Buf` 只持有合法 UTF-8），或是文档化的 const 构造器 panic（`Offset::east`/`west`，与 `chrono::FixedOffset::east` 一致）。

## 测试套件

| 套件 | 验证内容 |
| --- | --- |
| `src/**` 单元测试 | 日历 ±20 万天逐日往返、6000 年全量公历往返、星期锚点、ISO 周边界、格式化向量、解析拒绝清单 |
| `tests/chrono_parity.rs` | 用真实 chrono 写法验证迁移面 |
| `tests/y2038.rs` | 2038 边界、纪元前极值、扩展年份 |
| `tests/no_alloc.rs` | 免分配器表面（仅 `--no-default-features`） |
| `tests/robustness.rs` | 数千组对抗性输入（随机字节、超长输入、畸形结构、超大数字、恶意格式字符串）喂给所有解析与格式化入口 |
| `tests/readme.rs` | README 代码片段可编译运行 |
| doctest | crate 级与模块级示例 |

全部运行：

```sh
cargo test --all-features           # debug
cargo test --all-features --release # release（无溢出检查；两种模式必须一致）
cargo test --no-default-features   # 免分配器构建
cargo clippy --all-features --all-targets -- -D warnings
```

## 依赖

`cargo audit`（RustSec 公告库）对锁定的依赖图报告零已知漏洞（图中除 `tzcraft` 本身外只有 `nextjson` 与 `rustbinary`）。

## CI

`.github/workflows/ci.yml` 在每次 push 和 PR 上运行：

- `cargo fmt --check`
- `cargo clippy --all-features --all-targets -- -D warnings`
- `cargo test --all-features`（debug 与 release）
- feature 矩阵：`--no-default-features` 与 `alloc` / `std` / `serde` / `binary` 各自启用
- `cargo doc --all-features --no-deps`（`-D warnings`）
- docs.rs 同款构建：nightly + `--cfg docsrs`（与 docs.rs 实际使用的标志一致）
- `cargo audit`（`rustsec/audit-check`）
- MSRV **1.81** 上的全套测试
