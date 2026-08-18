# 迁移指南

本页记录从 `chrono`、`time` 或 `rustix` **迁入** tzcraft。迁移是刻意**单向容易：迁入 tzcraft**。crate 的依赖图中没有任何第三方日期时间库——只有 `nextjson` 与 `rustbinary`（均为可选）——所以这里没有任何东西链接那些库，也没有任何东西让离开比到达更容易。

## 迁入 tzcraft（从 chrono）

日常 `chrono` 表面按 1:1 映射。`_opt` 变体在我们的 `Result` 模型里直接变成 `?`。

| chrono | tzcraft |
| --- | --- |
| `Utc::now()` / `Local::now()` | `Ticks::now()?` / `Zoned::now_utc()?` |
| `DateTime::<Utc>` | `Ticks` |
| `DateTime::<FixedOffset>` | `Zoned` |
| `NaiveDate` | `Date` |
| `NaiveTime` | `TimeOfDay` |
| `NaiveDateTime` | `CivilDateTime` |
| `Duration` / `TimeDelta` | `Duration` |
| `from_ymd_opt` / `from_hms_opt` | `Date::from_ymd` / `TimeOfDay::from_hms` |
| `DateTime::from_timestamp` / `timestamp()` | `Ticks::from_timestamp` / `timestamp()` |
| `date.and_hms_opt(...)` | `date.and_hms(...)?` |
| `d.checked_add_months(Months::new(1))` | 同名同签名 |
| `d.checked_add_days(Days::new(1))` | 同名同签名 |
| `dt.format(...)` | `dt.format(...)?` |
| `NaiveDate::parse_from_str` | `Date::parse_from_str(s, fmt)` |
| `DateTime::parse_from_str` | `Ticks::parse_from_str` / `Zoned::parse_from_str` |
| `to_rfc3339` / `parse_from_rfc3339` | `to_rfc3339(frac)` / `from_rfc3339` |
| `to_rfc2822` / `parse_from_rfc2822` | 同名 |
| `Datelike::*` / `Timelike::*` | 同名固有方法 |
| `FixedOffset::east_opt` | `Offset::from_seconds` |
| `dt.with_timezone(...)` | `z.with_zone(...)` |
| `checked_add_signed` / `signed_duration_since` | 同名 |
| `with_year/with_month/...` | 同名 |
| `Duration::to_std/from_std` | 同名 |

三处有意的差异（正确性考量，不是疏漏）：

1. **`format()` 返回 `Result`** —— 未知指令是错误而不是像 chrono 那样静默丢弃。
2. **`timestamp()` 用 floor 语义** —— `1969-12-31T23:59:59.5Z` 映射为 `-1`，符合 Unix 时间的定义；chrono 向零截断会给出 `0`。
3. **`num_*` 返回 `Result<i64>`** —— 超出 `i64` 时显式报错而不是静默溢出。

`Local`（系统本地时区）在没有平台 FFI 时不在范围内；请自行解析偏移后交给 `Zone::fixed(...)`。

## 迁入 tzcraft（从 time）

| time | tzcraft |
| --- | --- |
| `OffsetDateTime` | `Zoned`（UTC 用 `Ticks`） |
| `PrimitiveDateTime` | `CivilDateTime` |
| `Date` | `Date` |
| `Time` | `TimeOfDay` |
| `UtcOffset` | `Offset` |
| `Duration` | `Duration` |
| `Weekday` / `Month` | `Weekday` / `Month` |

`OffsetDateTime::unix_timestamp()` + `.nanosecond()` 喂给 `Ticks::from_timestamp`；`Date::year/month/day` 与 `Time::hour/minute/second/nanosecond` 喂给 `Date::from_ymd` 与 `TimeOfDay::from_hms_nano`。

## 迁入 tzcraft（从 rustix）

`rustix::time::Timespec` 是 POSIX `{tv_sec, tv_nsec}` 对，一行迁入：`Ticks::from_timespec(tv_sec, tv_nsec)`。`rustix` 没有其它时间表面需要移植。

## 精确移植值

每个 `tzcraft` 访问器产出的整数与参考库使用的完全相同，因此**值的**移植是精确的——无信息损失、无舍入：

| tzcraft 访问器 | 产出 |
| --- | --- |
| `Ticks::to_unix_seconds()` | `(i64 秒, u32 纳秒)` |
| `Ticks::as_unix_nanos()` | 纪元起 `i128` 纳秒 |
| `Ticks::to_timespec()` | `(i64, i64)` —— POSIX `timespec` 对 |
| `Date::parts()` | `(i32 年, u32 月, u32 日)` |
| `TimeOfDay::parts()` | `(u32 时, u32 分, u32 秒, u32 纳秒)` |
| `Offset::as_seconds()` | `i32` 整秒 |
| `Duration::as_nanos()` / `num_*` | 纳秒 / 更粗单位的跨度 |

## 方向性，直说

**迁入** tzcraft 是上文的受支持路径。**迁出**留给用户：tzcraft 不提供到 `chrono` / `time` / `rustix` 的转换实现，因为那些 crate 不在依赖图中。如果必须离开，上表反向读即可——两边的整数是一样的。

## 验证

`tests/chrono_parity.rs` 用真实 chrono 写法（构造器、strftime、checked 算术、编解码）在 tzcraft API 上验证迁移面，保证移植路径随 crate 演进始终保持诚实。
