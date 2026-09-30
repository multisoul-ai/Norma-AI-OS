# Norma 云服务（现有语音网关）

源码目录已从 `services/voice` 改为 `services/norma-cloud`。当前线上公网路径 `/norma/voice`、systemd 单元 `norma-voice` 和运行目录 `/opt/norma-voice/current` 仍沿用原名称；本次只调整仓库目录，不迁移线上部署。

从 companion 的 `server/` 迁入，源仓库保持只读，文件来源与 SHA-256 见
[`2026-09-13-realtime-source.json`](../../docs/design-docs/2026-09-13-realtime-source.json)。
Node.js 22+；仅依赖既有 `ws`。生产地址为 `https://soulvoiceai.com/norma/voice`，部署验证见[交付记录](../../docs/exec-plans/2026-09-15-voice-server-deployment.md)。

```sh
cd services/norma-cloud
npm ci
cp .env.example .env
# 编辑 .env，填入你自己的访问口令和豆包 Plan API Key。
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
当前生产由 Nginx 将 `/norma/voice/` 转发到 `127.0.0.1:8789/` 并剥离前缀，媒体与工具票据保留公网前缀。部署配置见 `deploy/`。
`norma-voice.service` 使用独立 Node 22 常驻单实例，内存维护一次性票据；不需要 Redis，不受 Vercel 300 秒执行上限影响，单次通话仍受现有 35 分钟上限约束。
新版 iOS 自动迁移两个旧默认地址，保留自定义部署；新服务器只为此次迁移兼容旧 Norma audience 的有效 Mac 签名，证书登记、设备绑定和到期规则不变。Mac 工具通道的子路径支持需要更新 Mac。

旧 Vercel 入口仍保留以下独立部署方式：
Vercel 使用现有 Redis 集成与独立 `norma-voice:<管理口令SHA256前24位>:` 命名空间：跨实例一次性票据、45秒独占租约及设备隔离的已播历史。
媒体与Mac工具连接可落到不同实例，控制消息由有界Redis队列转发；音频仍只走WebSocket。云端供应商请求使用Node原生HTTPS，保留流式响应、取消与禁止跳转。每个私有部署同时仅一通电话，240秒空闲轮换，280秒强制结束以避开300秒平台上限。超过3秒音频突发仍限流，但以可重试错误交给手机恢复；正常挂断在上游结束及云端租约释放后确认。

常驻生产环境设置 `COMPANION_ACCESS_TOKEN`、`DOUBAO_PROTOCOL=plan`、`DOUBAO_SPEECH_API_KEY`、`PUBLIC_ORIGIN` 和 `NORMA_VOICE_TRUSTED_CERTIFICATES`；只有 Vercel 入口额外需要 `KV_REST_API_URL`、`KV_REST_API_TOKEN`。
最后一项是受信任 Mac 的 DER 公开证书 base64 字符串数组；当前仅登记此用户正式 Mac 安装，最多8个。新安装由服务管理员登记公开证书，私钥不离开Mac。不要使用已脱敏的环境导出值作为供应商Key。

`npm run deploy:production` 使用 Vercel CLI 58.0.0，仅上传本目录运行文件；`.vercelignore` 排除环境、测试和探针。`npm run test:cloud` 需要真实Redis环境变量，随机命名空间不触碰正式会话。

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
node device-admin.mjs invite
node device-admin.mjs list
node device-admin.mjs revoke <device-id>
```

`invite` 打印 7 天有效、只能兑换一次的 `norma://invite?code=…` 链接。App 使用
`POST /v1/devices/claim` 兑换后把设备令牌放在 Keychain；本服务只保存哈希。
`POST /v1/text/turn` 验证设备和额度，以 NDJSON 返回 `start`、`delta`、`done` 或 `error`。
每天每设备默认上限 300 回合，可用 `NORMA_TEXT_DAILY_LIMIT` 调整；并发每设备 1 回合。
服务端不持久保存聊天正文，iPhone 本地历史与原生页面仍需按 #58 后续阶段接入和验收。

