# ADR-0029：账号身份经 norma-cloud 中转

- 日期：2026-10-22
- 状态：Accepted
- 规格：[`2026-10-22-SPEC-account-login.md`](../product-specs/2026-10-22-SPEC-account-login.md)

## 决策

Norma 支持 GitHub 与 Google 登录，账号仅作身份，不取代 License 门控（ADR-0015）。

1. **服务端中转**：OAuth client secret 只存在于 `norma-cloud`；App 以 PKCE + 一次性 `code` 向服务端换取会话，不持有提供方凭据，也不保存提供方 token。
2. **回环回调**：App 在 `127.0.0.1` 随机端口单次监听接收回调，不注册 URL scheme，开发态与打包态行为一致。
3. **独立域 crate `account`**：状态机、PKCE、会话模型与 `/v1` 协议类型零 GPUI/Storage/网络；副作用经 `AccountSessionStore`、`AuthApi` 两个 trait 注入。边界由 `scripts/check-account-boundaries.sh` 机械拦截。
4. **账号模型**：服务端 `account_id` + `identities[]`，身份为 `(provider, subject)`，邮箱只作展示、永不作合并依据。关联接口本期不开放，但数据模型已支持。
5. **会话存放**：macOS 端写入 `app.sqlite`（ADR-0028），不用 Keychain；会话带 `schema_version`，读到更高版本或损坏数据按未登录处理。
6. **默认关闭的发布开关**：`account-login` feature（调试构建另可用 `NORMA_ACCOUNT_LOGIN=1`）开启前，启动页、设置入口与控制器均不安装，行为与引入前一致。
7. **协议版本化**：路由 `/v1/auth/*`，保留 `refresh`、`me`、`logout`，以便旧版 App 持续兼容。

## 取舍

- 多一个 crate 与一个服务端模块，换来后续使用方（同步、`norma`、agent）可依赖账号而不依赖 `app`，且状态机无需窗口即可测试。
- 会话明文入库与 ADR-0028 同一风险边界；日志、诊断与 `Debug` 输出不得包含令牌、`code`、verifier。
- 服务端当前以文件存储账号，通过 `AccountStore` 接口隔离，后续换数据库不影响路由与协议。

## 不做

偏好同步、账号注销、设备列表、其它提供方、iOS 登录。
