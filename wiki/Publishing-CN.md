# 发布（crates.io + docs.rs）

给维护者的说明。仓库已为两个平台配置就绪；本页记录配置与发布流程。

## Cargo 元数据

`Cargo.toml` 携带 crates.io 与 docs.rs 需要的字段：

```toml
[package]
name = "tzcraft"
version = "0.1.1"
edition = "2021"
authors = ["blueokanna"]
license = "Apache-2.0"
description = "A schema-driven date & time library: one 128-bit nanosecond timeline, a const civil calendar, and codec-aware wire formats on nextjson / rustbinary."
repository = "https://github.com/blueokanna/Tzcraft"
documentation = "https://docs.rs/tzcraft"
readme = "README.md"
keywords = ["date", "time", "datetime", "timezone", "posix"]
categories = ["Date and Time", "Value Formatting and Parsing", "Data Processing"]

[package.metadata.docs.rs]
all-features = true
rustdoc-args = ["--cfg", "docsrs"]
```

- `readme = "README.md"` —— 英文 README 是 crates.io 与 docs.rs 的默认展示；`README_CN.md` 从它互链。
- `documentation = "https://docs.rs/tzcraft"` —— docs.rs 是官方文档宿主。
- `license = "Apache-2.0"`，仓库内有完整 `LICENSE` 文件。
- `keywords` 与 `categories` 取自 crates.io 白名单。
- MSRV 以 `rust-version = "1.81"` 声明。

## docs.rs 构建

docs.rs 用 `--all-features` 与 `--cfg docsrs` 标志构建。crate 在 CI 中镜像了完全相同的条件（nightly + `RUSTDOCFLAGS="--cfg docsrs"`），以便在发布前就捕获 docs.rs 构建失败。`doc_cfg` 属性（在 Rust 1.92 中吸收了 `doc_auto_cfg`）在特性门控条目上渲染"Available on crate feature X only"徽章。

本地验证：

```sh
# stable，捕获 intra-doc 链接与缺文档问题
RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps

# 与 docs.rs 完全一致的条件
RUSTDOCFLAGS="--cfg docsrs" cargo +nightly doc --all-features --no-deps
```

## 发布清单

1. 更新 `Cargo.toml` 中的 `version`；保持 `rust-version` 与精确依赖锁定与 CI 验证的一致。
2. 本地跑全套测试与 docs.rs 同款构建（上面命令）。
3. `cargo package --list` 检查 crate 内容（确认没有 `target/` 文件、README 双版本齐全）。
4. （可选）`cargo publish --dry-run`，然后 `cargo publish`。
5. docs.rs 构建完成后，确认 crate 页与 `migration` / `write` 模块渲染正确。

## 依赖

两个依赖均为可选且精确锁定：

```toml
nextjson = { version = "=0.1.4", features = ["derive"], optional = true }
rustbinary = { version = "=0.1.6", optional = true, features = ["std"] }
```

依赖图中**没有任何第三方日期时间库**：`migration` 模块只是文档。精确锁定（`=`）保证 MSRV 与线格式行为可复现，是刻意的：caret 范围会让未来版本悄悄抬高 MSRV 或改变本 crate 依赖的行为。
