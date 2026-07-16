# 软件 License 与商业化收款设计

日期：2026-07-14  
状态：Keygate spike **GO**；硬门控接入 SPEC 已定案 → [`2026-07-14-SPEC-software-licensing.md`](../product-specs/2026-07-14-SPEC-software-licensing.md)  
ADR：[`0014`](../adr/0014-spike-keygate-before-app-licensing.md)、[`0015`](../adr/0015-license-hard-gate-outside-workbench.md)  
Spike：[`spikes/keygate-mvp/README.md`](../../spikes/keygate-mvp/README.md)

## 1. 目标与边界

产品模型（已确认）：

- **买断**（perpetual），非订阅
- **允许离线**（接受**首次联网激活**，之后本地验签）
- **一期不控设备**
- **支付与授权解耦**
- **人工发证**（后台手动签发）

## 2. 当前分期

| 阶段 | 内容 | 状态 |
|------|------|------|
| **Spike** | 本机 Docker 起 Keygate；`spikes/keygate-mvp/` 脚本：发证→激活→断网复验 | **GO** |
| **硬门控接入** | GateRoot + `license` crate + 激活壳；env 可跳过 | **下一步**（见 SPEC） |
| **路径二** | 港企 + Lemon Squeezy 收款 | 延后 |
| **no-go 换道** | 优先 Keygen CE → 再自建 Ed25519 | 仅 spike 失败时 |

## 3. Spike 架构

```text
本机 Docker：Keygate (+ Postgres)
        │
运营者：Admin 手动签发买断 License
        │
spikes/keygate-mvp/ 脚本：
  activate（联网）→ 落盘离线凭证 → 断网再 validate
        │
过关 → 薄接入 App（同一校验契约）
不过 → 停 Keygate，改 Keygen CE spike
```

## 4. 路径二（延后）：港企 + Lemon Squeezy

（内容同前：大陆个人需港企主体；LS 买家侧可有支付宝；与授权服务解耦。）

## 5. 选型备注

- Keygate：一体后台 + 统计，契合运营需求；星少、早期，故必须用短 spike 否决。
- Keygen CE：成熟度最高的备选核；CE 无完整 Dashboard，可后补 Portal。
- 自建签名：无 server 时的保底。

调研对比仍见历史章节与对话记录；以本节分期与 ADR 0014 为准。

## 6. 相关文档

- 词汇：[`CONTEXT.md`](../../CONTEXT.md)「软件授权（草案）」
- ADR：[`0014-spike-keygate-before-app-licensing.md`](../adr/0014-spike-keygate-before-app-licensing.md)
- 产品 SPEC：待 spike 结论后按 go/no-go 改写 [`2026-07-14-SPEC-software-licensing.md`](../product-specs/2026-07-14-SPEC-software-licensing.md)
