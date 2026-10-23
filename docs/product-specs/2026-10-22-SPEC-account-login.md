# SPEC：GitHub / Google 账号登录

日期：2026-10-22
状态：implementing（原型已定稿；2026-10-22 修订：补齐扩展接缝、账号关联仅留数据模型）
原型：[`docs/design-docs/2026-10-22-account-login-prototype/index.html`](../design-docs/2026-10-22-account-login-prototype/index.html)（变体 D 启动页、变体 A 账号页；变体 B、C 不实施）
相关：[软件 License 硬门控](2026-07-14-SPEC-software-licensing.md)、[ADR-0028 macOS 凭据存 AppDB](../adr/0028-store-macos-credentials-in-app-db.md)、[norma-cloud 服务](../../services/norma-cloud/README.md)

## Problem Statement

Norma 目前没有任何用户身份：授权靠 License Key，手机靠配对。用户无法在多台设备上被识别为同一个人，也没有承载后续云端能力（偏好、团队）的身份基础。需要让用户用 GitHub 或 Google 账号登录，并且不影响现有「无需账号即可完整使用」的承诺。

## Solution

1. License 激活通过后、进入 Launchpad 之前，显示一次极简启动登录页：Launchpad 专用壁纸铺满窗口，居中应用 Logo、`使用 GitHub 继续`、`使用 Google 继续`、`稍后再说`。
2. 授权在系统默认浏览器完成，由 `norma-cloud` 做 OAuth 中转，App 不持有任何 OAuth client secret；完成后自动回到 App。
3. 设置新增首个导航目的地「账号」，管理登录状态、关联第二种登录方式、退出登录。
4. 账号只作为身份。License 门控、工作台功能与手机配对均不依赖账号。

## 已定决策

| 决策 | 结论 | 原因 |
| --- | --- | --- |
| 登录用途 | 仅作账号身份 | 保持 License 硬门控（ADR-0015）与「无需账号」不变 |
| 登录入口 | 启动页（一次）+ 设置 → 账号 | 启动页发现性最高，设置页负责管理；标题栏头像弹层本期不做 |
| OAuth 后端 | `norma-cloud` 新增 `auth/` 模块 | client secret 只在服务端；App 只拿短期会话 |
| 回调方式 | 本机回环 `http://127.0.0.1:<随机端口>/callback` | 无需在 `.app` 注册 URL scheme，开发态 `cargo run` 同样可用 |
| 客户端会话存放 | `app.sqlite` 设置键，权限 0600 | 遵循 ADR-0028，不读写 macOS Keychain（避免授权弹窗） |
| 账号模型 | 服务端 `account_id` + `identities[]`（provider + sub）；邮箱只作展示，永不作合并依据 | 稳定 `account_id` 是后续云端数据主键，上线后难补 |
| 账号关联 | 数据模型支持多身份；关联接口与「关联」按钮本期不开放 | 接缝现在留，功能后做 |
| 域 crate | 新增 `account`（零 GPUI/Storage/网络），以 `AccountSessionStore`、`AuthApi` 两个 trait 与 `app` 解耦 | 后续使用方（同步、`norma`、agent）不能依赖 `app`；crate 边界后移代价大 |
| 协议版本 | 路由 `/v1/`，会话带 `schema_version`，保留 `refresh`、`me`、`logout` | 旧版 App 需持续兼容 |
| 变更通知 | `app` 层 `AccountState` 全局状态 + 变更通知 | 后续功能订阅即可，不散读 AppDB |
| 跳过后再提示 | 跳过或退出登录后不再自动弹出启动登录页 | 账号可选，避免反复打扰；入口保留在设置 |
| 本期不做 | 偏好同步、跨设备数据、iOS 登录、头像弹层、账号关联入口 | 无对应云端存储；原型里「同步偏好」文案不得上线 |

## User Stories

1. 作为新用户，我首次进入时看到简洁的登录页，可以一键用 GitHub 或 Google 登录。
2. 作为不想登录的用户，我点「稍后再说」直接进入 Launchpad，且之后不再被打扰。
3. 作为登录中的用户，我可以随时取消授权，或重新打开浏览器，不会卡死在等待页。
4. 作为授权失败的用户，我看到明确原因并能重试，同时确认 License 与本地数据不受影响。
5. 作为已登录用户，我在 设置 → 账号 看到头像、昵称、邮箱、登录方式。
6. （数据模型支持、本期不开放）作为已登录用户，将来可以关联另一种登录方式，用任一方式登录得到同一账号。
7. 作为已登录用户，我可以退出登录，只清除本机会话。
8. 作为已登录用户，重启 App 后仍保持登录，无需再次授权。
9. 作为离线用户，我已登录的状态在无网络时仍显示，不阻塞启动。
10. 作为维护者，我希望 OAuth secret 只存在于服务器，App 包与仓库中没有。
11. 作为维护者，我希望登录流程的状态机与协议在无 GPUI 的 crate 中可单测。

