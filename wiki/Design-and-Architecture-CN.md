# 设计与架构

## 四大设计前提

tzcraft 建立在四个决策之上，每一条都是可验证的代码事实，不是口号。

### 1. 一根时间轴，一个算术源头

`Ticks` 是唯一的瞬时类型：自 `1970-01-01T00:00:00Z`（投影公历）起的有符号 128 位纳秒计数。这个宽度本身就是设计：

- **精度**：每个可表示的瞬时都具备纳秒分辨率。
- **量程**：`i128` 纳秒覆盖约 ±2920 亿年。不存在"小瞬时/大瞬时"两套类型，没有溢出塌缩策略，也没有需要记住的第二类型。

`Duration` 是独立的有符号跨度类型（同样是 `i128` 纳秒，转换零成本）。因为类型不同，`Ticks + Ticks` 编译不过——类型系统直接拒绝把两个瞬时相加。

### 2. 公历类型是投影，不是主人

`Date`（纪元起 `i32` 天）、`TimeOfDay`（当日 `u64` 纳秒）和 `CivilDateTime`（`Date` + `TimeOfDay`）**不持有自己的算术**。跨午夜的加法、跨年的进位全部投影到那根 `i128` 纳秒轴上，在时间线投影代码（`calendar::ns_divmod_day` 等）里一次性算完。

月份/年份这类日历语义运算只存在于一处：投影 → 调整 → 再投影。这消除了 `chrono` 必须维护的那一整片 `Add` impl 矩阵。

### 3. 编译器就是日历

闰年规则、日数与公历互转（Hinnant 算法）、星期、ISO 周——全部是 `const fn`。`const` 日期、静态时区表、ISO 周数组都在编译期被编译器折叠。

时区同样是 `const` 数据。`Zone` 只有两种：

- `Zone::Utc`，或
- `Zone::Fixed(Offset)`，其中 `Offset` 是开区间 `(-24h, 24h)` 内的有符号整秒位移。

`Zoned` 是 `Ticks + Zone`，作为值内联携带。没有全局注册表、没有可变的"当前时区"、没有隐藏上下文——因此 `Zoned` 天然 `Copy` + `Send` + `Sync`。

### 4. 编解码器自己挑线格式

每个类型只实现一次 `nextjson` 的格式中立契约（`NsonSchema` + `NsonSerialize` + `NsonDeserialize`）。编码时问一句 `is_human_readable()`：

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

一套实现、两种线格式、零特性开关。

## 类型模型

```text
                      ┌────────────────────────────┐
                      │  Ticks (i128 ns 自纪元起)     │  ← 唯一的算术拥有者
                      └─────────────┬──────────────┘
                                    │ 投影
                    ┌───────────────┴───────────────┐
                    ▼                               ▼
        CivilDateTime (Date + TimeOfDay)      Zoned (Ticks + Zone)
        ┌──────────────┴─────────────┐            │
        ▼                            ▼            ▼
     Date (i32 天)             TimeOfDay (u64 纳秒)  Zone (Utc | Fixed(Offset))
```

依赖流向把纯数学放在叶子层：

- `calendar.rs` —— 纯 `const fn` 公历数学，无依赖。
- `write.rs` —— 免分配的 `Write` trait 与 `Buf` 输出端，只依赖错误类型。
- `units.rs`、`date.rs`、`time.rs` —— 基于 `calendar` 的公历投影。
- `ticks.rs`、`datetime.rs`、`zoned.rs` —— 时间线及其投影。
- `format.rs`、`strftime.rs` —— 在类型之上的解析与格式化。
- `codec.rs`、`binary.rs` —— 线格式（特性门控）。
- `migration.rs` —— 记录从 `chrono` / `time` / `rustix` 迁入 `tzcraft` 的单向路径（不依赖那些 crate）。

`Ticks ↔ CivilDateTime` 与 `Ticks ↔ Zoned` 是刻意双向的：这正是前提 2 的体现，投影算术集中在 `calendar.rs` 一处，而不是散落在各模块里。

## 模块布局

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

## 刻意不做的事

- **没有 IANA 时区数据库，也没有 DST。** `Zone` 只有 `Utc` 和固定偏移。如果墙钟要跟随真实切换，请用自己的策略解析出偏移，再把 `Zone::Fixed` 交给库。这条缝是刻意收窄的，也为将来加 `Zone::Database` 变体预留了空间。
- **只有投影公历**（公元 0 年 = 公元前 1 年）。没有儒略历、希伯来历等其它历法。
- **严格实现 ISO 8601 / RFC 3339 / RFC 2822，外加完整 strftime 引擎。** 交付的都是完整且经过测试的实现，没有半成品模板引擎。
- **没有 `unsafe`。** 全 crate 开启 `#![deny(unsafe_code)]`。
