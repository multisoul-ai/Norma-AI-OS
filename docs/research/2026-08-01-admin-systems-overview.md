# Norma OS 后台系统、管理面板与统计能力全景调研

> 调研日期：2026-08-01
> 调研范围：当前源码、产品 SPEC、设计文档、ADR、历史 Superpowers SDD/计划、研究文档、原型/图片文件名与可运行 spike。
> 证据原则：项目状态只以本仓库为准；Keygate 的“平台原生能力”额外按仓库钉死的上游 `v0.1.1`（commit `40f59cd`）核对，并与 Norma 当前接入严格分开。
> 报告口径：这里的“后台”指运营者或开发者跨用户管理/统计的系统；终端用户本机 Settings、License 激活壳、诊断日志单独列为边界项，不冒充运营后台。

## 1. 结论摘要

### 1.1 最重要的结论

1. **调研时唯一已经进入实际系统链路的业务后台是 Keygate License 管理后台。** 仓库有一个已通过 GO 验证的本机 Docker spike，并把激活请求指向生产 Keygate 地址。Keygate 后台服务器源码本身没有 vendored 到 Norma 仓库；本机环境从上游 `v0.1.1` 构建。2026-08-11 起 App 默认关闭 License 硬门控，发行打包也显式禁用默认 features；Keygate 代码仅作显式 `licenses` feature 的可选链路保留。[spike 结论](../../spikes/keygate-mvp/README.md#L30-L59) · [上游版本 pin](../../spikes/keygate-mvp/docker-compose.yml#L1-L20) · [License feature](../../crates/app/Cargo.toml#L8-L14) · [生产地址约束](../../crates/app/src/license_gate/config.rs#L126-L145)
2. **Keygate 平台原生能力明显强于 Norma 当前使用范围。** 上游后台原生具备 products、plans、licenses、customers/users、usage metering、analytics、audit logs 等管理页面和 API，也能统计 License 状态、激活数、席位数、激活趋势、功能用量和按 License 去重的 `unique_users`；但 Norma App 当前只调用 `/api/v1/license/activate`，spike 也只硬验证 License 列表与 `stats.total_licenses`。因此不能把上游已有 usage/analytics 页面写成“Norma 已有功能使用数据”。[Keygate 上游功能总览](https://github.com/tabloy/keygate/blob/40f59cd518b46a8b61830e7834567ab31dfa42a8/README.md#L49-L89) · [上游管理/分析路由](https://github.com/tabloy/keygate/blob/40f59cd518b46a8b61830e7834567ab31dfa42a8/cmd/server/main.go#L820-L902) · [Norma 激活调用](../../crates/app/src/license_gate/activate_http.rs#L40-L105) · [spike 实际断言](../../spikes/keygate-mvp/scripts/check.sh#L26-L46)
3. **目前没有产品级用户注册系统。** License 激活设计明确允许用户“不创建账号”而用 Key 激活；当前客户端请求只包含 `license_key`、机器标识、标识类型和标签，不包含用户邮箱、姓名或账号 ID。产品 SQLite 也没有 `users`、`accounts`、`registrations` 或 `analytics_events` 表。[无账号激活用户故事](../product-specs/2026-07-14-SPEC-software-licensing.md#L17-L20) · [激活 DTO](../../crates/license/src/activate.rs#L1-L18) · [当前 SQLite 表](../../crates/storage/src/migrations.rs#L26-L55) [续](../../crates/storage/src/migrations.rs#L57-L169)
4. **目前没有产品使用分析接入。** 2026-07-28 的调研推荐 PostHog Cloud，并设计了 `app_session_started`、`feature_used`、Terminal/Agent/Voice 等最小事件，但这些仍是建议；Cargo 依赖、源码和 SQLite schema 中都没有 PostHog 或任何其他候选平台的 adapter、发送队列、事件表或 Analytics Settings 开关。[建议架构](2026-07-28-independent-developer-product-analytics-platforms.md#L196-L208) · [建议事件而非实现](2026-07-28-independent-developer-product-analytics-platforms.md#L210-L234) · [当前 workspace 依赖](../../Cargo.toml#L30-L80)
5. **目前没有 Norma 社区模块，也没有社区活跃度统计。** 仓库不存在帖子、评论、群组、社区成员或社区事件模型；提议的最小分析事件也没有 community/post/comment/member 事件。Keygate 上游 README 中的 Discussions/Issues 是 Keygate 自身开源社区入口，不是 Norma 的社区功能。[建议事件清单](2026-07-28-independent-developer-product-analytics-platforms.md#L210-L223) · [Keygate 自身社区](https://github.com/tabloy/keygate/blob/40f59cd518b46a8b61830e7834567ab31dfa42a8/README.md#L144-L153)

### 1.2 当前能力一句话画像

> Norma OS 已经有“**License 运营后台 + 桌面激活客户端**”，但还没有“**账号注册后台 + 产品行为分析后台 + 社区运营后台**”。

## 2. 状态定义与调研方法

### 2.1 状态定义

| 状态 | 含义 |
|---|---|
| **已实现 / 已接入** | 当前源码或可运行环境已存在，并有自动化/脚本证据 |
| **部分实现** | 平台或局部链路存在，但数据采集、产品化、部署或 UI 验收不完整 |
| **仅文档/调研** | 只在研究、SPEC、设计或候选方案中出现，没有当前代码接入 |
| **备选/已延后** | 明确列为 fallback、未来阶段或 out of scope |
| **不存在** | 全仓没有对应领域模型、采集链路或管理界面 |

### 2.2 覆盖范围

本次检索覆盖：

- `crates/**`、根 `Cargo.toml` 与 `Cargo.lock`；
- `docs/product-specs/**`、`docs/design-docs/**`、`docs/adr/**`、`docs/research/**`、`docs/exec-plans/**`；
- `.superpowers/**` 与 `docs/superpowers/**` 历史 SDD/计划；
- `spikes/**`、`scripts/**`、`README.md`、`ARCHITECTURE.md`、`CONTEXT.md`；
- `docs/**/*.html` 以及图片/原型资产文件名。

关键词包括 `admin/dashboard/portal/analytics/telemetry/registration/signup/user profile/feature_used/community/DAU/WAU/MAU/retention` 及对应中文。历史 SDD 没有发现另一套业务后台；命中主要集中在 Keygate 商业化链路和产品分析平台调研。

## 3. 后台与管理面板总清单

| 系统 / 面板 | 在 Norma 中的状态 | 真实用途 | 是否是当前运营后台 |
|---|---|---|---|
| **Keygate Admin Dashboard** | **已运行过本机 spike；生产激活客户端已接入；后台高级统计未被 Norma 使用** | License、客户、激活、用量及商业化管理 | **是，当前唯一业务后台** |
| **PostHog Cloud** | **仅调研，推荐首选** | 用户识别、事件、漏斗、留存、Session | 否，未接入 |
| **TelemetryDeck** | 仅调研，隐私/时长候选 | 匿名用户、Duration、漏斗、留存 | 否，未接入 |
| **OpenPanel** | 仅调研，Profile/自托管候选 | Profile、Session、Funnels、Cohorts、Retention | 否，未接入 |
| **Umami** | 仅调研，轻量自托管候选 | Distinct ID、事件、漏斗、Retention | 否，未接入 |
| **Mixpanel** | 仅调研，不建议当前主选 | User Profile、Session、漏斗、留存 | 否，未接入 |
| **Amplitude** | 仅调研，不建议当前主选 | User ID/Profile、Session、分析 | 否，未接入 |
| **Aptabase** | 仅调研，明确不满足唯一用户/留存 | 匿名事件与 Session Timeline | 否，未接入 |
| **Firebase Analytics / GA4** | 仅调研，不适合当前 Rust-first 架构 | User ID/Properties、事件、漏斗、Cohort | 否，未接入 |
| **Lemon Squeezy 商户后台** | **仅非技术调研；商业化路径已延后** | 商品、Checkout、收款、结算 | 否，未接入 |
| **Keygen CE + 可补 Portal** | **失败备选，Keygate GO 后未采用** | 备选 License 核心 | 否 |
| **Feature Guide / Help Center 指标方案** | **仅产品设计草案** | 导览完成/跳过、主题查看/退出、首次 Agent/Voice 派发等成功指标 | 否，没有采集后台 |
| **Norma Settings 面板** | 已实现 | 单机用户配置 Themes/Keyboard/Voice/Agents/Orchestration/Wallpaper | **否，是用户侧本机设置** |
| **本地性能诊断/日志** | 日志为基线能力；`performance_monitor` 是当前工作树未跟踪实现，不代表已提交基线 | 单进程 CPU/内存/wakeup/repaint/render 诊断 | **否，是开发诊断** |

产品分析八个平台的能力与接入路径来自同一份项目内调研对比；该文档自己也把它们写成“推荐排序”和“建议方案”，而不是已接入清单。[推荐排序](2026-07-28-independent-developer-product-analytics-platforms.md#L11-L37) · [平台对比](2026-07-28-independent-developer-product-analytics-platforms.md#L71-L84)

## 4. Keygate License 管理后台

### 4.1 当前状态：后台存在，Norma 只使用了很窄的一段

当前链路可分成三层：

```text
Keygate v0.1.1 平台原生能力
  ├─ Admin Dashboard / Customers / Analytics / Usage / Audit / Team / Payments ...
  └─ Public license APIs: activate / verify / usage / ...
             │
             │ Norma 当前只调用 activate
             ▼
Norma Rust License 客户端
  ├─ License Key + machine id → /api/v1/license/activate
  ├─ 保存 offline token
  └─ Ed25519 本地验签 → 进入 Workbench
```

- 本机 spike 使用 Docker Compose，从 GitHub `tabloy/keygate@v0.1.1` 构建 Keygate，并使用 PostgreSQL 16；端口为 `9000`，开发环境可 `dev-login`。[Compose](../../spikes/keygate-mvp/docker-compose.yml#L1-L46)
- spike 的 GO 证据是：能启动、自助/脚本发买断 License、列出 License、读取 admin stats、在线激活并在断网状态本地验签。[README](../../spikes/keygate-mvp/README.md#L7-L15) [结论](../../spikes/keygate-mvp/README.md#L34-L47)
- 2026-08-11 起 App 的 `licenses` feature 默认关闭，启动决策会在读取本地凭证前直接进入工作台；发行打包同时显式使用 `--no-default-features`。只有显式使用 `--features licenses` 的专用构建才会无凭证显示激活壳。[feature](../../crates/app/Cargo.toml#L8-L14) · [App 启动](../../crates/app/src/lib.rs#L65-L86) · [E2E 证据](../../crates/e2e/src/e2e_17_license_gate_startup.rs#L6-L39)
- 当前发行客户端的嵌入地址被测试锁定为 `https://soulvoiceai.com/keygate`，说明激活链路不再只是 localhost spike；但仓库没有该线上 Keygate 的部署脚本、数据快照或生产运维面板验收记录。[生产 URL 测试](../../crates/app/src/license_gate/config.rs#L126-L145)

因此准确状态应写成：

> **Keygate License 体系已接入；Admin 基础能力已由本机 spike 验证；生产后台的实际数据量、可用性和高级 Analytics 是否启用，仓库没有证据。**

### 4.2 Keygate 平台原生支持什么

仓库 pin 的上游 `v0.1.1` 原生后台包含：

- Products、Plans、Licenses、Customers/Users、API Keys、Webhooks、Analytics、Audit Logs、Team、Email Templates、Branding，以及搜索、筛选和 CSV/JSON 导出。[上游 README](https://github.com/tabloy/keygate/blob/40f59cd518b46a8b61830e7834567ab31dfa42a8/README.md#L75-L89)
- License 生命周期：创建、查看、导出、退款、撤销、暂停、恢复、换 Plan；还能查看每个 License 的 usage、seats、addons 与 floating sessions。[上游路由](https://github.com/tabloy/keygate/blob/40f59cd518b46a8b61830e7834567ab31dfa42a8/cmd/server/main.go#L839-L858)
- 用户列表与用户详情：`/admin/users`、`/admin/users/:id`；用户实体有 `id/email/name/avatar/role/created_at/updated_at`，详情关联 Licenses、Subscriptions、总用量、活跃席位、激活数和近期审计日志。[上游路由](https://github.com/tabloy/keygate/blob/40f59cd518b46a8b61830e7834567ab31dfa42a8/cmd/server/main.go#L893-L902) · [User 类型](https://github.com/tabloy/keygate/blob/40f59cd518b46a8b61830e7834567ab31dfa42a8/web/src/lib/api.ts#L456-L466) · [UserDetail](https://github.com/tabloy/keygate/blob/40f59cd518b46a8b61830e7834567ab31dfa42a8/web/src/lib/api.ts#L827-L835)
- License/激活统计：总 License、active/trialing/expired/canceled/suspended/revoked/past_due 状态数、总激活数、总席位、平均每 License 激活数。[上游 AnalyticsSummary](https://github.com/tabloy/keygate/blob/40f59cd518b46a8b61830e7834567ab31dfa42a8/internal/store/analytics.go#L55-L67) · [查询实现](https://github.com/tabloy/keygate/blob/40f59cd518b46a8b61830e7834567ab31dfa42a8/internal/store/analytics.go#L255-L319)
- 分布与趋势：按 License 状态、Plan、License 类型 breakdown，以及按日期统计 activation trend。[上游 breakdown](https://github.com/tabloy/keygate/blob/40f59cd518b46a8b61830e7834567ab31dfa42a8/internal/store/analytics.go#L322-L368) · [激活趋势路由](https://github.com/tabloy/keygate/blob/40f59cd518b46a8b61830e7834567ab31dfa42a8/cmd/server/main.go#L893-L898)
- 功能用量：public `/license/usage` 可以记录使用事件，Admin 的 `usage-top` 按 `feature` 聚合 `total_usage`，并以 `COUNT(DISTINCT license_id)` 计算 `unique_users`。这里的“unique user”实际是去重 License，不一定等于真人账号。[上游 public usage API](https://github.com/tabloy/keygate/blob/40f59cd518b46a8b61830e7834567ab31dfa42a8/cmd/server/main.go#L461-L489) · [聚合实现](https://github.com/tabloy/keygate/blob/40f59cd518b46a8b61830e7834567ab31dfa42a8/internal/store/analytics.go#L370-L398)

### 4.3 Norma 当前真正使用了什么

| 能力 | 平台原生可做 | Norma 当前有数据/接入吗 | 证据 |
|---|---:|---:|---|
| 手工创建客户 License | 是 | **是，spike 已验证** | bootstrap 以 `email/plan/product/notes` 创建 License。[脚本](../../spikes/keygate-mvp/scripts/bootstrap.sh#L136-L156) |
| License 列表 | 是 | **是，spike 已验证至少 1 行** | [硬断言](../../spikes/keygate-mvp/scripts/check.sh#L26-L39) |
| 总 License 数 | 是 | **是，spike 已验证 `total_licenses >= 1`** | [硬断言](../../spikes/keygate-mvp/scripts/check.sh#L41-L46) |
| 客户/用户详情页 | 是 | **平台有；Norma 没有账号同步或 App 查询** | Norma 激活请求无用户字段。[DTO](../../crates/license/src/activate.rs#L3-L18) |
| 注册量 | 平台可列 User，也可统计 License/激活 | **没有 Norma 注册事件；只能把 License/激活当商业代理指标** | License 设计允许无账号激活。[SPEC](../product-specs/2026-07-14-SPEC-software-licensing.md#L17-L20) |
| 激活趋势 | 是 | **激活 API 会产生激活记录；仓库没有生产数据/趋势验收** | App 只 POST activate。[客户端](../../crates/app/src/license_gate/activate_http.rs#L40-L57) |
| 功能使用 `usage` | 是 | **否** | 当前客户端只实现 `/license/activate`，没有 `/license/usage` 调用。[客户端](../../crates/app/src/license_gate/activate_http.rs#L40-L105) |
| 社区活跃度 | 否，非此产品域 | **否** | 无社区实体或事件 |

### 4.4 技术实现与代码位置

#### Keygate 服务/后台

- **部署形态**：上游单个 Go binary + PostgreSQL；前端为 React 19 + TypeScript + Vite，使用 TanStack Query、React Router、Recharts 与 Radix UI。[上游架构说明](https://github.com/tabloy/keygate/blob/40f59cd518b46a8b61830e7834567ab31dfa42a8/README.md#L87-L89) · [前端依赖](https://github.com/tabloy/keygate/blob/40f59cd518b46a8b61830e7834567ab31dfa42a8/web/package.json#L14-L46) · [Go 依赖](https://github.com/tabloy/keygate/blob/40f59cd518b46a8b61830e7834567ab31dfa42a8/go.mod#L1-L15)
- **Norma 仓库内的服务 harness**：`spikes/keygate-mvp/docker-compose.yml`、`scripts/bootstrap.sh`、`scripts/check.sh`、`scripts/activate.sh`、`scripts/offline_verify.sh`。[脚本职责表](../../spikes/keygate-mvp/README.md#L49-L57)
- **边界**：Keygate 的 Go/React 源码不在 Norma 仓库；Compose 通过 GitHub tag 拉取并构建，所以业务后台不能在 `crates/` 内修改。[Compose pin](../../spikes/keygate-mvp/docker-compose.yml#L1-L9)

#### Norma 客户端

- `crates/app/src/license_gate/gate_root.rs`：GPUI 激活壳、后台发起激活、保存 token、成功后同窗进入 Workbench。[激活流程](../../crates/app/src/license_gate/gate_root.rs#L102-L160) · [界面](../../crates/app/src/license_gate/gate_root.rs#L259-L340)
- `crates/app/src/license_gate/activate_http.rs`：用 `ureq` 调 `POST /api/v1/license/activate`，解析 token/status/license_id。[HTTP adapter](../../crates/app/src/license_gate/activate_http.rs#L40-L105)
- `crates/app/src/license_gate/token_store.rs`：把离线 token 存到 Application Support 的 `license/offline.token`。[token store](../../crates/app/src/license_gate/token_store.rs#L1-L28)
- `crates/license/src/evaluate.rs`：零 GPUI 的 Ed25519 验签、过期、机器 `did` 与状态校验。[校验流程](../../crates/license/src/evaluate.rs#L7-L145)
- `crates/app/src/license_gate/config.rs`：feature、环境变量、嵌入 URL 和公钥的解析优先级。[配置](../../crates/app/src/license_gate/config.rs#L1-L54)

## 5. 产品分析后台候选：八个平台均未接入

### 5.1 统一状态

项目已有一份完整的平台调研，但没有相应实现。它明确写的是“推荐排序”“建议方案”“可以先用 PostHog Cloud 做验证”，并要求未来实现 provider-neutral adapter、磁盘队列、批量发送和失败重试。[建议方案](2026-07-28-independent-developer-product-analytics-platforms.md#L196-L208) · [最终建议](2026-07-28-independent-developer-product-analytics-platforms.md#L265-L277)

全仓当前事实：

- Cargo manifest/lock 中没有这些平台 SDK；
- `crates/**` 中没有 PostHog、TelemetryDeck、OpenPanel、Umami、Mixpanel、Aptabase 或 Firebase Analytics adapter；
- 当前 SQLite migrations 没有用户、Session、事件或上传队列表；
- 当前 Settings 导航只有 Themes、Keyboard、Voice、Agents、Orchestration、Wallpaper，没有 Analytics 开关。[Settings 导航](../../crates/app/src/settings/navigation.rs#L33-L45) [分组](../../crates/app/src/settings/navigation.rs#L116-L126)

因此下面所有能力都是**平台候选能力，不是 Norma 当前能力**。

### 5.2 候选平台完整对比

| 平台 | 用户信息 | 注册量/活跃 | 功能使用 | 技术接入路径 | Norma 状态 |
|---|---|---|---|---|---|
| **PostHog** | 匿名 ID、`identify`、Person Profile | 可做活跃、漏斗、留存；注册总数仍应来自账号/授权后端 | 自定义事件、漏斗、留存完整 | 官方 Rust SDK、Swift/macOS、HTTP；Rust 需自己补桌面 Session | **仅调研；首选推荐** |
| **TelemetryDeck** | 哈希 `clientUser`，不可读 Profile | 可去重活跃注册用户；Duration Events 能排除后台 | 自定义事件、漏斗、留存 | Swift/macOS、HTTP，无 Rust SDK | **仅调研；隐私/时长首选** |
| **OpenPanel** | `profileId`、完整 Profile | Sessions、Cohorts、Retention | 自定义事件、Funnels | Rust、Swift/macOS、HTTP；可自托管 | **仅调研；开放性首选** |
| **Umami** | 匿名/Distinct ID/Identify | Session、Retention；访问时长是首末事件差 | 自定义事件、Funnels、Journeys | HTTP；无官方 Rust/Swift SDK；轻量自托管 | **仅调研** |
| **Mixpanel** | Device ID、`identify`、User Profile | Session、漏斗、留存 | 成熟事件分析 | Swift/macOS、HTTP，无 Rust SDK | **仅调研；不占优** |
| **Amplitude** | Device ID、User ID、User Profile | 自动 Session、活跃/留存；后台事件可能虚高 | 成熟事件分析 | Swift/macOS、HTTP，无 Rust SDK | **仅调研；不占优** |
| **Aptabase** | 刻意无长期用户 ID/Profile | 不能稳定算 MAU 或用户留存 | 匿名自定义事件、Session Timeline | Swift/macOS、HTTP；可自托管 | **仅调研；明确不满足用户统计目标** |
| **Firebase Analytics / GA4** | User ID、User Properties，禁止邮箱等 PII | GA4 Session/参与度、Cohort/Retention | 事件、漏斗 | macOS beta Swift 二进制 SDK；HTTP Measurement Protocol 只是补充 | **仅调研；不适合当前 Rust-first** |

本表的项目内证据见原始平台矩阵。[平台对比](2026-07-28-independent-developer-product-analytics-platforms.md#L71-L84)；Mixpanel/Amplitude/Aptabase/Firebase 的展开核对见[后半部分](2026-07-28-independent-developer-product-analytics-platforms.md#L157-L194)。

### 5.3 这些平台能否统计“注册量”

项目调研已经给出正确边界：

- **注册用户总数的事实源应该是账号、授权或支付后端，不应是分析平台。** 事件可能丢失、延迟或被 opt-out。[指标定义](2026-07-28-independent-developer-product-analytics-platforms.md#L39-L48)
- 分析平台适合回答“多少注册用户活跃”“注册用户用了什么”“注册→激活→留存→付费的转化”，而不是替代账号账本。[同上](2026-07-28-independent-developer-product-analytics-platforms.md#L43-L48)
- 当前 Norma 没有账号后端；Keygate License 可以作为授权事实源，但一个 License 可能对应一台、多台或多人，不能未经定义就等同于一个注册用户。

### 5.4 已设计但尚未实现的功能使用统计

调研提出的最小事件包括：

- App Session、前后台与 `active_ms`；
- 通用 `feature_used`；
- Terminal 创建；
- Agent 任务开始/完成与耗时桶；
- Voice 输入；
- onboarding 步骤；
- Analytics opt-out。[事件表](2026-07-28-independent-developer-product-analytics-platforms.md#L210-L225)

身份策略也只处于设计态：匿名 ID → 授权后稳定 UUID → HMAC 后的 analytics ID → `identify`；注册总数仍回到授权后端。[身份策略](2026-07-28-independent-developer-product-analytics-platforms.md#L227-L234)

当前代码里虽然有 `voice_history` 与 `agent_dispatch_tasks` 等本地业务表，但它们是单机/单项目的运行记录，不是跨用户产品分析仓库，也没有运营 dashboard 或上传链路。[本地 schema](../../crates/storage/src/migrations.rs#L93-L107) [Agent task schema](../../crates/storage/src/migrations.rs#L126-L149)

### 5.5 Feature Guide / Help Center 已定义成功指标，但没有采集后台

当前工作树还有一份“功能导览与帮助中心”产品设计草案。它明确标注“待确认、待排期、本轮不进入代码实现”，因此只能算**指标需求来源**，不能算已经存在的管理面板或 Analytics 实现。[草案状态](../product-specs/2026-08-01-SPEC-feature-guide-and-help-center.md#L1-L8)

该草案提出的成功指标包括：

- 自动导览展示率、完成率和跳过率；
- 各主题查看率与中途退出位置；
- 新用户打开项目后首次有效 Agent 派发的时间；
- 看过语音主题的用户到第一次语音派发的速度；
- 用户经 Help 菜单重新打开导览的比例。[成功指标](../product-specs/2026-08-01-SPEC-feature-guide-and-help-center.md#L410-L421)

这些指标与前述 PostHog 事件方案方向一致，但当前没有事件定义、发送 adapter、数据库表或后台报表实现。草案自身还要求不得记录语音内容、任务正文、项目路径或 Agent 对话文本，并把“首次有效派发”而非导览完成率定义为核心结果指标。[隐私与核心结果](../product-specs/2026-08-01-SPEC-feature-guide-and-help-center.md#L412-L421)

## 6. Lemon Squeezy 商户后台：提到过，但尚未成为 Norma 系统

### 6.1 状态

Lemon Squeezy 在商业化文档中是“路径二”，用于港企/个人主体收款，明确延后；License SPEC 也把 Payments、Invoices、Lemon Squeezy 和 Customer Portal/Account System 列为 out of scope。[License 设计分期](../design-docs/2026-07-14-software-licensing.md#L18-L25) · [SPEC out of scope](../product-specs/2026-07-14-SPEC-software-licensing.md#L91-L101)

2026-07-27 的专项文档明确声明“不讨论 API、Webhook 或代码实现”，所以它只能证明团队研究了商户注册、店铺审核、支付和结算，不能证明已建立 Norma 商户后台集成。[调研范围](2026-07-27-lemonsqueezy-registration-and-payout-hk.md#L1-L5)

### 6.2 文档中提到的后台能力

- 注册商户账号、创建 Store；
- 在后台创建产品、选择一次性或订阅价格；
- 激活店铺、身份审核；
- Settings → Payouts 连接银行账户/PayPal；
- 使用托管店铺或 Checkout Link，无需先写代码。[非技术流程](2026-07-27-lemonsqueezy-registration-and-payout-hk.md#L32-L49)

### 6.3 对本题四类统计的支持判断

| 维度 | 结论 |
|---|---|
| 用户信息统计 | 文档只证明商户/客户问卷、交易与结算场景；没有 Norma 客户同步实现 |
| 注册量统计 | 商户自己的注册流程不等于 Norma 用户注册；Norma 无接入 |
| 功能使用统计 | 不属于该调研范围，也无埋点链路 |
| 社区活跃度 | 无 |

## 7. Keygen CE / Portal：仅失败备选

Keygen CE 只在 Keygate spike 失败时才启用；设计文档写明它“CE 无完整 Dashboard，可后补 Portal”。Keygate spike 已经 GO，所以 ADR 明确“不换 Keygen CE”。仓库没有 Keygen 服务、客户端、Portal 或数据模型。[设计备注](../design-docs/2026-07-14-software-licensing.md#L45-L49) · [ADR outcome](../adr/0014-spike-keygate-before-app-licensing.md#L5-L7)

## 8. 容易误判为“后台”的本机面板与诊断

### 8.1 Norma Settings

Settings 是 GPUI 用户侧配置面板，不是运营后台。当前实际页面为：Themes、Keyboard、Voice、Agents、Orchestration、Wallpaper。[导航枚举](../../crates/app/src/settings/navigation.rs#L33-L45) · [内容路由](../../crates/app/src/settings/content.rs#L81-L107)

它不支持：

- 跨用户查看资料；
- 用户注册或注册量；
- 产品行为聚合；
- 社区内容/成员管理；
- License 管理页。当前上下文还明确写着“一期不在 Settings 做完整 License 管理页”。[License 客户端边界](../../CONTEXT.md#L328-L330)

### 8.2 本地性能监控与日志

当前工作树中有一套本地性能采样实现：按 Background/Idle/Animation/TerminalActive/VoiceActive 区分状态，采集 CPU、内存、wakeups、线程、repaint、render；每 10 个样本输出 summary，高 CPU 时写本机 JSON 诊断。[状态与指标](../../crates/app/src/performance_monitor/recording.rs#L12-L20) [样本字段](../../crates/app/src/performance_monitor/recording.rs#L80-L90) · [日志/诊断落盘](../../crates/app/src/performance_monitor.rs#L280-L334)

**状态限定：** 调研时 `crates/app/src/performance_monitor.rs` 与其子目录仍是 Git 未跟踪文件，所以这只能描述当前共享工作树里正在形成的实现，**不能代表已提交仓库基线或已发布版本**。无论它之后是否提交，它的设计仍是本机技术诊断，而不是运营后台。

它不是产品使用统计后台，原因是：

- 没有用户身份；
- 没有远程上报或跨安装聚合；
- 没有管理 UI；
- `TerminalActive`/`VoiceActive` 是性能场景标签，不是用户功能采用事件。

同理，`~/.norma/logs` 的滚动日志只是本机诊断文件，不是运营 dashboard；日志按日轮转、保留 14 天。[日志实现](../../crates/app/src/logging.rs#L15-L22) [初始化](../../crates/app/src/logging.rs#L49-L100)

## 9. 用户、注册、功能使用、社区四类能力总对比

符号：`✅` 当前已接入且有证据；`◐` 平台有能力但 Norma 未完整使用；`◇` 仅文档候选；`—` 不存在/不适用。

| 系统 | 用户信息 | 注册/新增量 | 功能使用统计 | 社区活跃度 | 状态说明 |
|---|---:|---:|---:|---:|---|
| **Keygate Admin** | ◐ | ◐ | ◐ | — | 平台支持 Users/Customers、License/激活趋势、usage；Norma 只接 activate，spike 只验证 License 列表和总数 |
| **PostHog** | ◇ | ◇ | ◇ | ◇（需自定义事件） | 首选调研方案，未接入 |
| **TelemetryDeck** | ◇（哈希） | ◇ | ◇ | ◇（需自定义事件） | 未接入 |
| **OpenPanel** | ◇ | ◇ | ◇ | ◇（需自定义事件） | 未接入 |
| **Umami** | ◇ | ◇ | ◇ | ◇（需自定义事件） | 未接入 |
| **Mixpanel** | ◇ | ◇ | ◇ | ◇（需自定义事件） | 未接入 |
| **Amplitude** | ◇ | ◇ | ◇ | ◇（需自定义事件） | 未接入 |
| **Aptabase** | — | —（不能稳定 MAU/留存） | ◇ | ◇（只能匿名事件） | 未接入且不满足用户级目标 |
| **Firebase Analytics** | ◇ | ◇ | ◇ | ◇（需自定义事件） | 未接入 |
| **Feature Guide / Help 指标** | — | ◇（新用户首次派发时间） | ◇（导览查看/完成/跳过/退出） | — | 仅产品设计草案，无采集或后台实现 |
| **Lemon Squeezy** | ◇（交易客户语境） | — | — | — | 仅非技术商户调研，路径延后 |
| **Keygen CE/Portal** | — | — | — | — | 失败备选，未采用 |
| **Norma Settings** | — | — | — | — | 单机用户配置，不是运营后台 |
| **本地性能/日志** | — | — | — | — | 本机技术诊断；性能监控还是未提交的在研工作树代码 |

这里的“◇ 社区”只表示通用事件平台理论上可以接收自定义社区事件；由于 Norma 根本没有社区模块或事件源，**当前实际值全部是“无”**。

## 10. 社区活跃度专项结论

### 10.1 当前不存在的内容

全仓没有发现以下任何 Norma 产品实体或流程：

- 社区账号/成员；
- 帖子、评论、点赞、关注、群组；
- 社区审核/举报后台；
- 社区 DAU/WAU/MAU；
- 发帖数、评论数、互动率、贡献者活跃度；
- Discord/论坛/社群同步。

SQLite 当前只有工作区、设置、授权、Canvas/session、Voice 历史、Agent 定义/任务/会话尾部等表，没有 community/member/post/comment 表。[应用库](../../crates/storage/src/migrations.rs#L26-L55) · [项目库](../../crates/storage/src/migrations.rs#L57-L169)

### 10.2 不应误读的两处“社区”

1. Rust Agent 框架调研里的“社区规模”是在比较第三方技术生态，不是 Norma 用户社区。[技术调研](2026-07-18-rust-agent-frameworks.md#L8-L13)
2. Keygate README 的 Discussions/Issues 是 Keygate 项目的开源社区入口，不是部署实例里的社区功能，更没有 Norma 社区统计。[上游 README](https://github.com/tabloy/keygate/blob/40f59cd518b46a8b61830e7834567ab31dfa42a8/README.md#L144-L153)

### 10.3 如果未来要做，现有平台能承担哪一层

- PostHog/OpenPanel/Mixpanel/Amplitude 等只能承担**事件采集与分析层**；
- 社区业务后端仍需提供成员、内容和互动的事实源；
- 建议事件至少包括 `community_member_joined`、`post_created`、`comment_created`、`reaction_added`、`moderation_action`，并明确真实用户/组织去重口径；
- 这只是未来建议，当前仓库没有相关产品方向或已批准需求。

## 11. 最终判断

### 已有

- Keygate License 管理后台/服务链路；
- License 邮箱/备注、列表和总量统计；这些字段不等同于正式客户账号；
- Norma 桌面端在线激活 + 离线验签；
- Keygate 平台本身具备更完整的用户、License、激活和 usage analytics 能力。

### 部分已有但不能高估

- Keygate 上游有用户详情、激活趋势、功能 usage top；
- Norma 当前没有调用 `/license/usage`，所以这些页面没有 Norma 功能用量数据来源；
- Keygate 的 License 数/激活数可以作为商业运营指标，但不是经过定义的“注册用户数”。

### 只有文档

- PostHog、TelemetryDeck、OpenPanel、Umami、Mixpanel、Amplitude、Aptabase、Firebase Analytics；
- Lemon Squeezy 商户/收款后台；
- Keygen CE/Portal fallback。

### 完全没有

- Norma 账号注册后台；
- 注册用户总量的正式事实源与统计口径；
- 产品功能使用埋点与跨用户分析；
- 社区模块、社区管理后台和社区活跃度统计。
