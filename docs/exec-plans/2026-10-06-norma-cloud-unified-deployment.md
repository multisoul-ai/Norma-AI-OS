# Norma 统一云服务部署

2026-10-06 21:52（Asia/Shanghai）：按用户授权，将 `services/norma-cloud` 部署到 `47.251.242.24`，替代原独立 push / voice 进程。统一服务 `norma-cloud` 为 active/running、enabled；旧 `norma-push`、`norma-voice` 以及 SFOA 均为 inactive、disabled。

## 当前配置

| 项目 | 值 |
|---|---|
| 发布目录 | `/opt/norma-cloud/releases/20261006T134432Z` |
| 运行目录 | `/opt/norma-cloud/current`，指向上述发布目录 |
| 启动入口 | `server.mjs`，服务账号 `norma-cloud`，Node 22.23.2 / ws 8.18.3 |
| 监听 | `127.0.0.1:8789`，单进程、单端口 |
| systemd | `/etc/systemd/system/norma-cloud.service`，对应仓库 `services/norma-cloud/deploy/norma-cloud.service` |
| Nginx | `/etc/nginx/nginx.conf` 仅引入 `/etc/nginx/norma-cloud-front.conf`，对应仓库 `services/norma-cloud/deploy/nginx-front.conf` |
| 环境 | `/etc/norma-cloud/gateway.env`、`/etc/norma-cloud/push.env`，root 所有、0600 |
| 私钥 | `/etc/norma-cloud/apns.p8`、`/etc/norma-cloud/relay.key`，服务账号所有、0600；配置目录 root:服务组、0750 |
| 设备状态 | `/var/lib/norma-cloud/devices.json`；目录服务账号所有、0700 |

原管理口令、供应商 Key、Mac 公开证书名单和推送凭据原值保留，只迁移路径。切换前设备状态文件尚不存在，未创建或清空正式准入表；候选实例使用独立状态目录。

公网继续使用 `https://soulvoiceai.com/norma/v1/notifications`、`/norma/healthz` 和 `/norma/voice/*`。Mac / iOS 现有默认 URL 与签名 audience 已匹配，无需修改客户端或重新配对。仓库 `.env.example` 已改为正式部署默认值，旧 Vercel 命令改名 `deploy:vercel`。

## 验证

- 本地完整回归及 Swift HTTP 联调已通过；本轮服务器 Node 22 下再次运行 `npm test`，完整通过。
- 候选 `127.0.0.1:18789` 和正式 `127.0.0.1:8789` 各通过 13 项检查；正式 HTTPS/WSS 通过 17 项检查。两个健康入口均 200，匿名请求及跨模块凭据均拒绝；正确凭据加非法内容返回 400。
- Nginx 阻断两个公开管理前缀，以及通过推送前缀访问 session/text 的路径。媒体、工具、手机中继的无凭据 WebSocket 均 401；Mac 中继返回 101 和原 audience 的随机挑战，非法身份以 1008 拒绝，无测试 Mac 登记。
- 从开发机直连公网验证两个健康入口、两个管理封锁和网站首页：状态符合预期，实际 IP 为 `47.251.242.24`，TLS 校验成功。服务器本机对网站首页、既有 `/api/` 和 MarketBrief 首页抽查，切换前后状态与响应正文 SHA-256 均一致。
- 44 个发布文件与本地 SHA-256 清单一致；旧端口 8788、3030 及候选端口 18789 均不监听。未发送真实推送，未调用付费模型；真实 iPhone 通知、通话及蜂窝任务不属于本次验收证据。

`nginx -t` 和 systemd 单元验证通过。Nginx 原配置的重复 `text/html` MIME 警告仍存在。首次公网检查失败后自动恢复旧服务；重试在 reload 后等待公开推送健康入口就绪，再执行完整检查，最终通过。

## 备份与回退

服务器 root 专用备份目录：`/root/norma-cloud-backups/20261006T134432Z`，0700。包含旧 Nginx、systemd 单元及 drop-in、原环境/密钥、切换前状态、服务状态记录、测试日志、验证脚本与 JSON 结果。备份及真实密钥未下载到仓库。

在服务器以 root 执行：

```sh
/root/norma-cloud-backups/20261006T134432Z/rollback.py
```

该脚本停止并禁用新服务，恢复原服务启停状态、设备目录所有权及原 Nginx；保留回退时最新设备状态，不用旧快照覆盖新增记录。原 voice 发布目录仍为 `/opt/norma-voice/releases/20261006-121416-relay`。本次首次切换失败时已实际执行并验证回退成功。

最终 Nginx 主配置 SHA-256：`bd7d9179a8bf942214b13c08072edf5ca637e57caaac352140bc748d527004af`。
