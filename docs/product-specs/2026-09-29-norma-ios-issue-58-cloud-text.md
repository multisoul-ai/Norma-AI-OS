# #58｜Norma 单聊主屏的云端文字输入实施规格

状态：实现中，待真实云服务及 UI 验收 · 2026-09-29。依据 [01–05 Goal](../exec-plans/2026-09-28-norma-ios-issues-01-05-native-goal.md)、[云服务架构](../design-docs/2026-09-28-norma-cloud-service-architecture.md)、[存储矩阵](2026-09-28-norma-ios-data-storage-matrix.md)与 GitHub #58。本规格交付 **W07 Norma 单聊主屏中的文字输入能力**，不新增独立的「Norma 文字对话」页面或菜单模块；派工中继、Workspace 共享历史和实时通话分别属于后续切片。

## 用户流程与边界

1. 受邀者在 iPhone 上打开一次性邀请链接，App 自动向默认云服务登记设备并把返回的设备凭据保存到 Keychain。邀请由云服务管理员在服务器上生成；用户不填模型、供应商 Key、服务地址或 Mac 配对信息。未受邀设备不能消费模型。退出/重装后若本机凭据丢失，重新邀请；首版不提供云账户或换机恢复。
2. 用户点底部「Norma」进入 W07 角色主屏；点同屏的「输入文字」切换键盘和私聊历史，返回角色后仍停留在同一主屏。邀请深链也进入此主屏。iPhone 先把用户消息和稳定 `turn_id` 落入本机私聊库，再请求云端。全部 Mac 关闭时流程相同。云端按有界上下文推理、逐段返回 Norma 文字，iPhone 只在收到终止事件后将回复标为完整；途中断线保留用户原文，标明「回复中断，可重试」。
3. 用户可滚动、搜索、重启后继续查看完整私聊历史。每次请求只送必要的最近消息，云端不做长期聊天存储；私聊不会自动加入任何 Workspace/线程，也不出现 Mac「已接收／执行完成」状态。
4. 准入失效、被撤销、当天额度用尽、请求过大、模型不可用、超时分别显示可理解的状态；不自动重复一个结果未知的回合。用户主动重试时产生新 `turn_id`，本机保留前一次错误记录与原文。

单聊与群聊的页面边界：Norma 私聊以 W07 为唯一主入口，角色、语音控制和文字输入同属一页；Workspace 群聊按 P05/W04，承载具名消息、工作简报、Agent 回执和任务卡。私聊记录不自动注入群聊。旧版独立文字页的视觉 PASS 不作为修正后的验收证据。

## 首版准入与部署契约

- 继续使用当前 HTTPS 基址 `https://soulvoiceai.com/norma/voice`，保留旧 `/v1/session`、`/v1/voice`、`/v1/tools` 与 Mac 签名令牌的原语义；新接口是同一基址下的 `/v1/devices/claim` 和 `/v1/text/turn`。Mac 不签云端通行证，供应商 Key 只在服务器环境中。
- 单实例管理员命令生成 32 字节随机、7 天有效、只能兑换一次的邀请。二维码/链接使用 `norma://invite?code=…`；App 只提取一次性邀请码并将其提交到固定 HTTPS 基址，不把第三方链接当作服务地址。服务器只保存邀请码哈希，不记录明文或 URL。
- `POST /v1/devices/claim` 接受邀请码和至多 80 字符的设备显示名，原子消耗邀请并返回 32 字节随机设备令牌。设备令牌只在本机 Keychain 保存；服务器只保存哈希、设备 ID、创建/撤销时间和额度计数。每个文字请求都重新查撤销状态，撤销立即阻止新回合。首版用受控邀请、TLS、Keychain 和可撤销令牌服务少量设备；不引入尚无用户账户的登录 UI。
- 设备/邀请码/配额状态在服务器私有持久文件中原子写入，权限 `0600`。现有 systemd `ProtectSystem=strict` 保持不变，新增专用 `StateDirectory` 作为唯一可写状态目录；启动时损坏状态文件必须拒绝准入和推理，不能当空名单覆盖。首版单实例；扩容前再迁移共享存储。
- 默认每台设备同时最多 1 个文字回合，每 UTC 日最多 300 回合（服务端可通过 `NORMA_TEXT_DAILY_LIMIT` 调整）；用户输入最多 4096 个 Unicode 字符，上下文最多 24 条、JSON 请求最多 32 KiB，模型输出最多 1024 token。超过上限拒绝且不调用供应商。每日额度在发起上游请求前持久扣除；不因客户端断线自动返还或重发。`429` 带可读的限额原因和 `Retry-After`。

