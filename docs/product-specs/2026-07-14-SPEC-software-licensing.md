# SPEC：软件 License（买断 + Keygate + 薄接入）

日期：2026-07-14  
状态：Keygate 脚本 spike **GO**；下一手 **薄接入 App**  
设计依据：[`docs/design-docs/2026-07-14-software-licensing.md`](../design-docs/2026-07-14-software-licensing.md)  
ADR：[`docs/adr/0014-spike-keygate-before-app-licensing.md`](../adr/0014-spike-keygate-before-app-licensing.md)  
Spike：[`spikes/keygate-mvp/README.md`](../../spikes/keygate-mvp/README.md)

## Problem Statement

Norma OS / Multisoul Desktop 需要可商业化的激活能力。收款主体（港企 + Lemon Squeezy）延后；先跑通「签发 → 交付 → 首次联网激活 → 离线复验」。支付与授权解耦，一期由运营者在后台手动发证。

## Solution（已锁定）

- **签发与后台**：自托管 [Keygate](https://github.com/tabloy/keygate)（管理员手动签发买断 License，可见列表/统计）。
- **客户端契约**：首次 `POST /license/activate` 联网 → 落盘离线 token + pubkey → 之后本地 Ed25519 验签（可断网）。
- **一期不控设备**；不接支付；不做全功能门控 UI。
- **Spike 证据**：`spikes/keygate-mvp/scripts/check.sh` → GO（2026-07-14）。

自建纯 CLI 发证 / 无 server 方案仅作 Keygate 失败后的后备，当前不采用。

## Scope

### In scope（薄接入）

1. App 粘贴 License Key，复用 spike 同一 activate / offline-verify 契约
2. 本地持久化激活态（token + pubkey）；启动时离线可读
3. 未激活时可有提示；可用 env 跳过门控（开发自用）
4. 自动化：离线验签单测（已有 spike Python 缝；App 侧按域层/e2e 补）

### Out of scope（本期）

- 支付、发票、自动发证 webhook
- 设备绑定 / 换机 / seat 强控
- 全功能未激活硬门控、精美商业化激活页、账号体系
- 港企与收款开户（路径二延后）

## Acceptance Criteria（薄接入）

| # | 标准 | 证据 |
|---|------|------|
| AC1 | 运营者可在 Keygate admin 签发买断 License 并看到列表/统计 | spike `check.sh` + 手测 Admin UI |
| AC2 | 用户粘贴 Key 后首次联网激活成功，token 落盘 | App 行为测 / 手测 |
| AC3 | 激活后断网重启仍保持已激活（本地验签） | 域层/集成测 + 手测 |
| AC4 | 篡改 token / 过期 token 被拒绝 | 单测（spike 已覆盖核心缝） |
| AC5 | 可用环境变量跳过激活门控 | App 配置测 |
| AC6 | 支付模块缺失时 License 流程仍可独立完成 | 架构：无支付依赖 |

## User Stories

1. As an operator, I want to issue a buyout license from Keygate admin, so that I can deliver keys manually after offline payment.
2. As a user, I want to paste a license into the app, so that I can activate on first online use.
3. As a user, I want the app to stay activated offline after first activation, so that I do not need continuous network.
4. As a maintainer, I want validation isolated from payments, so that Lemon Squeezy can be added later without rewriting the client contract.

## Open Questions

1. 未激活是否允许有限试用？默认先「提示激活」，试用另开任务。
2. Token 存放路径与是否进 Keychain？实施时定。

## Related

- 路径二（延后）：设计文档 §4
- Spike 已知 Keygate 坑：`spikes/keygate-mvp/README.md`「Spike 结论」
