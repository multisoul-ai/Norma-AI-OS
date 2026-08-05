# 独立开发者友好的用户与产品分析平台调研

> 调研日期：2026-07-28
>
> 目标产品：Norma OS（原生 macOS、Rust + GPUI）
>
> 调研范围：注册/匿名用户识别、Session 与使用时长、功能埋点、漏斗与留存、macOS/Rust 接入、价格、自托管、隐私和运维
>
> 资料原则：仅采用平台官方文档、官方价格页和官方代码仓库；价格与免费额度是易变信息，应在采购前再次核对

## 结论先行

### 推荐排序

1. **当前默认首选：PostHog Cloud**
   - [官方事实] 同时具备匿名用户、登录后 `identify`、Person Profile、自定义事件、漏斗、留存和 Session；有官方 Rust SDK、HTTP Capture API，以及支持 macOS 的官方 Swift SDK。
   - [官方事实] Product Analytics 每月前 100 万事件免费，Session Replay 每月前 5,000 次免费；按量付费从 $0 起，不要求先购买固定套餐。[官方价格页](https://posthog.com/pricing)
   - [判断] 对 Norma OS 而言，它是“产品分析完整度、Rust 接入成本、免费额度”最均衡的选择。需要由 Norma 自己记录前台活跃时长，并显式传递 `session_id`，不能把 Rust 服务端 SDK 当作桌面生命周期 SDK。

2. **隐私与准确时长优先：TelemetryDeck**
   - [官方事实] Swift SDK 原生支持 macOS；Duration Events 以毫秒计时并自动排除 App 在后台的时间；支持自定义事件、漏斗和留存。
   - [官方事实] `clientUser` 会在端侧加盐哈希、服务端再次哈希，可去重用户但平台无法还原真人身份；它不是可读的 CRM/CDP User Profile。
   - [判断] 如果只需“有多少注册用户在使用什么功能、用了多久”，且不需要在分析后台看到邮箱/姓名，TelemetryDeck 的隐私模型和时长能力很适合独立 macOS App。短板是没有官方 Rust SDK、不能自托管。

3. **需要可读 Profile 或自主托管：OpenPanel**
   - [官方事实] 官方同时提供 Swift、Rust 和 HTTP SDK，具有 Profile、Session、Funnels、Cohorts、Retention；云端起价低，自托管版免费且不限量。
   - [判断] 它是最贴近“开源 Mixpanel/PostHog”的候选，但 Rust SDK偏服务端语义，不会替桌面 App 自动处理前后台 Session；自托管还要维护 PostgreSQL、Redis、ClickHouse、worker 和升级备份。

4. **轻量自托管：Umami**
   - [官方事实] MIT 开源，自托管只需应用和 PostgreSQL；支持自定义事件、Distinct ID、Sessions、Funnels、Journeys、Retention 和直接 HTTP 上报。
   - [判断] 运维比 PostHog/OpenPanel 轻，但产品仍以 Web Analytics 为中心。Norma 需要自行封装 Rust HTTP 客户端、离线队列、Session 和活跃时长。

### 不建议作为 Norma 当前主方案

- **Mixpanel / Amplitude**：分析能力成熟、云端免费额度高，也有支持 macOS 的 Swift SDK，但没有官方 Rust SDK。用 HTTP 接入可行，桌面生命周期、批处理和重试需要自建。
- **Aptabase**：非常适合匿名、隐私优先的事件统计，但官方刻意不使用长期用户标识，因此不能稳定计算跨日唯一注册用户、MAU 或用户留存。
- **Firebase Analytics / GA4**：2026 年官方矩阵已经是 **macOS beta 支持**，不能再简单说“不支持 macOS”；但 Analytics 仍是闭源二进制 Apple SDK，而 Measurement Protocol 官方定位是补充 SDK、不是替代 SDK。对 Rust 原生桌面应用并不顺手。

## 先把三个指标定义清楚

### 1. 注册用户数

- [判断] **注册用户总数的唯一事实源应是账号/授权/支付后端，不应是分析平台。** 分析事件可能丢失、被用户拒绝、被广告拦截或在离线期延迟发送。
- 分析平台更适合回答：
  - 多少注册用户在某天或某月活跃；
  - 注册用户用了哪些功能；
  - 从注册到激活、留存、付费的转化情况。
- 没有账号体系时，随机生成的 `installation_id` 只能近似表示安装实例，不能等同于真实用户；重装和多设备都会造成偏差。

### 2. 使用时长

桌面 App 至少有三种不同口径：

| 口径 | 含义 | 是否适合 Norma |
|---|---|---|
| Session 墙钟时间 | Session 第一条事件到最后一条事件的间隔 | 适合粗略分析，后台闲置会虚高 |
| 前台时间 | App 处于 foreground/active 的累计时间 | 比较适合 |
| 前台活跃时间 | App 在前台，且近期有键鼠、命令或功能交互的累计时间 | 最接近真实使用，但要自行定义 |

[判断] Norma 应把产品主指标定义为“前台活跃时长”，由本地生命周期和最近交互共同计算。平台自带的 Session Duration 只作辅助口径。

### 3. 功能埋点

[判断] 应上报“功能发生了什么”，不上传用户内容。对终端和 Agent 产品尤其要禁止默认采集：

- 终端命令、输出、工作目录和文件名；
- Prompt、模型回复、剪贴板；
- API Key、License Key、邮箱等直接标识；
- Voice 音频或完整转写。

## 核心平台横向对比

| 平台 | 用户识别 / Profile | Session / 时长 | 事件 / 漏斗 / 留存 | Norma 接入路径 | 2026-07-28 免费与付费 | 自托管 | 结论 |
|---|---|---|---|---|---|---|---|
| **PostHog** | 匿名 ID、`identify`、Person Profile | iOS SDK 自动 Session；Rust 需显式 Session；有 `$session_duration` | 完整 | 官方 Rust；Swift/macOS；HTTP | 100 万事件/月免费；超额按量 | 可，但官方不提供支持且运维重 | **总体首选** |
| **TelemetryDeck** | 双重哈希 `clientUser`；无可读 Profile | Swift Duration Events 自动排除后台 | 完整 | Swift/macOS；HTTP；无 Rust SDK | 5 万事件/月免费；Indie €9/月含 75 万 | 不支持 | **隐私/时长首选** |
| **OpenPanel** | 匿名设备 ID、`profileId`、完整 Profile | 30 分钟无活动切 Session；Rust 需自行发生命周期 | 完整 | 官方 Rust、Swift/macOS、HTTP | 30 天试用；5,000 事件 $2.50/月起 | AGPL，免费不限量 | **开放性首选** |
| **Umami** | 默认匿名；可传 Distinct ID 与 Identify 数据 | 访问时长为首末事件差 | 完整 | HTTP；无官方 Rust/Swift SDK | Hobby 10 万事件/月免费；Pro $20/月 | MIT，较轻 | **轻量自托管首选** |
| **Mixpanel** | 匿名设备 ID、`identify`、User Profile | 查询时按事件计算，默认 30 分钟 | 完整 | Swift/macOS；HTTP；无 Rust SDK | 100 万事件/月免费；超额 $0.28/千事件 | 官方未提供 | 成熟，但接入不占优 |
| **Amplitude** | Device ID、User ID、User Profile | Swift 自动 Session；后台事件可能虚高 | 完整 | Swift/macOS；HTTP；无 Rust SDK | 200 万事件/月免费；Plus 从 $0 按量 | 官方未提供 | 免费额度大，Rust 接入一般 |
| **Aptabase** | 故意不保存长期用户 ID/Profile | 有 Session Timeline，首末事件差 | 自定义事件；不支持用户留存 | Swift/macOS；HTTP；无纯 Rust SDK | 2 万事件/月免费；20 万 $10/月 | AGPL，免费 | 不满足唯一用户/留存 |
| **Firebase Analytics** | User ID、User Properties | GA4 Session/参与度指标 | 事件、漏斗、Cohort/Retention | macOS beta Swift 二进制 SDK；HTTP 仅补充 | Analytics 免费 | 不支持 | 能用，但不适合 Rust-first |

> [判断] 表中的“官方未提供自托管方案”不等同于法律或技术上绝不可能自行搭建，而是官方产品和文档未给出受支持的自托管发行版，应按云服务评估。

## 平台逐项核对

### PostHog

**用户与事件**

- [官方事实] SDK 会先给匿名用户生成 distinct ID；用户注册或登录后调用 `identify`，可把匿名行为与稳定 ID 关联。Person Properties 可通过 `$set`、`$set_once` 写入 Profile。[iOS Identify 文档](https://posthog.com/docs/libraries/ios) · [Person Properties](https://posthog.com/docs/product-analytics/person-properties)
- [官方事实] 自定义事件可用于 [Funnels](https://posthog.com/docs/product-analytics/funnels) 和 [Retention](https://posthog.com/docs/product-analytics/retention)。

**Session 与时长**

- [官方事实] 移动 SDK 会自动发送 `$session_id`；默认连续 30 分钟无活动或 Session 最长 24 小时后新建 Session。服务端 SDK 默认不发送 `$session_id`，需要调用方自己生成。[Sessions 文档](https://posthog.com/docs/data/sessions)
- [判断] Norma 经 Rust SDK 上报时，应显式产生 `session_id` 和 `active_duration_ms`；否则 PostHog 的 Session 能分析事件分组，但无法准确表示原生 App 前台活跃时长。

**接入、价格与运维**

- [官方事实] 官方提供 [Rust SDK](https://github.com/PostHog/posthog-rs)、[Swift/iOS SDK](https://github.com/PostHog/posthog-ios) 和 [HTTP Capture API](https://posthog.com/docs/api/capture)。Swift 包声明支持 macOS 10.15+。
- [官方事实] Product Analytics 前 100 万事件/月免费，免费层还有 5,000 次 Session Replay；之后按事件量阶梯计价，最新单价应以[价格计算器](https://posthog.com/pricing)为准。
- [官方事实] 自托管使用 MIT 版 Docker Compose，但官方明确不提供保证和支持、只跟踪最新 master；参考最低机器约 4 vCPU、16 GB RAM、30 GB 存储，依赖 ClickHouse、Kafka、Redis、PostgreSQL、MinIO 等组件。[Self-host 文档](https://posthog.com/docs/self-host)
- [判断] 独立开发者应优先使用 PostHog Cloud 的 EU 或 US 区域，不建议为了少量事件承担 PostHog 自托管运维。

### TelemetryDeck

**用户与隐私**

- [官方事实] `clientUser` 可以来自内部用户 ID、邮箱或自动 ID，但 SDK 在发送前使用 app-specific salt 哈希，服务器再哈希；相同原始 ID 和 salt 可跨设备合并，TelemetryDeck 不能还原身份。[匿名化原理](https://telemetrydeck.com/docs/articles/anonymization-how-it-works/) · [Signals Reference](https://telemetrydeck.com/docs/api/signals-reference/)
- [判断] 可把注册后的内部 UUID 设为 `clientUser`，统计唯一活跃注册用户；不要直接发送邮箱。后台看到的是不可逆匿名用户，不是可编辑 User Profile。

**时长与分析**

- [官方事实] Swift SDK 初始化后自动发送 `TelemetryDeck.Session.started`，并跟踪 Session、启动和设备信息。[Swift Setup](https://telemetrydeck.com/docs/guides/swift-setup/)
- [官方事实] Duration Events 以毫秒记录，App 进入后台会停止计时、回前台后继续；生成的事件可以进入漏斗。[Duration Events](https://telemetrydeck.com/docs/articles/duration-events/)
- [官方事实] 平台提供 [Funnel Insights](https://telemetrydeck.com/docs/articles/how-to-funnel-insights/)、[TQL Funnel](https://telemetrydeck.com/docs/tql/funnel/) 和 [Retention](https://telemetrydeck.com/docs/tql/retention/)。

**接入、价格与运维**

- [官方事实] Swift SDK 支持 iOS、macOS、watchOS 和 tvOS；没有官方 Rust SDK，Rust 可使用 [Ingest API v2](https://telemetrydeck.com/docs/ingest/v2/) 上报 `appID`、哈希后的 `clientUser`、`type`、`sessionID` 和 payload。
- [官方事实] 2026-07-01 起新免费账号为 5 万事件/月；Indie 为 €9/月含 75 万事件、24 个月留存和无限 App，年付 €90。免费层查询隔几小时更新，超额的新事件会丢弃。[2026 价格更新](https://telemetrydeck.com/blog/pricing-update-2026/) · [FAQ](https://telemetrydeck.com/faq/)
- [官方事实] 不提供自托管，数据托管在欧盟，主要位于德国纽伦堡 Hetzner，且不保存 IP。[Hosting](https://telemetrydeck.com/docs/articles/hosting-solutions/) · [Privacy FAQ](https://telemetrydeck.com/docs/guides/privacy-faq/)

### OpenPanel

**能力与接入**

- [官方事实] Profile 以 `profileId` 关联跨设备行为；不登录时可使用设备 ID。平台提供 Profiles、Sessions、Funnels、Cohorts 和 Retention。[How it works](https://openpanel.dev/docs/how-it-works) · [官方仓库](https://github.com/Openpanel-dev/openpanel)
- [官方事实] Session 默认在 30 分钟无活动后结束；Overview 的平均时长只统计 duration 大于 0 的 Session。[Overview](https://openpanel.dev/docs/dashboard/understand-the-overview)
- [官方事实] 官方提供 [Rust SDK](https://openpanel.dev/docs/sdks/rust)、[Swift SDK](https://openpanel.dev/docs/sdks/swift) 和 [Track HTTP API](https://openpanel.dev/docs/api/track)。Swift SDK 支持 macOS 10.15+，并可自动发 App 生命周期事件。
- [判断] Rust SDK的文档语义更接近服务端事件发送器。Norma 应自己发送 `app_session_started`、`app_foregrounded`、`app_backgrounded`、heartbeat 和 duration，而不是期待 SDK 自动知道 GPUI 窗口状态。

**价格与自托管**

- [官方事实] Cloud 当前提供 30 天免信用卡试用，没有永久免费 Cloud 套餐；5,000 事件 $2.50/月、10,000 事件 $5/月、100,000 事件 $20/月、100 万事件 $90/月，均含无限 App、用户和 Dashboard。[价格页](https://openpanel.dev/pricing)
- [官方事实] 自托管版为 AGPL-3.0，免费不限事件；Docker Compose 包含 PostgreSQL、Redis、ClickHouse、API、Dashboard 和 worker，最低约 2 GB RAM、推荐 4 GB。[Docker Compose 部署](https://openpanel.dev/docs/self-hosting/deploy-docker-compose)
- [判断] Cloud 的低付费起点对独立开发者友好；只有在数据主权是硬要求时再自托管。

### Umami

**能力与接入**

- [官方事实] 默认以 IP、User-Agent 和 Website ID 生成不依赖 Cookie 的匿名 Session 标识；也可传 Distinct ID，把多个 Session 关联为同一访客，并发送 Identify 属性。[Distinct IDs](https://docs.umami.is/docs/distinct-ids) · [Sessions](https://docs.umami.is/docs/sessions)
- [官方事实] 支持自定义事件、[Funnels](https://docs.umami.is/docs/funnel) 和 [Retention](https://docs.umami.is/docs/retention)。[Sending Stats API](https://docs.umami.is/docs/api/sending-stats) 可直接发送 event、identify 和 performance。
- [官方事实] Visit Duration 是同一次访问第一条和最后一条 Pageview/Event 的时间差，而且只有多于一个事件的访问才有有效时长。[Metric Definitions](https://docs.umami.is/docs/metric-definitions)
- [判断] Norma 不能用这个 Visit Duration 直接代表使用时长，应发送显式前后台事件和累计活跃秒数。

**价格与自托管**

- [官方事实] Umami Cloud Hobby 免费层包含 10 万事件/月、1 个网站和 6 个月数据留存；Pro 为 $20/月、100 万事件/月、20 个网站、2 年留存。[价格页](https://umami.is/pricing) · [Cloud FAQ](https://docs.umami.is/docs/cloud/faq)
- [官方事实] 官方 Integrations 没有原生 Swift 或 Rust SDK；Rust 需要薄封装 HTTP API。[Integrations](https://docs.umami.is/docs/integrations)
- [官方事实] [官方仓库](https://github.com/umami-software/umami)使用 MIT License，自托管要求 Node.js 18.18+ 和 PostgreSQL 12.14+，官方也提供应用 + PostgreSQL 的 Docker Compose。[安装文档](https://docs.umami.is/docs/install)
- [判断] 如果团队愿意维护一个数据库和一个应用实例，Umami 是本次候选里最轻的自托管方案。

### Mixpanel

- [官方事实] 匿名设备 ID 可以在注册后通过 `identify` 与用户关联；User Profile 可保存用户属性。[Identity Management](https://docs.mixpanel.com/docs/tracking-methods/id-management/identifying-users-original) · [User Profiles](https://docs.mixpanel.com/docs/data-structure/user-profiles)
- [官方事实] Session 在查询时按事件计算，默认 30 分钟超时，也支持基于特定开始/结束事件或属性定义；分析中可使用 Session Duration。[Sessions](https://docs.mixpanel.com/docs/features/sessions)
- [官方事实] Free 套餐每月 100 万事件、5 个保存报告和 10,000 次 Replay；Growth 前 100 万事件免费，之后从每千事件 $0.28 起，并含无限保存报告。精确阶梯价以[价格计算器](https://mixpanel.com/pricing/)为准。
- [官方事实] 官方 [Swift SDK](https://docs.mixpanel.com/docs/tracking-methods/sdks/swift) 的 Swift Package 支持 macOS 10.13+；无官方 Rust SDK，可用 [Track Event API](https://docs.mixpanel.com/reference/track-event)。
- [判断] Mixpanel 很适合有专职产品分析需求的团队；Norma 当前通过 HTTP 自建桌面生命周期适配的收益不如直接使用 PostHog Rust SDK。

### Amplitude

- [官方事实] Device ID、User ID 和 Amplitude ID 用于把匿名、登录后以及跨设备行为合并；可附加 User Properties。[Unique Users](https://amplitude.com/docs/data/sources/instrument-track-unique-users)
- [官方事实] Swift SDK可自动处理 Session 和生命周期，但官方提醒后台事件可能夸大前台时间，且 App 未再次打开时可能缺少 Session End。[Swift SDK](https://amplitude.com/docs/sdks/analytics/ios/ios-swift-sdk) · [Sessions](https://amplitude.com/docs/data/sources/instrument-track-sessions)
- [官方事实] 官方 [Amplitude-Swift](https://github.com/amplitude/Amplitude-Swift) 支持 macOS 10.15+；无官方 Rust SDK，可用 [HTTP API V2](https://amplitude.com/docs/apis/analytics/http-v2)。
- [官方事实] Free 套餐无需信用卡，包含每月 200 万事件、Basic Product Analytics 和 10,000 次 Replay；Plus 前 200 万事件免费，并增加自定义分析能力和 2 年留存，之后按事件量计费。[价格页](https://amplitude.com/pricing)
- [判断] 免费事件额度是本次完整产品分析候选中最高的，但 Rust 桌面端仍要自行补 Session、队列和失败重试。

### Aptabase

- [官方事实] Aptabase 明确不使用设备标识、Cookie、指纹或长期用户 ID，因此不能计算 MAU 和用户级 Retention。[价格/FAQ](https://aptabase.com/#pricing) · [隐私政策](https://aptabase.com/legal/privacy)
- [官方事实] 支持自定义事件和 Session Timeline；官方 ClickHouse 查询以同一 Session 最后事件时间减去第一事件时间作为时长。[Session 查询源码](https://github.com/aptabase/aptabase/blob/main/etc/clickhouse/queries/historical_sessions__v1.liquid)
- [官方事实] 有官方 [Swift/macOS SDK](https://github.com/aptabase/aptabase-swift) 和 [Tauri Rust 插件](https://github.com/aptabase/tauri-plugin-aptabase)。纯 Rust 可按官方 [SDK 协议](https://github.com/aptabase/aptabase/wiki/How-to-build-your-own-SDK) POST 到 `/api/v0/events`。
- [官方事实] 免费层 2 万事件/月；20 万 $10/月、100 万 $20/月、200 万 $40/月，超限不会收费但会暂停采集到下月。[价格页](https://aptabase.com/#pricing)
- [官方事实] 主服务 AGPLv3、SDK MIT；[自托管仓库](https://github.com/aptabase/self-hosting)包含应用、PostgreSQL 15 和 ClickHouse。
- [判断] 它适合只看匿名功能采用率，不适合本题要求的“唯一注册用户 + 跨日用户留存”，不应作为主方案。

### Firebase Analytics / Google Analytics 4

**当前 macOS 支持状态**

- [官方事实] Firebase 2026-07-20 更新的 Apple 平台支持矩阵，把 macOS 标为 beta 支持；Analytics 从 Firebase Apple SDK 8.9.0 起可用于 macOS。[Apple 平台支持矩阵](https://firebase.google.com/docs/ios/learn-more)
- [官方事实] [firebase-ios-sdk 官方仓库](https://github.com/firebase/firebase-ios-sdk)包含大部分 Apple SDK 源码，但 Analytics 是例外，仍以闭源二进制分发。

**分析能力与 Rust 适配**

- [官方事实] Analytics 可以设置 [User ID](https://firebase.google.com/docs/analytics/userid)，但禁止用邮箱等 PII；GA4 Explorations 提供 [Funnel](https://support.google.com/analytics/answer/9327974) 和 [Cohort/Retention](https://support.google.com/analytics/answer/9670133)。
- [官方事实] Firebase Analytics 本身是[无费用产品](https://firebase.google.com/pricing)。
- [官方事实] [GA4 Measurement Protocol](https://developers.google.com/analytics/devguides/collection/protocol/ga4)允许 HTTP 发送事件，但官方明确定位为补充 Firebase SDK 或 gtag，而不是取代标准埋点；只用 Measurement Protocol 无法完整使用全部能力。
- [判断] 如果 Norma 已深度使用 Firebase，可通过 Swift/Objective-C bridge 接入。对于当前 Rust-first 桌面架构，引入闭源 beta SDK bridge 或使用“仅补充用途”的 HTTP 协议都不如 PostHog/OpenPanel 直接。

## 给 Norma OS 的建议方案

### 方案 A：PostHog Cloud + Provider-neutral Rust Adapter

这是当前最推荐的起步方案。

1. 在 `agent`、`runtime`、`terminal` 等域层只定义稳定的分析事件，不依赖 PostHog 类型。
2. 在负责副作用的上层实现 provider-neutral adapter、磁盘队列、批量发送和失败重试。
3. 使用 PostHog Rust SDK或 HTTP Capture API发送事件。
4. 明确发送 Norma 自己生成的 `session_id`、`active_duration_ms` 和生命周期事件。
5. 日后如隐私或成本要求变化，可把 adapter 切到 TelemetryDeck、OpenPanel 或自建接收端。

[判断] 这能避免产品逻辑被某家平台的事件模型锁定，也符合本仓库“逻辑沉底、适配上浮”和 GPUI 前台禁阻塞的约束。

### 建议的最小事件模型

| 事件 | 关键属性 | 用途 |
|---|---|---|
| `app_session_started` | `session_id`, `app_version`, `os_version` | Session 和版本分布 |
| `app_foregrounded` | `session_id` | 前台开始 |
| `app_backgrounded` | `session_id`, `foreground_ms`, `active_ms` | 准确时长 |
| `feature_used` | `feature`, `surface`, `success` | 通用功能采用率 |
| `terminal_node_created` | `session_id`, `source` | Terminal 激活 |
| `agent_task_started` | `provider_family`, `entry_point` | Agent 漏斗起点 |
| `agent_task_completed` | `success`, `duration_bucket` | 完成率与耗时 |
| `voice_input_started` | `provider_kind` | Voice 采用率 |
| `onboarding_step_completed` | `step` | 新用户漏斗 |
| `analytics_opted_out` | 不带用户内容 | 隐私选择趋势 |

事件属性只允许枚举、布尔值、版本和桶化数值。不得把命令、Prompt、路径、文件名、模型回复或音频塞进属性。

### 身份策略

1. 首次启动生成随机 `anonymous_id`，保存在本地。
2. 用户完成注册/授权后，从账号后端取得稳定内部 UUID。
3. 上报前用服务端持有的 secret 对 UUID 做 HMAC，产生 `analytics_user_id`；不要直接使用邮箱、License Key 或支付平台 Customer ID。
4. 调用 PostHog `identify(analytics_user_id)` 合并注册前后的行为。
5. 注册总数仍从授权后端查询；PostHog 只计算活跃、采用、漏斗和留存。
6. 退出登录或切换账号时调用 reset，避免同一台 Mac 上串联不同用户。

### 活跃时长策略

1. App 进入前台时开始累计 `foreground_ms`。
2. 只有最近 60 秒内发生键盘、鼠标、节点交互或明确功能事件时，才累计 `active_ms`。
3. App 进入后台、休眠或退出时立即结算本地计时。
4. 可每 60 秒写入本地内存/轻量队列，但不要同步访问网络或 SQLite 热路径。
5. Session 结束事件可能来不及发送，因此用本地持久化队列在下次启动补发；服务端分析必须容忍缺少显式结束事件。

### 隐私与工程护栏

- Settings 提供清晰的 Analytics 开关，并说明采集事件类别、用途、区域和保留期限。
- 默认不启用 Session Replay；终端、Prompt 和语音产品很容易在 Replay 中泄露敏感内容。
- 发送在后台执行；`Runtime::drain_all`、PTY output chunk、鼠标移动和输入路由热路径只更新内存计数，不做网络或同步存储。
- 建立 allowlist schema；未知属性拒绝发送，避免开发者误传自由文本。
- 开发、测试、预发布、生产使用不同 project/environment，测试数据不进入生产指标。
- 在 Privacy Policy 中写明子处理者、区域、退出方式、删除请求路径和数据保留期。

## 按场景选择

| 场景 | 推荐 |
|---|---|
| 想最快获得完整产品分析，且希望直接用 Rust | **PostHog Cloud** |
| 只需匿名注册用户去重，特别看重准确前台时长和欧盟隐私 | **TelemetryDeck** |
| 需要可读 User Profile，或未来确定要自主托管 | **OpenPanel** |
| 只想维护轻量开源统计服务，能接受自己写桌面 Session | **Umami** |
| 团队已有成熟数据分析人员，偏好业界成熟 SaaS | Mixpanel 或 Amplitude |
| 只做匿名事件计数，不需要跨日用户留存 | Aptabase |
| 产品本来就使用 Firebase Apple SDK，愿意接受 macOS beta | Firebase Analytics |

## 最终建议

[判断] Norma OS 可以先用 **PostHog Cloud** 做一个小规模、可退出的验证：

- 只接入 8–10 个最小事件；
- 不启用 Session Replay；
- 不采集用户内容；
- 注册总数以授权后端为准；
- Norma 自己计算前台活跃时长；
- 使用 provider-neutral adapter，保留切换 TelemetryDeck/OpenPanel 的能力；
- 运行 4 周后再根据月事件量、实际查询需求、用户隐私反馈和数据价值决定是否长期保留。

如果团队认为“分析后台绝不能持有可读用户 Profile”是硬要求，则把默认首选替换为 **TelemetryDeck**；如果“必须自托管”是硬要求，则优先 **OpenPanel**，事件量小且只需轻量分析时选 **Umami**。
