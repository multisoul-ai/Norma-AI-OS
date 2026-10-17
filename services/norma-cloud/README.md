# Norma 统一云服务

`push` 已合并到本目录；`server.mjs` 是唯一启动入口，推送、实时语音、文字和协作中继共用一个 Node.js 进程与监听端口。生产目标为 `47.251.242.24` 的 `norma-cloud.service`，Nginx 保留原客户端地址；当前部署与回退信息见[统一服务部署记录](../../docs/exec-plans/2026-10-06-norma-cloud-deployment.md#cloud-current)。

## 模块

```text
norma-cloud/
├── server.mjs          # 统一 HTTP / WebSocket 装配与生命周期
├── push/               # APNs、普通通知、ActivityKit 与去重
├── voice/              # 实时语音、豆包协议、Plan ASR/Chat/TTS
├── text/               # 设备准入、文字流与配对登记
├── collaboration/      # Mac / iPhone 在线字节中继
├── cloud/              # Vercel Redis 会话与工具通道
├── api/                # 现有 Vercel 入口
├── deploy/             # 一个 systemd 单元与统一 Nginx 配置
├── scripts/            # 管理工具与人工验收探针
└── tests/              # 模块与单端口集成检查
```

`npm test` 同时运行推送、统一入口和原云端回归；`npm run test:push` 可聚焦 APNs 模块。
推送模块沿用独立 relay key，不接受云端管理员/手机凭据。未设置任何 APNs 配置时只启动云端能力；一旦设置任一推送配置，缺失、损坏或权限错误的凭据必须阻止启动。
推送协议和历史交付证据见 [push/README.md](push/README.md)。

## 常驻部署

统一 systemd 单元为 `deploy/norma-cloud.service`，运行目录 `/opt/norma-cloud/current`，账号为 `norma-cloud`；Node 22.23.2 放在 `/opt/norma-cloud/node-v22.23.2-linux-x64/bin/node`。
主环境文件 `/etc/norma-cloud/gateway.env` 配置 `HOST=127.0.0.1`、`PORT=8789`、`PUBLIC_ORIGIN=https://soulvoiceai.com/norma/voice` 及原云端配置。可选 `/etc/norma-cloud/push.env` 使用 `deploy/push.env.example` 的五个推送字段；不再使用 `NORMA_PUSH_PORT`。
`.p8` 与 relay key 必须归 `norma-cloud` 所有且权限 0600；环境文件归 root 所有、0600。设备状态继续使用 `/var/lib/norma-cloud/devices.json`，迁移时保留内容并调整账号权限，不清空准入表。

| 公网入口 | 统一后端路由 |
|---|---|
| `/norma/v1/notifications` | `127.0.0.1:8789/v1/notifications` |
| `/norma/healthz` | `127.0.0.1:8789/healthz` |
| `/norma/voice/*` | `127.0.0.1:8789/*` |

Nginx 只引入 `deploy/nginx-front.conf` 一次，并移除旧 push/voice 片段的 include，避免重复 location。`/norma/` 只放行推送及其健康路径，禁止通过它访问云端管理接口；`/norma/voice/v1/admin/` 仍返回 404。推送请求限制保持 4 KiB / 12 秒，文字请求保持 32 KiB / 流式不缓冲。
部署前在目标主机创建服务账号、准备 Node 与配置，安装完整目录并运行 `npm ci --omit=dev`；先用独立回环端口验证候选，再安装新单元、执行 `systemctl daemon-reload`，启动 `norma-cloud`。Nginx 片段安装为 `/etc/nginx/norma-cloud-front.conf`，`nginx -t` 成功后 reload。统一服务迁移时停用旧 `norma-push`、`norma-voice` 单元；SFOA 保持关闭，保留备份供回退。

以下为各能力协议与历史交付记录。

从 companion 的 `server/` 迁入，源仓库保持只读，文件来源与 SHA-256 见
[`2026-09-13-realtime-source.json`](../../docs/design-docs/2026-09-13-realtime-source.json)。
Node.js 22+；仅依赖既有 `ws`。生产地址为 `https://soulvoiceai.com/norma/voice`，部署验证见[交付记录](../../docs/exec-plans/2026-10-06-norma-cloud-deployment.md#cloud-superseded)。

```sh
cd services/norma-cloud
npm ci
cp .env.example .env
# 编辑 .env，填入访问口令和豆包 Plan API Key；本地运行改用回环 PUBLIC_ORIGIN/PORT，删除设备状态路径或改为本地可写文件。
npm test
npm start
```

旧版实时语音的默认 iOS 无需填写地址、Key 或访问口令；它目前仍从已配对 Mac 领取签名授权。新增受邀设备文字入口使用独立云端设备令牌，不依赖 Mac。供应商长期 Key 始终只留在服务器。
`COMPANION_ACCESS_TOKEN` 沿用原字段名，作为服务端管理/本地调试口令；正式手机用 Mac 签发的 24 小时授权替代。
该 Mac 签名只适用于旧语音路由；`/v1/text/turn` 不接受它作为设备准入。
授权经已配对的 pin TLS 通道领取，使用现有 Mac P-256 安装身份签名，固定 audience；手机钥匙串缓存，最后一小时尝试刷新，Mac 离线时未过期授权仍可聊天。到期需重新连接 Mac；撤销或只读设备不能领取新授权，已签发授权最长到当天到期。
`DOUBAO_PROTOCOL=plan` 开启 ASR → 豆包 Chat → TTS，并支持 Mac 工具和成果播报。
`2` / `3` 保留原语音协议，仅提供聊天和角色行为。
自定义网关地址与 `PUBLIC_ORIGIN` 必须相同，可包含挂载前缀；真机使用 HTTPS/WSS，HTTP 只允许回环开发。
统一部署由 Nginx 将 `/norma/voice/` 转发到 `127.0.0.1:8789/` 并剥离前缀，媒体与工具票据保留公网前缀。部署配置见 `deploy/`。
`norma-cloud.service` 使用 Node 22 常驻单实例，内存维护一次性票据；不需要 Redis，不受 Vercel 300 秒执行上限影响，单次通话仍受现有 35 分钟上限约束。
新版 iOS 自动迁移两个旧默认地址，保留自定义部署；新服务器只为此次迁移兼容旧 Norma audience 的有效 Mac 签名，证书登记、设备绑定和到期规则不变。Mac 工具通道的子路径支持需要更新 Mac。

旧 Vercel 入口仍保留以下部署方式；它不配置 APNs，推送模块只用于常驻单实例：
Vercel 使用现有 Redis 集成与独立 `norma-voice:<管理口令SHA256前24位>:` 命名空间：跨实例一次性票据、45秒独占租约及设备隔离的已播历史。
媒体与Mac工具连接可落到不同实例，控制消息由有界Redis队列转发；音频仍只走WebSocket。云端供应商请求使用Node原生HTTPS，保留流式响应、取消与禁止跳转。每个私有部署同时仅一通电话，240秒空闲轮换，280秒强制结束以避开300秒平台上限。超过3秒音频突发仍限流，但以可重试错误交给手机恢复；正常挂断在上游结束及云端租约释放后确认。

常驻生产环境设置 `COMPANION_ACCESS_TOKEN`、`DOUBAO_PROTOCOL=plan`、`DOUBAO_SPEECH_API_KEY`、`PUBLIC_ORIGIN` 和 `NORMA_VOICE_TRUSTED_CERTIFICATES`；只有 Vercel 入口额外需要 `KV_REST_API_URL`、`KV_REST_API_TOKEN`。
最后一项是受信任 Mac 的 DER 公开证书 base64 字符串数组；当前仅登记此用户正式 Mac 安装，最多8个。新安装由服务管理员登记公开证书，私钥不离开Mac。不要使用已脱敏的环境导出值作为供应商Key。

`npm run deploy:vercel` 使用 Vercel CLI 58.0.0，仅上传本目录运行文件；`.vercelignore` 排除环境、测试和探针。当前 Nginx 生产部署使用上面的常驻单元；`npm run test:cloud` 需要真实Redis环境变量，随机命名空间不触碰正式会话。

手机从 `/v1/session` 领取一次性媒体票据。Plan 的 `session.created` 声明
`doubao_…` 媒体身份及 `task_tools` / `reports` 能力；旧网关不声明能力时，App 保持聊天模式。
当前媒体上的 `gateway.tools.request` 签发有效期 30 秒的一次性 `/v1/tools` 票据。
手机通过已配对连接把票据和目标项目交给 Mac；Mac 唯一执行任务工具，回传既有持久回执。
网关只在收到 Mac 结果及 `response.create` 后续答；连接丢失或回执超时保持 unknown，不重试写操作。

`gateway.report` 对已核对的成果文本直接调用 TTS，不调用 LLM 或工具，也不加入讨论历史。
App 根据真实音频播放回调标记“已播”，并保留独立的用户验收状态。忙时延后，暂停/插话仍保留待播记录。

`npm test` 使用真实 HTTP/WebSocket 与本机 ASR/Chat/TTS 替身，覆盖票据鉴权、过期/重用/媒体撤销、
PCM、插话、回执续答和逐字成果播报；不使用云端 Key，不代表云端模型工具支持或物理耳机性能已验收。

## 受邀 iPhone 文字入口（#58 服务端阶段）

先为常驻服务设置 `NORMA_CLOUD_DEVICE_STATE=/var/lib/norma-cloud/devices.json`。systemd 配置的
`StateDirectory=norma-cloud` 提供唯一可写目录；Nginx 必须使用本仓库更新的配置，阻断公网
`/norma/voice/v1/admin/`，并关闭文字流的代理缓冲。旧公网基址及三个语音路由保持不变。
状态文件不存在时可创建空准入表；文件损坏或写入失败时新准入/文字路由返回 503，绝不将损坏文件当作空表覆盖。

管理员在**服务器本机**运行以下命令，令牌只存在服务器环境变量中：

```sh
node scripts/device-admin.mjs invite
node scripts/device-admin.mjs list
node scripts/device-admin.mjs revoke <device-id>
```

`invite` 打印 7 天有效、只能兑换一次的 `norma://invite?code=…` 链接。App 使用
`POST /v1/devices/claim` 兑换后把设备令牌放在 Keychain；本服务只保存哈希。
`POST /v1/text/turn` 验证设备和额度，以 NDJSON 返回 `start`、`delta`、`done` 或 `error`。
每天每设备默认上限 300 回合，可用 `NORMA_TEXT_DAILY_LIMIT` 调整；并发每设备 1 回合。
服务端不持久保存聊天正文，iPhone 本地历史与原生页面仍需按 #58 后续阶段接入和验收。

当前受邀文字实现及待发布证据见[09-29 发布计划](../../docs/exec-plans/2026-10-06-norma-cloud-deployment.md#cloud-text-2026-09-29)。Nginx 的文字请求上限须为 32 KiB，并关闭响应缓冲；旧 4 KiB 代理配置会在有历史上下文时提前拒绝请求。页面与本机历史已接入 W07 同屏文字模式，真实云服务与设备验收尚未完成。

本地 Swift 客户端与正式网关 HTTP 联调：`npm --prefix services/norma-cloud run test:native:text`。它自动启动回环服务并运行 `CloudTextNetworkTests`，使用临时设备状态和确定的测试供应商，覆盖中文增量、完整结束、历史恢复、重复请求、中断及撤销；完成后清理服务与临时凭据，不访问生产环境。该检查运行 macOS 上的共享 Swift 核心，不等同于 iOS 页面、Keychain 或真实模型验收。

## 协作中继（#57 / #59）

旧 `POST /v1/threads/plan` 已退役，返回 404；`tests/check-thread-retirement.mjs` 已进入 `npm test`。当前工作空间聊天和任务执行由 Mac 承担，手机直连与云中继进入相同的 Mac 协议处理器；云中继只负责在线转发。

常驻网关增加 `/v1/relay/mac` 与 `/v1/relay/phone`。Mac 复用安装证书签署一次性挑战，
服务端通过 `NORMA_RELAY_TRUSTED_CERTIFICATES`（DER 公开证书 base64 数组）限制可上线的 Mac；
手机使用既有受邀设备凭据，撤销后在线连接也停止转发。空证书清单默认关闭 Mac 入口。
中继只转发在线通道的不透明字节，Mac 离线返回 offline；不存任务正文，不代 Mac 产生已送达或执行完成回执。
具体容量与协议见 [#59 规格](../../docs/product-specs/2026-09-28-norma-ios-issues-spec.md#issue-59)。

`npm run test:relay` 使用临时 P-256 证书、真实邀请准入与本机 WebSocket，覆盖签名重放、重复 Mac、
双向转发、跨连接隔离、撤销、断线、重启和限额；无真实模型调用。它已进入 `npm test`。
双端已接入中继与端到端加密会话，Mac 负责持久回执及执行。2026-10-06 的服务器候选验证与公网 HTTPS/WSS 路由、鉴权检查已通过；
真实 iPhone 蜂窝网络任务与断线恢复尚未验收，不得把服务端网络检查当作完整任务验收。旧 Vercel 入口不承载此常驻中继。

线上音频验收使用 `scripts/probe-plan-call.mjs`（源自 companion 同名探针），显式指定 `NORMA_PROBE_PCM`（PCM16/16kHz/单声道）、`NORMA_PROBE_OUTPUT`（已存在目录）及服务端管理口令，再运行：

```sh
node scripts/probe-plan-call.mjs --gateway=https://soulvoiceai.com/norma/voice --deployment
```

探针只上传预生成语音，检查匿名拒绝、真实ASR/Chat/PCM、工具通道和独立TTS，不采集现场麦克风。输出PCM是接收证据，不等同于手机实际扬声器播放。使用HTTP代理的本机需 Node.js 24.5+ 和 `NODE_USE_ENV_PROXY=1`。