## 用户体验

### 启动登录页（原型变体 D）

- 触发：License 门控放行（含 `NORMA_SKIP_LICENSE=1`）后，若「无有效会话」且「未跳过过」，在同一窗口显示；其余情况直接进入 Launchpad。
- 视觉：无标题栏；仅保留 macOS 原生窗口控件。背景为 `crates/app/assets/wallpapers/home.mp4`（与 Launchpad 同一专用资源，沿用 [`2026-07-15-launchpad-dedicated-video.md`](../design-docs/2026-07-15-launchpad-dedicated-video.md) 的低拷贝 `surface()` 路径，禁止逐帧 CPU RGBA），其上压深色渐变；缺失时回退静态壁纸。居中 `APP_ICON_ASSET`、产品名、磨砂卡片。
- 卡片状态：
  - 未登录：标题 `登录 Norma`、副标题、两个提供方按钮、`稍后再说`。
  - 等待授权：`正在等待 {提供方} 授权…`、转圈、`重新打开浏览器`、`取消`。
  - 失败：行内错误提示 + 两个按钮可重试。
  - 成功：头像、`欢迎回来，{昵称}`、`进入 Norma`、`切换账号`。
- 提供方按钮遵循各自品牌规范（GitHub 深色/浅色、Google 白底），不改品牌色。

### 设置 → 账号（原型变体 A）

- 导航：设置侧栏最前新增「账号」分组，其下一项「账号」；已登录时侧栏条目显示头像与昵称。
- 未登录：居中登录卡片（Logo、两个按钮、隐私说明「仅读取头像、昵称与邮箱 · 浏览器中授权」）。不展示尚未实现的权益说明块；原型中的「登录后可以」两项为占位，实施时删除或改为真实能力。
- 已登录：封面 + 头像 + 昵称 + 邮箱 + `已登录`；「登录方式」只读列表（显示当前登录方式，无「关联」按钮）；退出登录按钮及说明「只清除本机会话，不影响 License 与项目数据」。
- 原型中「此设备」列表（Mac / iPhone）与「会话保存在系统钥匙串」文案不实施：设备列表无数据来源；会话存放以 ADR-0028 为准，文案写「本机」。

### Apple / Material 原则落地（CLAUDE.md §8）

- 层级与内容优先：启动页只有一个视觉焦点；设置页沿用现有侧栏与卡片。
- 直接反馈：按钮按下即响应；等待态立即出现，可取消、可重开浏览器。
- 状态：按钮覆盖 hover / pressed / 键盘焦点 / disabled；Esc 在等待态等价于取消；Tab 顺序为 GitHub → Google → 稍后再说。
- 可访问性：等待态与错误提示带可读文本；转圈在「减少动态效果」下改为静态进度提示；文本对比度在壁纸之上通过磨砂卡片保证。
- 动效：仅卡片出现的轻微过渡与转圈，服务于状态理解；无装饰动效。
- 参考的 skill：`apple-design`（响应、可中断、减少动态）；账号页的列表与选中状态沿用设置既有规范。

## 架构

### 依赖方向

```text
app -> account            (account 为 GPUI/Storage/网络无关的域 crate)
account -> serde / serde_json / sha2 / base64 等纯依赖；不得依赖 app/runtime/storage/ui/gpui
storage 不依赖 account；app 负责把会话交给 storage 的 settings
```

`scripts/check-account-boundaries.sh` 用 `cargo tree` 机械拦截上述方向，并纳入 pre-commit 与 `docs/quality/mechanized-constraints.md`。

### 目录结构

