# Norma 云端协作中继部署

2026-10-06：按用户要求部署到 `47.251.242.24`，复用现有 HTTPS 证书、Node 22 与 `norma-voice` 常驻网关。核心中继已上线；真实 iPhone 蜂窝网络任务与断线恢复尚未验收。

## 代码与运行位置

- 源码：`services/norma-cloud/`；入口 `server.mjs`，中继 `collaboration-relay.mjs`，配对注册 `paired-registration.mjs`。
- 公网基址：`https://soulvoiceai.com/norma/voice`；Mac `/v1/relay/mac`，手机 `/v1/relay/phone`，使用 WSS。
- 发布目录：`/opt/norma-voice/releases/20261006-121416-relay`；`/opt/norma-voice/current` 指向该目录。上传 8 个运行模块与 `package.json`、`package-lock.json`，安装既有 `ws@8.18.3`；没有上传本地环境文件或测试私钥。
- 服务：`norma-voice`，`active/running`、开机启动，非 root 用户，监听 `127.0.0.1:8789`；没有替换服务器其他 Node 安装。
- Nginx：`/etc/nginx/norma-voice-front.conf`，由 `soulvoiceai.com` 的 HTTPS server include；剥离 `/norma/voice/` 前缀，转发 WebSocket，关闭缓冲，请求大小为 32 KiB，公网 `/v1/admin/` 返回 404。
- 保留原管理口令、供应商配置及语音可信证书；中继准入复用已被管理员登记的 Mac 公开证书，没有新增测试 Mac 身份。
- 设备状态：`/var/lib/norma-cloud/devices.json`；systemd drop-in `/etc/systemd/system/norma-voice.service.d/relay-state.conf` 设置 `StateDirectory=norma-cloud` 与 `StateDirectoryMode=0700`，目录归服务用户所有。

## 验证

1. 本地 `npm test` 全部通过：语音与 Plan、工具票据、文字准入、旧线程规划退役、中继、配对注册。
2. 使用实际 systemd 环境和服务用户验证独立候选实例：既有证书准入、设备状态可写、真实邀请兑换、手机 WebSocket、Mac 离线返回、撤销拒绝；临时状态清理，未创建生产测试手机。
3. 生产 10 个运行/包文件及 Nginx 挂载配置的 SHA-256 与验证快照一致，见 [manifest.json](evidence/norma-cloud-relay-20261006/manifest.json)。
4. 从开发机直连目标 IP，保留域名、证书验证及 SNI，公网检查通过，见 [public-check.json](evidence/norma-cloud-relay-20261006/public-check.json)：
   - 健康 200；匿名配对注册、语音换票和手机中继均 401。
   - 公网管理接口 404。
   - Mac WSS 成功升级为 101，返回绑定生产基址的随机签名挑战；非法身份/签名被 1008 `unauthorized` 拒绝。
5. `nginx -t` 通过；原有重复 `text/html` MIME 警告仍存在，未修改无关配置。

这些检查确认新版路由和鉴权已上线，没有验证真实 Mac 签名上线、手机端到端加密任务执行、模型调用、麦克风/扬声器或蜂窝网络重连。客户端仍需与当前协议匹配；Debug Mac 仅使用显式本地中继配置，正式 Release 使用上述公网基址。

## 回退与交付边界

部署准备期间，旧网关与 Nginx 挂载于服务器时间 20:14 被另一操作停用/移除。首次候选部署的本机健康检查成功，公网挂载检查 404，因此自动恢复旧发布及配置；随后仅补回该云服务的 HTTPS include，重新部署并验证成功，没有恢复其他已停用的 `/norma/` 服务。

最终切换前备份在 `/opt/norma-voice/backups/20261006-121416-relay-mount/`，目录仅 root 可读，保存旧环境、Nginx 挂载/主配置及前一发布目标 `releases/20260915-0045`。首轮回退备份另存 `/opt/norma-voice/backups/20261006-121416-relay/`。

回退应先读取 `previous-target.txt`，恢复 `current` 目标及该次备份的环境/挂载配置，按需移除本次新增 drop-in；`systemctl daemon-reload` 后重启网关，经 `nginx -t` 再 reload。主 Nginx 配置存在他人后续修改时，只移除本次新增 include，不整份覆盖；保留已产生的设备状态，避免丢失注册或撤销记录。重启会断开当时的长连接。

本次没有更新用户 Mac 安装包或 iPhone 安装包，没有声称真实 4G/5G 任务链路已验收，也没有提交或推送代码。

## 当前正式 Mac 证书修复（2026-10-06）

真机验收准备时确认，已安装 Mac 的身份为 `mac-9409cef99d3545bade8b61729ef303394f5b1f4403ae911ebc50bfda9854bf60`，与云端原准入名单中的旧安装身份不同。按既有服务器授权，将当前安装的公开证书追加到 `NORMA_RELAY_TRUSTED_CERTIFICATES` 并重启网关；保留旧条目与原语音证书名单，没有上传私钥。公开证书留在 [current-mac-public.pem](evidence/norma-cloud-relay-20261006/current-mac-public.pem)。

配置备份：`/etc/norma-voice/gateway.env.pre-current-mac-20261006`，权限 0600。网关重启后健康检查成功，真实 Mac 已建立到 `47.251.242.24:443` 的出站连接；此 TCP 证据本身不等于手机任务端到端验收通过。

用户已连接 iPhone 11 并开启开发者模式；Apple 设备工具可读取正式 Norma build 52。原生自动化首次执行被开发签名配置阻止：现有开发描述文件不包含此手机，未执行测试主体。用户随后要求重打包安装 Mac 并发布 iOS，由用户验收，后续交付见 [双端自测包记录](2026-10-06-mobile-self-test-release.md)。iPhone 11 的蜂窝验收范围为 4G；5G 需要支持该网络的设备。
