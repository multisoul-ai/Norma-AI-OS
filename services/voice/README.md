# Norma 语音网关

从 companion 的 `server/` 迁入，源仓库保持只读，文件来源与 SHA-256 见
[`2026-09-13-realtime-source.json`](../../docs/design-docs/2026-09-13-realtime-source.json)。
Node.js 22+；仅依赖既有 `ws`。独立生产地址为 `https://norma-voice.vercel.app`，部署验证见[交付记录](../../docs/exec-plans/2026-09-14-voice-vercel-deployment.md)。

```sh
cd services/voice
npm ci
cp .env.example .env
# 编辑 .env，填入你自己的访问口令和豆包 Plan API Key。
npm test
npm start
```

默认 iOS 无需填写地址、Key 或访问口令。新版 Mac 开启 Mobile Connection，手机配对并取得项目控制权限后自动领取签名授权，供应商长期 Key 只留在 Vercel。
`COMPANION_ACCESS_TOKEN` 沿用原字段名，作为服务端管理/本地调试口令；正式手机用 Mac 签发的 24 小时授权替代。
授权经已配对的 pin TLS 通道领取，使用现有 Mac P-256 安装身份签名，固定 audience；手机钥匙串缓存，最后一小时尝试刷新，Mac 离线时未过期授权仍可聊天。到期需重新连接 Mac；撤销或只读设备不能领取新授权，已签发授权最长到当天到期。
`DOUBAO_PROTOCOL=plan` 开启 ASR → 豆包 Chat → TTS，并支持 Mac 工具和成果播报。
`2` / `3` 保留原语音协议，仅提供聊天和角色行为。
自定义网关地址与 `PUBLIC_ORIGIN` 必须相同；真机使用 HTTPS/WSS，HTTP 只允许回环开发。
Vercel 使用现有 Redis 集成与独立 `norma-voice:<管理口令SHA256前24位>:` 命名空间：跨实例一次性票据、45秒独占租约及设备隔离的已播历史。
媒体与Mac工具连接可落到不同实例，控制消息由有界Redis队列转发；音频仍只走WebSocket。云端供应商请求使用Node原生HTTPS，保留流式响应、取消与禁止跳转。每个私有部署同时仅一通电话，240秒空闲轮换，280秒强制结束以避开300秒平台上限。超过3秒音频突发仍限流，但以可重试错误交给手机恢复；正常挂断在上游结束及云端租约释放后确认。

生产环境设置 `COMPANION_ACCESS_TOKEN`、`DOUBAO_PROTOCOL=plan`、`DOUBAO_SPEECH_API_KEY`、`PUBLIC_ORIGIN`、`KV_REST_API_URL`、`KV_REST_API_TOKEN` 和 `NORMA_VOICE_TRUSTED_CERTIFICATES`。
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

线上音频验收使用 `probe-plan-call.mjs`（源自 companion 同名探针），显式指定 `NORMA_PROBE_PCM`（PCM16/16kHz/单声道）、`NORMA_PROBE_OUTPUT`（已存在目录）及服务端管理口令，再运行：

```sh
node probe-plan-call.mjs --gateway=https://norma-voice.vercel.app --deployment
```

探针只上传预生成语音，检查匿名拒绝、真实ASR/Chat/PCM、工具通道和独立TTS，不采集现场麦克风。输出PCM是接收证据，不等同于手机实际扬声器播放。使用HTTP代理的本机需 Node.js 24.5+ 和 `NODE_USE_ENV_PROXY=1`。