```text
crates/account/src/
├── lib.rs
├── model.rs      # AccountId、Identity、Profile、Session（含 schema_version）
├── provider.rs   # Provider 注册表（GitHub、Google）
├── pkce.rs       # code_verifier、S256 challenge、state 生成与常量时间校验
├── flow.rs       # 登录状态机（纯函数）
├── ports.rs      # AccountSessionStore、AuthApi 两个 trait
└── protocol.rs   # /v1 请求与响应类型
crates/account/tests/{unit,support}/

crates/app/src/account/
├── mod.rs
├── loopback.rs   # 127.0.0.1 随机端口单次监听、超时、取消
├── browser.rs    # 打开授权页、重新打开、复制链接
├── client.rs     # 实现 AuthApi，后台执行网络
├── store.rs      # 实现 AccountSessionStore，写 AppDB
├── state.rs      # AccountState 全局状态与变更通知
├── controller.rs # 串联 flow 与副作用
└── gate_phase.rs # 纯函数 (license, session, dismissed) -> 启动阶段

crates/app/src/license_gate/gate_root/login.rs   # 启动登录页
crates/app/src/settings/account/                 # 设置 → 账号页
crates/ui/locales/account.{en,zh-CN}.json

services/norma-cloud/auth/
├── routes.mjs  sessions.mjs  config.mjs
├── accounts/{store.mjs,file-store.mjs}          # 接口 + 文件实现
└── providers/{index.mjs,github.mjs,google.mjs}
services/norma-cloud/tests/auth.test.mjs

scripts/check-account-boundaries.sh
docs/adr/0029-account-identity-via-norma-cloud.md
```

### `account` crate 职责

- **状态机**：`Idle → AwaitingBrowser → Exchanging → SignedIn | Failed(reason)`，取消回到 `Idle`，另含超时；输入为事件，输出为新状态与待执行动作，不做 IO。
- **PKCE**：`code_verifier` 43–128 字符 base64url，`code_challenge = S256`，`state` ≥128 位随机，校验使用常量时间比较。
- **回调解析**：纯函数解析 `code`、`state`、`error`，缺失或 `state` 不匹配即拒绝。
- **端口**：`AccountSessionStore`（读/写/清除会话与「已跳过」标记）与 `AuthApi`（`exchange`、`refresh`、`me`、`logout`）。`app` 提供真实实现，测试使用内存假实现。
- **提供方注册表**：`Provider` 的标识、显示名、品牌资源键；新增提供方不改状态机。

### App 适配层约束

- 回环监听只绑定 `127.0.0.1`，单次有效，收到一次回调、超时（5 分钟）或取消即关闭；响应静态「可以回到 Norma」页面。
- 所有网络与监听在 `background_executor` 执行；前台只更新 `AccountState`。
- 持久化键 `account.session`、`account.login_prompt.dismissed`；启动只读本地值，不联网，无网络不影响启动，读取失败回落为未登录。
- GateRoot 在 `BootPhase` 增加登录阶段（Activation 之后、Workbench 之前）；`MultisoulApp` 仍只在进入 Workbench 时构造，登录阶段不启动 Runtime/PTY。
- 文案全部走 `ui::i18n`；单文件 ≤ 500 行；不得使用 `#[allow(...)]`。
- 扩展方式：新增提供方 = 服务端一个 `providers/*.mjs` + `account::provider` 一个枚举值 + 一条文案；新增使用方 = 订阅 `AccountState`。

### 服务端 `services/norma-cloud/auth/`（Node 22，无 npm 依赖，沿用 `server.mjs` 单端口装配）

| 路由 | 说明 |
| --- | --- |
| `GET /v1/auth/{provider}/start` | 入参 `code_challenge`、`state`、`redirect_port`（1024–65535）；保存一次性授权记录（TTL 10 分钟，内存），302 到提供方 |
| `GET /v1/auth/{provider}/callback` | 用服务端 secret 换取提供方 token，仅读一次 profile 后丢弃；按 `(provider, sub)` 查找或创建账号；生成一次性 `code`（TTL 60 秒、单次），302 到 `http://127.0.0.1:{port}/callback?code=&state=` |
| `POST /v1/auth/token` | 入参 `code`、`code_verifier`；PKCE 通过返回 `Session`；失败或重放均拒绝 |
| `POST /v1/auth/refresh` | 刷新令牌轮换，旧令牌立即失效 |
| `GET /v1/auth/me` | 返回 `Profile`（含 `identities[]`） |
| `POST /v1/auth/logout` | 使会话与刷新令牌失效 |

- scope：GitHub `read:user user:email`；Google `openid email profile`。仅读取头像、昵称、邮箱。
- 账号：`AccountStore` 接口（`find_by_identity`、`create`、`get`），当前实现为 `/var/lib/norma-cloud/accounts.json`（0600、原子写入）；不存储提供方 token 与密码。邮箱不参与查找，同邮箱不同提供方得到不同账号。
- 配置：`GITHUB_CLIENT_ID/SECRET`、`GOOGLE_CLIENT_ID/SECRET`、会话签名密钥只来自环境文件（root 0600）；缺少已启用提供方的配置必须拒绝启用该提供方并明确报错，不得静默降级；密钥不入库、不进 App 包。
- 安全：`redirect_port` 只允许回环；日志不得输出 `code`、token、邮箱全文；限流沿用现有 Nginx 限制。

