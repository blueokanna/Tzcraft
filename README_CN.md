# Tzcraft

> 中文版说明。英文版（默认，crates.io / docs.rs 使用）：[README.md](./README.md)。

一个 Rust 日期时间库，只围绕一个想法构建：**时间只有一根轴，剩下的都是投影**。整条时间轴是一根**有符号 128 位纳秒计数器**（`Ticks`）；投影公历（proleptic Gregorian）、星期、ISO 周、时区偏移全都是这根轴上的纯投影。没有互相纠缠的 `Add` impl，没有全局的"当前时区"变量，没有 IANA 数据库的运行时下载，没有 `unsafe`。

本 crate 在任何配置下都是 `#![no_std]`；关闭默认特性后，**完全不依赖分配器**。

## 为什么又造一个轮子

生态里的参考库带着本项目刻意不要的形状：

- `chrono` 把时区做进类型参数里，`DateTime<Tz>` 到处跟着泛型走，一套运算要实现好几遍；
- `time` 用"从午夜起的 `i64` 纳秒 + 偏移量"的表示，量程受限，溢出是需要惦记的事；
- `jiff` 很漂亮，但它背后是 IANA 数据库、运行时状态和沉重的依赖树。

`Tzcraft` 换了一套设计前提，每一条都是可验证的代码事实，不是口号：

**1. 一根时间轴，一个算数源头。**
`Ticks` 是唯一的瞬时类型：从 Unix 纪元（`1970-01-01T00:00:00Z`）起的**有符号 128 位纳秒计数**。128 位同时买到了完整纳秒精度和约 ±5.4×10^21 年的存储量程——不存在"小瞬时/大瞬时"两套类型，也没有溢出塌缩策略需要记。`Duration` 是独立的**有符号**跨度类型：类型系统直接禁止你把两个瞬时相加，因为 `Ticks + Ticks` 根本编译不过。

**2. 公历类型是投影，不是主人。**
`Date`、`TimeOfDay`、`CivilDateTime` 不持有任何自己的算术。跨午夜的加法、跨年的进位，全部投影到那根 `i128` 纳秒轴上一次性算完。月份/年份这类"日历语义"运算只在一处实现（投影 → 调整 → 再投影），你不需要在十几个类型组合里挑该用哪个 impl。

**3. 编译器就是日历。**
闰年规则、日数与公历互转、星期、ISO 周——**全是 `const fn`**。编译器在编译期就把日历数学算掉了：

```rust
use tzcraft::{Date, Weekday};

const NEW_YEAR_2025: Date = Date::from_days_since_epoch(20_089);
const WD: Weekday = NEW_YEAR_2025.weekday(); // 编译器算出来是星期三
assert_eq!(WD, Weekday::Wednesday);
```

时区同样是 `const` 数据：`Zone` 要么是 `Utc`，要么是一个固定 `Offset`，作为值随瞬时一起携带在 `Zoned` 里。没有全局注册表、没有可变的"当前时区"、没有隐藏上下文。`Zoned` 因此天然是 `Copy` + `Send` + `Sync`。

**4. 编解码器自己挑线格式。**
每个类型只实现一次 `nextjson` 的格式中立契约（`NsonSerialize` / `NsonDeserialize`）。编码时问一句 `is_human_readable()`：

| 类型 | 人类可读（nextjson JSON） | 二进制（rustbinary） |
| --- | --- | --- |
| `Ticks` | RFC 3339 字符串 | `i128` 纳秒 |
| `Duration` | ISO 8601 时长字符串 | `i128` 纳秒 |
| `Date` | `YYYY-MM-DD` | `i32` 天数 |
| `TimeOfDay` | `HH:MM:SS[.f]` | `u64` 当日纳秒 |
| `CivilDateTime` | 无时区 ISO 字符串 | 打包 `i128` |
| `Offset` | `+08:00` / `Z` | `i32` 秒 |
| `Zone` | `UTC` / 偏移字符串 | 带标签数组 |
| `Zoned` | 带偏移的 RFC 3339 | `[ticks, offset]` 数组 |
| `Weekday` | `"Monday"`（也接受数字） | `u8` 判别值 |
| `Month` | `"January"`（也接受数字） | `u8` 月份号 |

