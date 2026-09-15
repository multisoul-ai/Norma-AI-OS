# Norma 语音服务迁移到自有服务器

2026-09-15：部署到 `47.251.242.24`，复用从 companion 迁入并扩展的 `services/voice`。companion 源目录未修改。

## 已上线

- 基址：`https://soulvoiceai.com/norma/voice`。
- 健康：`GET /norma/voice/health`；换票：`POST /norma/voice/v1/session`。
- 媒体：`wss://soulvoiceai.com/norma/voice/v1/voice`；Mac 工具：同前缀 `/v1/tools?call_id=…`。
- systemd：`norma-voice`，开机启动、非 root 用户，监听 `127.0.0.1:8789`；Nginx 剥离挂载前缀，提供现有证书的 HTTPS/WSS。
- 运行目录：`/opt/norma-voice/current` → `releases/20260915-0045`。独立 Node `22.23.2`，官方 SHA-256 校验通过；现有 Node 20 未替换。
- 配置：`/etc/norma-voice/gateway.env`，`root:root 0600`。豆包 Key、管理口令只留服务端；未上传人物资源、客户端、测试或源仓库环境文件。
- 使用豆包 Plan ASR → Chat → TTS，沿用 Node 原生 HTTPS 适配；单实例内存票据，无 Redis/Vercel 执行时限依赖。

配置模板在 `services/voice/deploy/`。8 个部署运行/配置文件的 SHA-256 均与本地最终版本一致。
启动命令使用工作目录内的相对入口 `server.mjs`，避免发布目录软链接与 ES module 入口判定不一致而直接退出。

## 客户端迁移

iOS 默认地址已改为新基址；自动迁移旧 Norma、Companion 默认值，保留用户自定义地址。
媒体与工具票据校验保留同源、同挂载目录、协议、过期和凭据边界，拒绝相邻服务与编码斜杠。
Mac 新签名 audience 使用新基址；新服务定向兼容旧 Norma audience 的有效签名，登记证书、设备归属、24 小时有效期和撤销后的刷新限制不变。
无需手机配置供应商 Key 或管理口令。已安装的旧 iOS 不会自动获得代码；新路径的 Mac 工具能力也需要更新 Mac。本轮未发布 TestFlight 或安装到用户手机。

## 验证证据

- 公网 HTTPS 健康 200、匿名换票 401；显式绕过 curl 代理后解析并连接 `47.251.242.24`。现有网站 200、推送 `/norma/healthz` 200，三个服务均 active。
- 公网合成语音探针通过：ASR 转写、真实豆包回复、Mac 工具 WSS、独立成果 TTS、关闭 ACK。回复 PCM 259056 bytes，播报 PCM 114448 bytes，总耗时 23.146 秒；探针未使用代理环境变量、未采集现场麦克风。
- 首次公网探针收到两轮音频，但在收尾时因上传积压触发 `audio_rate_limit`；同一部署、同一限流参数复测完整通过。未放宽限流，也不能据单次通过承诺手机网络稳定性。
- 原生 iOS L3：真实配对并自动获得 Mac 签名 → 新域名 HTTP 200 → 豆包 WSS ready → close ACK，1/0；未配置供应商 Key、未采麦克风。
- Swift Package 81/0；网关 `npm test` 通过；Mac 工具及跨语言签名集成 8/0；`cargo test -p norma`、`cargo check -p app`、diff/行数/no-allow/指南护栏通过。
- 全仓检查未全绿：`cargo test --workspace` 被另组未提交改动中 `mobile_codex_archive_reconnect_tests.rs:107` 缺少 `isolated_config` 阻断；扩大 e2e 时 224 通过、3 失败、60 忽略，失败是两个 `e2e_mobile_plans` 网络等待用例及 `e2e_realtime_roster` 未打开卡片名单用例。本次未修改这些实现或测试。

本机详细证据：`/tmp/norma-voice-soulvoice-probe-retry/vercel-plan-call-result.json`（沿用探针历史文件名）、`/tmp/norma-voice-native-soulvoice.log`、`/tmp/norma-voice-mac-mount-green.log`、`/tmp/norma-voice-deployed-sha256.json`。
用户手机关闭 VPN 后的实际麦克风/扬声器表现仍待安装新版后验证。

## 运维与回退

SSH 端口 `2222`。通过 `systemctl status norma-voice`、`journalctl -u norma-voice` 与公网 `/health` 检查服务；修改环境后用 `systemctl restart norma-voice`，会终止当时通话。
Nginx 配置在 `/etc/nginx/norma-voice-front.conf`，由现有 HTTPS server include。更长的 `/norma/voice/` 前缀不影响 `/norma/` 推送；静音和空闲工具连接的代理超时为 60 分钟，长于网关 35 分钟通话上限。
如需撤下新服务，仅删除主配置中的 voice include，经 `nginx -t` 后 reload，再停止 `norma-voice`。主配置添加前的备份为 `/etc/nginx/nginx.conf.pre-norma-voice-20260915`；存在后续其他修改时不要整份覆盖恢复。
旧 Vercel 部署未删除；客户端回退需要相应版本和 audience 配套，不能只把新客户端地址切回旧服务。