## 验收标准与证据映射

| # | 标准 | 证据 |
| --- | --- | --- |
| 1 | `account` crate 无 GPUI/Storage/ui/app/runtime 依赖 | `bash scripts/check-account-boundaries.sh`；`cargo test -p account` |
| 2 | PKCE 的 `code_challenge` 与 RFC 7636 示例向量一致；`state` 不匹配/缺失被拒；回调含 `error` 进入 `Failed` | `account` 单元测试 |
| 3 | 状态机：取消、超时、重复回调、乱序事件均不进入 `SignedIn` | `account` 单元测试 |
| 4 | 用内存假 `AccountSessionStore` 与假 `AuthApi` 即可跑通完整登录、失败、取消流程 | `account` 单元测试（证明接缝可替换） |
| 5 | 会话带 `schema_version`；未知更高版本与损坏数据回落为未登录而不崩溃 | `account` 单元测试 |
| 6 | 服务端：`code` 单次有效、60 秒过期、错误 `code_verifier` 被拒、`redirect_port` 非法被拒、刷新令牌轮换后旧令牌失效 | `npm --prefix services/norma-cloud test`（新增 `tests/auth.test.mjs`，本地假提供方） |
| 7 | 服务端：同邮箱不同提供方得到不同 `account_id`；同一 `(provider, sub)` 重复登录得到同一 `account_id`；模型支持 `identities[]` 多项 | 同上 |
| 8 | 服务端：新增提供方只需新增 `providers/` 文件并注册，无需改路由；缺 secret 时该提供方拒绝启用且日志无明文 secret | 同上 |
| 9 | 会话写入 AppDB 且文件权限 0600；重启后无需联网即显示已登录 | `cargo test -p storage`、`cargo check -p app` 及 `app` 单元测试 |
| 10 | 登录阶段不构造 `MultisoulApp`、不启动 PTY；跳过或退出登录后直接进入 Launchpad 且不再弹出 | `gate_phase` 纯函数测试 |
| 11 | 网络请求不在 GPUI 前台线程执行 | `scripts/check-performance-guardrails.sh` 增加对账号模块的规则 |
| 12 | 真实 App：启动登录页壁纸铺满、无标题栏、居中卡片；四种状态；深/浅主题 | `scripts/desktop-e2e.sh account`（`test/desktop-e2e/Scenarios/account/`），覆盖不到再降级 `computer-use:computer-use` |
| 13 | 真实 App：设置 → 账号 未登录/已登录/退出 | 同上 |
| 14 | 真实授权端到端（GitHub 与 Google 各一次，含取消与失败） | 人工 L3 验收，记录到 `docs/exec-plans/`；需要真实 OAuth 应用 |
| 15 | 仓库与 App 包中无 OAuth secret | `rg -n 'GITHUB_CLIENT_SECRET\|GOOGLE_CLIENT_SECRET'` 仅命中 `*.example` 与文档 |

自动化中的 `TestAppContext` 不等于真实 App；标准 12–14 必须在真实程序上取得证据。

## 开放项（实施前需要运营者提供，不阻塞编码）

1. 在 GitHub 与 Google 各注册一个 OAuth 应用，回调地址为生产 `https://soulvoiceai.com/norma/v1/auth/{provider}/callback`；提供 client id/secret 配置到服务器环境文件。
2. 确认 Google OAuth 同意屏幕的应用名称、Logo、隐私政策链接（Google 审核需要）。
3. 确认账号数据的保留与删除政策（最少需要「注销账号」入口时的行为）；本期不实现注销，只实现退出登录。

## Out of Scope

- 偏好同步与任何云端用户数据；「同步偏好」不得出现在 UI 文案中。
- iOS 端登录；手机配对与账号相互独立。
- 标题栏头像弹层（原型变体 C）与首次引导弹窗（变体 B）。
- 用账号取代 License 门控；账号与 License 的绑定。
- 账号关联接口与按钮（数据模型已支持）、邮箱密码登录、其它第三方提供方、账号注销、多设备会话管理列表。

## 实施顺序建议

1. ADR-0029 与 `scripts/check-account-boundaries.sh`。
2. `account` crate 与测试（红 → 绿）。
3. `services/norma-cloud/auth/` 与 `tests/auth.test.mjs`。
4. App 适配层：回环监听、后台网络、AppDB 持久化、`AccountState`。
5. GateRoot 登录阶段与启动登录页。
6. 设置 → 账号页与 i18n。
7. desktop-e2e `account` 模块与人工 L3 验收。