JSON 始终可读、自描述，二进制始终紧凑——没有独立的 serde 模块，也没有"改个 feature 就悄悄换格式"的坑。文本和二进制共享同一套实现，这正是 nextjson 格式中立契约的用武之地。

## `no_std`：带分配器，或完全不带

本 crate 在所有构建下都是 `#![no_std]`。`alloc` 特性（默认开启，因为 `std` 蕴含它）只门控两类东西：返回 `String` 的方法，以及编解码器。

| 特性 | 作用 |
| --- | --- |
| `std`（默认） | `Ticks::now()`、`to_std_time`、`std::error::Error` |
| `alloc`（默认经 `std` 开启） | `to_rfc3339`、`format`、`to_iso`、`to_iso8601` 等返回 `String` 的方法 |
| `serde`（默认） | `nextjson` 文本/二进制编解码实现（`tzcraft::codec`） |
| `binary`（默认） | `rustbinary` 紧凑线格式（`tzcraft::binary`） |

使用 `--no-default-features` 构建时，crate **不链接任何分配器**：解析、算术、`Display`/`FromStr`，以及全部 `write_*` 缓冲区格式化方法都照常工作。每个返回 `String` 的方法都有一个免分配的孪生方法，写入调用者提供的字节切片并返回写入长度：

```rust
use tzcraft::{Date, Ticks};

let mut buf = [0u8; 64];
let d = Date::from_ymd(2024, 2, 29).unwrap();
let n = d.write_iso(&mut buf).unwrap();
assert_eq!(&buf[..n], b"2024-02-29");

let n = Ticks::EPOCH
    .write_rfc3339(&mut buf, tzcraft::FractionDigits::None)
    .unwrap();
assert_eq!(&buf[..n], b"1970-01-01T00:00:00Z");
```

免分配的输出端以 `tzcraft::write::{Write, Buf}` 公开，可直接用于嵌入式目标。

## 快速上手

```rust
use tzcraft::{Date, Duration, Months, Offset, Ticks, Weekday, Zone, Zoned};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 一根时间轴，任意读数。
    let launch = Ticks::from_rfc3339("2024-06-15T08:30:00Z")?;
    let local = launch.to_zoned(Zone::fixed(Offset::from_hms(8, 0, 0)?));
    assert_eq!(local.to_rfc3339(tzcraft::FractionDigits::None), "2024-06-15T16:30:00+08:00");
    assert_eq!(local.date()?.weekday(), Weekday::Saturday);

    // 日历感知的月份运算会钳制日期，而不是溢出。
    let jan = Date::from_ymd(2023, 1, 31)?;
    assert_eq!(jan.checked_add_months(Months::new(1))?, Date::from_ymd(2023, 2, 28)?);
    assert_eq!(jan.checked_add_months(Months::new(13))?, Date::from_ymd(2024, 2, 29)?); // 闰年

    // 时长带符号，ISO 8601 严格往返。
    let span = Duration::from_iso8601("P1DT2H3M4.5S")?;
    assert_eq!(span.to_iso8601(), "P1DT2H3M4.5S");

    // 文本和二进制共享同一套实现。
    let json = nextjson::nextencode(&local)?;
    let back: Zoned = nextjson::nextdecode(&json)?;
    assert_eq!(back, local);

    let bin = tzcraft::binary::encode(&local)?;
    let back: Zoned = tzcraft::binary::decode(&bin)?;
    assert_eq!(back, local);
    Ok(())
}
```

自定义固定时区就是一行 `const`：

```rust
use tzcraft::{Offset, Zone};

const TOKYO: Zone = Zone::fixed(Offset::east(9 * 3600));
```

组合进派生结构体也很自然（derive 来自 nextjson，`tzcraft` 类型可直接混用）：