## 文字协议和本机历史

| 阶段 | 契约 |
|---|---|
| 请求 | `POST /v1/text/turn`，`Authorization: Bearer <device-token>`，`Content-Type: application/json`；正文 `{turn_id, messages}`。`turn_id` 为 UUID；消息仅允许 `user`/`assistant`、非空纯文字，最后一条必须为本次 `user`。客户端不能传 `system`、模型名、供应商 URL 或工具定义。服务端自行加入 Norma 文字人格指令。 |
| 流 | `application/x-ndjson`，依次发送 `start {turn_id}`、若干 `delta {text}`、终止的 `done {turn_id, usage?}`。服务端限制单事件与累计回复大小。iPhone 以 UTF-8 按换行增量解析，不把半个 JSON 行或尚未 `done` 的文字存成完整回复。 |
| 异常 | 流开始前：`400 invalid_request`、`401 unauthorized`、`403 revoked`、`409 duplicate_turn`、`413 request_too_large`、`429 quota_exceeded/busy`、`503 provider_unavailable/device_state_unavailable`。流开始后上游超时或失败发送 `error {code: provider_timeout/provider_unavailable}` 后结束；连接直接断开同样视为不完整。错误不含供应商 Key、提示词或原始上游响应。 |
| 防重复 | 每设备持久记录最近的 `turn_id`；同一 ID 再请求返回 `409`，不二次收费或调用模型。断线后 iPhone 不盲目自动重发；用户明确重试才生成新 ID。 |
| 本机记录 | App 私有 SQLite 保存 `message_id`、`turn_id`、作者、正文、创建时间、状态与错误码；用户消息在请求前落盘，Norma 回复在 `done` 后落盘。未完成的临时片段可在界面标「中断」，不能伪装完整答复。完整历史长期保留并可本地搜索；不自动复制给 Mac。 |

文字上游首版复用现有服务器已经配置的豆包 Plan Chat 凭据，使用兼容的流式 Chat 接口；不新增客户端模型选择或新的供应商依赖。云服务只生成文字，不接受桌面工具调用。服务升级顺序为：先在服务器增加新路由和可回滚状态目录，再发布 iOS 入口；回滚新路由不得影响旧语音客户端。

## 验收映射

| 场景 | 自动化 | 真实环境 |
|---|---|---|
| 邀请仅一次、匿名拒绝、撤销立即生效、额度与重复回合 | Node HTTP 端到端测试，使用临时状态目录和假上游；覆盖重启后状态仍在 | 自有测试部署生成一次性邀请，iPhone 领取后撤销并验证拒绝；不使用生产用户令牌作 fixture |
| 流式成功、中途断流、过大输入、供应商超时 | Node 流测试 + Swift 增量解码和本机历史测试 | 用真实云模型进行至少一次中文文字回合，记录服务响应和 App 可见文字，不在日志中保存密钥 |
| Mac 离线、重启恢复、私聊不进入 Workspace | iOS 原生 UI 测试 + 本机存储测试 | Computer Use 操作真实 App：关闭 Mac、发送、退出重开、搜索；核对未出现执行回执 |
| 视觉和无障碍 | 同尺寸原型截图对照；Dynamic Type、减少动态效果、VoiceOver 焦点 | 独立只读 UI Agent 检查浅色内容、Norma 身份、三项底栏/Sheet 的系统材质与触控路径，问题返工复验 |

真机由用户后置验收；当前只允许本地运行，不部署云端。真实模型回合与原生 UI 证据未齐备前，#58 仍保持进行中。旧语音的 Mac 授权入口仅为迁移兼容，不作为本票文字入口的准入方式。


## 开发版本地入口（09-29）

DEBUG 版可通过启动环境 `NORMA_CLOUD_LOCAL_PORT` 与 UUID 格式的 `NORMA_CLOUD_TEST_NAMESPACE` 连接 `http://127.0.0.1:<port>/`。只接受非零 UInt16 端口，不接受自定义主机或 URL，不跟随重定向；显式配置不合法则停止加载/准入/发送，不能静默回退正式地址。

本地数据库位于独立的 `Norma/LocalCloudChat/<port>/<namespace>/`，Keychain service 也含 `.local.<port>.<namespace>`。普通 DEBUG 测试、正式环境和本地网关不共用凭据或历史。Release 不编译本地入口；产品不新增服务器配置页面。本地网络测试已改用这一实际客户端入口，仍使用确定的测试供应商，不代表真实模型验收。
