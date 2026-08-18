# no_std 与特性

## 一句话版本

crate 在**所有**配置下都是 `#![no_std]`。默认特性下链接 `alloc`；`--no-default-features` 下完全不链接分配器。

## 特性矩阵

| 特性 | 默认 | 作用 |
| --- | --- | --- |
| `std` | 开 | `Ticks::now()`、`Ticks::to_std_time`、`impl std::error::Error` |
| `alloc` | 开（由 `std` 蕴含） | 返回 `String` 的格式化方法（`to_rfc3339`、`format`、`to_iso`、`to_iso8601` 等） |
| `serde` | 开 | `nextjson` 编解码实现（`tzcraft::codec`） |
| `binary` | 开 | `rustbinary` 紧凑线格式（`tzcraft::binary`） |

没有其它特性，依赖图中也没有任何第三方日期时间库。唯一的依赖是 `nextjson` 与 `rustbinary`，均为可选。

`Cargo.toml` 中的特性蕴含关系：

```toml
std = ["alloc"]
alloc = []
serde = ["dep:nextjson", "alloc"]
binary = ["dep:rustbinary", "serde", "alloc"]
```

`std` 蕴含 `alloc`；编解码器也需要 `alloc`（它们产生和消费 `String`）。关掉全部默认特性，就得到一个不依赖分配器的构建。

## 没有分配器时仍能做什么

使用 `--no-default-features`：

- 解析（`from_rfc3339`、`from_iso`、`from_iso8601`、`parse_from_str`、`FromStr`）——解析器是对 `&[u8]` 的逐字节扫描器，不使用堆；
- 算术（checked/saturating 加减、月份/年份步进、时长）；
- 所有类型的 `Display`——直接写入 `fmt::Formatter`；
- 所有类型的 `FromStr`；
- 全部 `write_*` 缓冲方法——写入调用者提供的 `&mut [u8]` 并返回字节数：
  - `Ticks::write_rfc3339` / `write_rfc2822` / `write_format`
  - `Date::write_iso` / `write_format`
  - `TimeOfDay::write_iso` / `write_format`
  - `CivilDateTime::write_iso` / `write_format`
  - `Duration::write_iso8601`
  - `Offset::write_iso`
  - `Zone::write_iso`
  - `Zoned::write_rfc3339` / `write_rfc2822` / `write_format`

## 什么需要 `alloc` 特性

所有返回 `String` 的方法：`to_rfc3339`、`to_rfc2822`、`format`、`to_iso`、`to_iso8601`（以及 `Offset`/`Zone` 上的 `to_iso`），还有 `codec`/`binary` 模块。它们都是同一套 `write_*` 机制的薄包装——字符串写进一个实现了 crate 自身 `Write` trait 的新 `String`。

docs.rs（`--all-features` 构建）上，这些方法会带"Available on crate feature alloc only"徽章。

## `write` 模块

```rust
use tzcraft::write::{Buf, Write};

let mut storage = [0u8; 64];
let mut buf = Buf::new(&mut storage);
buf.write_str("hello")?;
assert_eq!(buf.as_str(), "hello");
```

`tzcraft::write::Write` 是一个三方法 trait（`write_bytes` 加默认的 `write_str`/`write_byte`/`write_char`）。crate 不依赖 `core::fmt::Write`（并非所有 `no_std` 目标都可用），因此定义了自己的最小 trait。`Buf` 是定容输出端；缓冲区过小返回 `Error::buffer_overflow()`——绝不会 panic，也绝不静默截断。

## Cargo.toml 用法

```toml
[dependencies]
tzcraft = { version = "0.1", default-features = false }   # 无分配器
tzcraft = { version = "0.1", default-features = false, features = ["alloc"] }
tzcraft = { version = "0.1" }                              # 默认：std + serde + binary
```

## MSRV

声明的最低 Rust 版本为 **1.81**。CI 在该版本上测试所有特性组合。