```rust
#[derive(Debug, PartialEq, nextjson::NsonSerialize, nextjson::NsonDeserialize)]
struct Alarm {
    name: String,
    when: Zoned,
    repeat: tzcraft::Weekday,
    snooze: tzcraft::Duration,
}
```

## Y2038：构造上就安全

2038 年是 32 位有符号秒计数溢出问题（纪元后 `i32::MAX` 秒 = `2038-01-19T03:14:07Z`，下一秒就会回绕成负数）。`Tzcraft` 在构造上不可能遇到它：

- `Ticks` 是纪元起的 `i128` 纳秒——存储量程约 ±5.4×10^21 年（`i64` 秒访问器把可用范围界定在约 ±2920 亿年）；
- 所有 `timestamp*` 访问器与 `from_timestamp*` 构造器都使用 `i64` 秒——量程约 ±2920 亿年；
- `Date` 是纪元起的 `i32` **天**（约 ±580 万年），从来不是秒；
- 唯一使用 `i32` 的时域值是 `Offset`，限定在 ±24 小时内且构造时校验。

`tests/y2038.rs` 锁定了边界行为：精确的回绕秒（`2_147_483_647` → `2_147_483_648`）、纪元前极值、9999 年之后的扩展年份，以及跨越边界的文本往返。

## 从 chrono 迁移

`Tzcraft` 覆盖了 `chrono` 应用日常依赖的那一整套 API。`_opt` 变体在我们的 `Result` 模型里直接变成 `?`：

| chrono | tzcraft |
| --- | --- |
| `Utc::now()` / `Local::now()` | `Ticks::now()?` / `Zoned::now_utc()?` |
| `DateTime::<Utc>` | `Ticks` |
| `DateTime::<FixedOffset>` | `Zoned` |
| `NaiveDate` | `Date` |
| `NaiveTime` | `TimeOfDay` |
| `NaiveDateTime` | `CivilDateTime` |
| `Duration` / `TimeDelta` | `Duration`（`seconds/hours/days/weeks/num_*` 同名可用） |
| `from_ymd_opt` / `from_hms_opt` | `Date::from_ymd` / `TimeOfDay::from_hms`（返回 `Result`） |
| `DateTime::from_timestamp` / `timestamp()` | `Ticks::from_timestamp` / `timestamp()`（含 `_millis`/`_micros`/`_nanos`） |
| `date.and_hms_opt(...)` | `date.and_hms(...)?` |
| `d.checked_add_months(Months::new(1))` | 同名同签名 |
| `d.checked_add_days(Days::new(1))` | 同名同签名 |
| `dt.format("%Y-%m-%d %H:%M:%S")` | `dt.format("%Y-%m-%d %H:%M:%S")?`（未知指令报错而非静默丢弃） |
| `NaiveDate::parse_from_str` / `NaiveDateTime::parse_from_str` | `Date::parse_from_str(s, fmt)` / `CivilDateTime::parse_from_str(s, fmt)` |
| `DateTime::parse_from_str` | `Ticks::parse_from_str` / `Zoned::parse_from_str`（civil 路径要求时区偏移） |
| `to_rfc3339` / `parse_from_rfc3339` | `to_rfc3339(frac)` / `from_rfc3339` |
| `to_rfc2822` / `parse_from_rfc2822` | 同名 |
| `Datelike::year/month/day/ordinal/weekday/iso_week/num_days_from_ce` | 同名固有方法 |
| `Timelike::hour/minute/second/nanosecond/num_seconds_from_midnight` | 同名固有方法 |
| `FixedOffset::east_opt/from_hms_opt` | `Offset::from_seconds` / `Offset::from_hms` |
| `dt.with_timezone(...)` | `z.with_zone(...)` |
| `checked_add_signed` / `signed_duration_since` | 同名 |
| `with_year/with_month/.../with_nanosecond` | 同名 |
| `Duration::to_std/from_std` | 同名 |
| serde 支持 | nextjson `NsonSerialize` / `NsonDeserialize`（文本 + rustbinary 二进制） |

三处有意的差异（都是安全/正确性考量，不是疏漏）：