当前受邀文字实现及待发布证据见[09-29 发布计划](../../docs/exec-plans/2026-09-29-norma-cloud-text-release.md)。Nginx 的文字请求上限须为 32 KiB，并关闭响应缓冲；旧 4 KiB 代理配置会在有历史上下文时提前拒绝请求。页面与本机历史已接入 W07 同屏文字模式，真实云服务与设备验收尚未完成。

本地 Swift 客户端与正式网关 HTTP 联调：`npm --prefix services/norma-cloud run test:native:text`。它自动启动回环服务并运行 `CloudTextNetworkTests`，使用临时设备状态和确定的测试供应商，覆盖中文增量、完整结束、历史恢复、重复请求、中断及撤销；完成后清理服务与临时凭据，不访问生产环境。该检查运行 macOS 上的共享 Swift 核心，不等同于 iOS 页面、Keychain 或真实模型验收。

## 协作中继（#57 / #59 本地实施中）

线程编排使用 `POST /v1/threads/plan`：与文字回合共用受邀手机准入、额度、并发与去重，调用同一豆包文字模型并完整验证结构化候选。只返回 Norma 回复与具名任务建议，不调用 Mac、不产生执行回执；来源由请求回填，拒绝未知/重复成员、额外执行字段、截断和超预算候选，生成期间撤权则丢弃候选。请求上限 64 KiB；每项任务与冻结快照合计不超过 Mac 协议的 32 KiB。契约见 [#59 规格](../../docs/product-specs/2026-09-29-norma-ios-issue-59-single-mac.md)。

`node check-thread-plan.mjs` 已加入 `npm test`，通过回环 HTTP 检查上述边界，供应商是测试替身。当前只完成服务端；手机原子保存/派工入口及真实模型验收待接入，`@Norma` 尚不可用。本阶段不部署；未来启用公网入口前需同步代理的请求大小配置。

常驻网关增加 `/v1/relay/mac` 与 `/v1/relay/phone`。Mac 复用安装证书签署一次性挑战，
服务端通过 `NORMA_RELAY_TRUSTED_CERTIFICATES`（DER 公开证书 base64 数组）限制可上线的 Mac；
手机使用既有受邀设备凭据，撤销后在线连接也停止转发。空证书清单默认关闭 Mac 入口。
中继只转发在线通道的不透明字节，Mac 离线返回 offline；不存任务正文，不代 Mac 产生已送达或执行完成回执。
具体容量与协议见 [#59 规格](../../docs/product-specs/2026-09-29-norma-ios-issue-59-single-mac.md)。

`npm run test:relay` 使用临时 P-256 证书、真实邀请准入与本机 WebSocket，覆盖签名重放、重复 Mac、
双向转发、跨连接隔离、撤销、断线、重启和限额；无真实模型调用。它已进入 `npm test`。
当前只是中继基础能力，端到端加密会话、原生客户端、线程持久回执与真实执行仍待接入；
不得把该网络测试当作 iPhone 到 Mac 的完整任务验收。未部署线上，旧 Vercel 入口不承载此常驻中继。

线上音频验收使用 `probe-plan-call.mjs`（源自 companion 同名探针），显式指定 `NORMA_PROBE_PCM`（PCM16/16kHz/单声道）、`NORMA_PROBE_OUTPUT`（已存在目录）及服务端管理口令，再运行：

```sh
node probe-plan-call.mjs --gateway=https://soulvoiceai.com/norma/voice --deployment
```

探针只上传预生成语音，检查匿名拒绝、真实ASR/Chat/PCM、工具通道和独立TTS，不采集现场麦克风。输出PCM是接收证据，不等同于手机实际扬声器播放。使用HTTP代理的本机需 Node.js 24.5+ 和 `NODE_USE_ENV_PROXY=1`。