1. **`format()` 返回 `Result`**：未知指令是错误而不是像 chrono 那样静默丢弃。
2. **`timestamp()` 用 floor 语义**：纪元前的瞬时（如 `1969-12-31T23:59:59.5Z` → `-1`）符合 Unix 时间的数学定义，而 chrono 是向零截断（会给出 `0`）。
3. **`num_*` 返回 `Result<i64>`**：超出 `i64` 时显式报错而不是静默溢出。

`Local`（系统本地时区）不在 v1 里：纯 `std` 拿不到本地偏移（需要 `libc`/平台 FFI，而核心 crate 只有 nextjson 和 rustbinary 两个依赖，且 `#![deny(unsafe_code)]`）。如果你的墙钟要跟随真实本地时区，用平台 API 解析出偏移后交给 `Zone::fixed(...)` 即可——接缝是显式的。

## 迁移：把 `chrono` / `time` / `rustix` 代码迁进来

迁移是**单向容易：迁入 `Tzcraft`**。crate 不依赖 `chrono`、`time` 或 `rustix`——它唯一的依赖是 `nextjson` 与 `rustbinary`（均为可选）——所以没有任何东西会链接那些库，也没有任何东西让离开比到达更容易。上表（以及 [`tzcraft::migration`](https://docs.rs/tzcraft/latest/tzcraft/migration/index.html) 模块文档里的 `time`/`rustix` 对应表）展示了机械化、同名对齐的移植方式。

- `chrono`：见上表——`DateTime`/`NaiveDate`/`NaiveTime`/`NaiveDateTime`/`Duration`/`FixedOffset` 分别对应 `Ticks`/`Date`/`TimeOfDay`/`CivilDateTime`/`Duration`/`Offset`。
- `time`：`OffsetDateTime` → `Zoned`，`PrimitiveDateTime` → `CivilDateTime`，`Date` → `Date`，`Time` → `TimeOfDay`，`UtcOffset` → `Offset`，`Duration` → `Duration`。
- `rustix`：`Timespec`（POSIX `{tv_sec, tv_nsec}` 对）→ `Ticks::from_timespec(tv_sec, tv_nsec)`。

**值的**移植是精确的：每个 `tzcraft` 访问器产出的整数与参考库使用的完全相同（`Ticks::to_unix_seconds` → `(i64, u32)`、`Date::parts`、`TimeOfDay::parts`、`Offset::as_seconds`、`Duration::as_nanos`）。完整映射与逐步示例见 `tzcraft::migration` 模块。

## 明确不做什么

这份 README 必须诚实，所以把边界写清楚：

- **没有 IANA 时区数据库，也没有 DST。** `Zone` 只有 `Utc` 和固定偏移两种。如果你的墙钟要跟随真实夏令时切换，请用自己的策略解析出偏移，再把 `Zone::Fixed` 交给库。这条缝是显式收窄的，也是为将来加 `Zone::Database` 变体预留的非破坏性接缝。
- **只有投影公历（proleptic Gregorian）**，包括公元 0 年（即公元前 1 年）。没有儒略历、希伯来历之类的多历法。
- **严格实现 ISO 8601 / RFC 3339 / RFC 2822，外加完整的 strftime 引擎**。我们交付的是完整且经过测试的实现，没有半成品模板引擎。
- **没有 `unsafe`。** 依赖只有 `nextjson`（编解码）和 `rustbinary`（二进制），均为可选。依赖图中不会出现任何第三方日期时间库。

## 内部实现

```text
src/
  calendar.rs   公历核心：闰年、日数互转、星期、ISO 周 —— 全部 const fn
  units.rs      Days / Months / IsoWeek（chrono 兼容的强类型单位）
  ticks.rs      Ticks：唯一的瞬时类型，i128 纳秒
  duration.rs   Duration：有符号跨度
  date.rs       Date：i32 天数投影
  time.rs       TimeOfDay：u64 当日纳秒
  datetime.rs   CivilDateTime：无时区日期时间
  offset.rs     Offset：±24 小时内秒数
  zone.rs       Zone：Utc / 固定偏移
  zoned.rs      Zoned：Ticks + Zone
  write.rs      免分配的 Write trait + Buf 输出端（no_std，无分配器）
  format.rs     手写 ISO/RFC3339 解析器与格式化（无正则、无外部解析器）
  strftime.rs   strftime 格式引擎 + RFC 2822（chrono 兼容面）
  codec.rs      nextjson 契约实现（human-readable 分支文本、否则二进制）
  binary.rs     rustbinary 门面
  migration.rs  迁移指南：把 chrono / time / rustix 代码迁进来
```

解析器是逐字节扫描的，每个失败都带精确字节偏移。小数秒最多 9 位，多了直接拒绝而不是截断——静默截断是对数据的背叛。

## 测试与安全

- 日历：±20 万天逐日往返、6000 年全量公历往返、星期锚点、ISO 周边界向量；
- 格式化：RFC 3339 / RFC 2822 / strftime 往返、非法输入拒绝清单；
- 编解码：全部类型在 nextjson 文本与 rustbinary 二进制下各自往返，外加派生结构体混合往返；
- **chrono 平价**：`tests/chrono_parity.rs` 用 chrono 的真实写法验证迁移面；
- **Y2038**：`tests/y2038.rs` 锁定边界秒、纪元前极值与扩展年份；
- **无分配器**：`tests/no_alloc.rs` 只在 `--no-default-features` 下编译运行，证明解析、`Display`/`FromStr` 与 `write_*` 缓冲 API 在无分配器时可用；
- **健壮性**：`tests/robustness.rs` 用确定性 PRNG 生成数千组对抗性输入（随机字节、超长输入、畸形结构、超大数字、恶意格式字符串）喂给所有解析与格式化入口，契约是**任何输入都不得 panic、不得无界分配**；
- **底层审计**：全库逐行检查 `as` 窄化转换与 `i64` 中间乘法。审计发现并修复了 `Duration::from_days/minutes/hours/weeks` 在 `i64` 域乘法的溢出（`i64::MAX` 时 debug 下 panic、release 下静默回绕成错误值），以及 `Ticks`/`Zoned` 的 `checked_add_days(Days)` 对 `u64` 超出 `i64` 时窄化回绕成负数的问题——现在全部在 `i128` 域计算或显式报错，有回归测试锁定；
- 依赖审计：`cargo audit` 对 `Cargo.lock` 零已知漏洞（退出码 0）。

```sh
cargo test --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo audit
```

## 基准对比

[`benchmark.md`](./benchmark.md) 存放对比报告：tzcraft 对 `chrono` 0.4.45 / `time` 0.3.55 / `jiff` 0.2.35，覆盖 RFC 3339 解析与格式化、公历投影、日期算术、时长算术、星期，外加一次防 panic 模糊测试和依赖/`unsafe` 足迹统计。

基准程序位于 **`benchmarks/`** 包（`publish = false`）。这是三个对比库唯一出现的地方；它们绝不进入 tzcraft 的依赖图。GitHub Action 在每次 push 到 `main` 时重跑并把新数字提交回 `benchmark.md`。

方法学见 [`benchmarks/README.md`](./benchmarks/README.md) 与报告内说明：相同输入、`black_box`、用变化的输入数组阻止循环不变式外提、取三次最小值。数字与机器相关，只可在同一次运行内比较。

## CI

`.github/workflows/ci.yml` 在每次 push 和 PR 上运行：格式检查、`-D warnings` 的 clippy、debug + release 双模式测试、feature 矩阵（`--no-default-features` 与 `alloc` / `std` / `serde` / `binary` 各自启用）、`-D warnings` 的文档构建、**docs.rs 同款构建**（nightly + `--cfg docsrs`，与 docs.rs 实际使用的标志完全一致）、RustSec 公告库安全审计，以及在声明的 **MSRV 1.81** 上跑全套测试。

## 许可

Apache-2.0（见 [LICENSE](./LICENSE)）。
