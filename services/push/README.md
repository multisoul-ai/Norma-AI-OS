# Norma 推送服务

## 工作空间实时活动（2026-10-05，待真机验收后发布）

源码已新增 `activity.mjs`，由 `relay.mjs` 导入；部署时须同时复制两个文件。现有线上服务尚未在本轮发布这条通道。实现与真机发布门槛见 `docs/exec-plans/2026-10-05-ios-workspace-live-activity.md`。

`/v1/notifications` 复用原授权与 APNs 连接，新增 `kind: activity` 的 `start/update/end`：专用 topic 为 `com.norma-ai.os.ios.push-type.liveactivity`，类型为 `liveactivity`。start 使用 `WorkspaceActivityAttributes` 并带一次 alert；普通 update 使用 priority 5 且无 alert/sound，关键更新使用 priority 10；end 的 dismissal-date 为 timestamp + 600 秒。stale-date 与有效期为 timestamp + 120 秒，正文限制 4 KiB，不接受控制指令。

普通工作空间事项新增 `review`、`desktop_workspace_id` 与 `area_id`，三者必须与复合 workspace scope 一致；点击仍只回源查看。Mac 用同一持久事项账本为新旧通知入口去重，不为普通进度或无冻结成果的完成卡片发提醒。实时活动开关和普通通知开关互相独立。

```sh
node --test services/push/activity.test.mjs services/push/relay.test.mjs
```

状态（2026-09-14）：已按用户授权将支持 `preview` 的 relay 直接部署到 `47.251.242.24`，公网 URL 与 Mac 凭据保持不变，Nginx `/norma/` 改为转发至本机 `127.0.0.1:8788`。真实已登记的 production iPhone 诊断请求携带项目、卡片、摘要，经公网返回 `200 {"status":"accepted"}`；Apple 已接受，尚未观察到手机展示或点击结果。

根因：安装中的 Mac 已发送 `preview`，旧线上 relay 严格拒绝该字段并返回 `400 invalid_event`。切换前通过 47 的 SSH 2222 端口登录，线上 `nginx -T` 确认其 `/norma/` 原来转发至 `118.31.118.73`，47 尚无本机 relay；本轮未登录或修改 118。Mac 已补充固定错误码分类，协议拒绝和 APNs 配置错误不会再统称 `delivery_unknown`，仍不重发未知请求。

使用 Node.js 自带的 HTTP/2 和加密模块，无 npm 依赖；当前部署复用 Node 20.18.1，并已在该版本通过 relay 全部 3 项测试。只处理待确认、完成、失败三类事件，接收项目名称、卡片名称和该回合最终输出的有界摘要，不接收完整聊天、工具参数、项目路径或控制指令。预览只进入 APNs alert，目标身份独立保留并在手机打开后回到 Mac 核验。

## 配置与运行

在 Apple Developer 为 `com.norma-ai.os.ios` 开启 Push Notifications，创建对应环境的 APNs 签名密钥，下载 `.p8` 并保存 Key ID、Team ID。开发与 TestFlight/App Store 的环境不同：开发用 `sandbox`，TestFlight/App Store 用 `production`；应分别配置服务和密钥。密钥仅存放服务端，不能加入 iOS、Mac 安装包或仓库。

部署账户准备两个仅自己可读的文件：Apple `.p8` 和包含 64 位小写十六进制随机字符串的 relay key。可用以下命令创建后者（不会输出密钥）：

```sh
umask 077
mkdir -p ~/.config/norma-push
node --input-type=module -e 'import {randomBytes} from "node:crypto"; import {writeFileSync} from "node:fs"; import {homedir} from "node:os"; writeFileSync(homedir()+"/.config/norma-push/relay.key", randomBytes(32).toString("hex"), {mode:0o600, flag:"wx"});'
```

运行时配置以下环境变量，不要把密钥内容作为命令行参数：

| 变量 | 内容 |
|---|---|
| `APNS_KEY_PATH` | `.p8` 的绝对路径，权限 `600` |
| `APNS_TEAM_ID` | 10 位 Team ID |
| `APNS_KEY_ID` | 10 位 Key ID |
| `APNS_ENVIRONMENT` | 显式指定 `sandbox` 或 `production` |
| `NORMA_PUSH_RELAY_KEY_FILE` | relay key 文件的绝对路径，权限 `600` |
| `NORMA_PUSH_PORT` | 可选，默认 `8788` |

```sh
node services/push/relay.mjs
node --test services/push/relay.test.mjs
```

服务固定监听 `127.0.0.1`。部署时用同机 HTTPS 反向代理暴露 `/v1/notifications`，Mac 只使用验证证书的 HTTPS。HTTP 监听口不得公开；反向代理不得记录 Authorization、请求体或 APNs token。当前是单用户/可信团队的私有服务，relay key 由部署者配置给可信 Mac，不是公共多租户注册系统。

## 复用主域名部署

外部入口为 `https://soulvoiceai.com/norma/v1/notifications`；健康检查为 `/norma/healthz`。Nginx 去掉 `/norma/` 前缀，内部仍使用 `/v1/notifications` 和 `/healthz`，无需修改 DNS。新版 Mac 配置校验只额外放行这个明确路径，保留 HTTPS、无查询/密码和禁止重定向的约束；已有 `/v1/notifications` 配置仍有效。

部署文件：同机反代 [`deploy/nginx.conf`](deploy/nginx.conf)、现网入口副本 [`deploy/nginx-front.conf`](deploy/nginx-front.conf)、[`deploy/norma-push.service`](deploy/norma-push.service)、[`deploy/relay.env.example`](deploy/relay.env.example)。两个 Nginx 片段内容相同，选择一个放入现有 HTTPS `server` 块，不要同时 include；主站自定义错误页不能吞掉 API 网关失败，缺少后端时返回 `503 {"error":"push_unavailable"}`。不会把网关的未知投递结果自动重试。

2026-09-14 实际部署位置：

- 主机：`47.251.242.24:2222`（SSH），主机名 `iZrj99wrem2lvepmol23n1Z`。现有 `/etc/nginx/nginx.conf` 引入 `/etc/nginx/norma-push-front.conf`，`/norma/` 去掉前缀后转发至本机 relay；未更改 DNS。
- 文件：`/opt/norma-push/relay.mjs`；复用 `/usr/local/bin/node`，Node `20.18.1`。relay SHA-256：`010faed23d882afb23c3993d0e2e56f176f1e86e8efb75484f94346740e6ef4a`。
- 服务账号：`norma-push`；systemd 单元为 enabled、active/running，监听 `127.0.0.1:8788`；凭据条件不满足时不启动。
- 共享服务器资源上限：CPU 为单核的 20%、内存 128 MiB、进程/线程合计 32，包含在 `/etc/systemd/system/norma-push.service` 中。
- 专用访问凭据位于 `/etc/norma-push/relay.key`；Apple 密钥位于 `/etc/norma-push/apns.p8`，均为服务账号所有、0600。正式环境文件 `/etc/norma-push/relay.env` 为 root 所有、0600。
- APNs 密钥：`Norma Production Push`，Key ID `2TR75XXS26`，Team ID `2BF8G9AN4L`；限定 production 环境及唯一 topic `com.norma-ai.os.ios`。未更改已有 Expo 密钥，也未使用 App Store Connect 上传密钥代替推送密钥。
- 本次 Nginx 片段备份：`/etc/nginx/norma-push-front.conf.before-local-20260914-143841`。原主配置备份：`/etc/nginx/nginx.conf.norma-backup-20260913-131204`。

本轮 `nginx -t`、reload、服务 active/running 及本机/公网 health 检查通过。公网 TLS 校验通过且实际 remote IP 为 `47.251.242.24`；无凭据返回 401、正确凭据加非法事件返回 400。带 preview 的真实 iPhone 诊断事件 `cc167f44-112b-4ae9-9a9c-4c0b85f49919` 返回 200 accepted；使用当前卡片 #507 的定位身份与新事件 ID，没有重放历史任务或重复投递。

切换前后网站首页 SHA-256 均为 `201cd1bf3d32e7f5fd8aa22c6de5370852fe03fb654ee22d345dd82f16a246a0`，Nginx 主 PID 未变。这里只验证配置、服务和 HTTP 可用性，未逐项执行网站登录、支付或写入流程。

回滚入口时恢复上述 `before-local` 片段，执行 `nginx -t` 成功后 reload。备份指向旧的 118 服务，会重新出现拒绝 preview 的问题；本轮未验证 118 当前进程状态。

如需重建部署，将 `.p8` 保存为 `/etc/norma-push/apns.p8`（服务账号所有、0600），复制环境示例为 `/etc/norma-push/relay.env` 并填写真实 Key ID；沿用 Team ID 前须核对密钥归属。TestFlight 使用 production。随后执行：

```sh
systemctl start norma-push
systemctl status norma-push --no-pager
curl --fail-with-body https://soulvoiceai.com/norma/healthz
```

当前已授权 iPhone 已登记 production token，且 Apple 已接受诊断通知；新任务自动触发、手机实际展示及锁屏点击回源仍需真机验收。

历史桌面包（2026-09-13）：`build/Norma OS_0.1.71_aarch64.dmg`，192860249 bytes，SHA-256 `a5847c0cea560e3a09f784ad53d7a0665f12875554934e1e2086e4a644ff85d4`，Developer ID 签名、未公证。2026-09-14 本次部署未替换或重启桌面 App，未上传 iOS/TestFlight。

## 协议

`POST /v1/notifications`，`Content-Type: application/json`，`Authorization: Bearer <relay key>`。每次仅一个设备、一个事件；请求最多 4096 字节。以下为结构示例，示例 ID/token 不可发送到真实 APNs：

```json
{
  "event_id": "00000000-0000-4000-8000-000000000001",
  "token": "设备从APNs获得的十六进制token",
  "expires_at": 1800000300,
  "kind": "approval",
  "machine_id": "mac-加64位证书指纹",
  "workspace_id": "项目身份",
  "route_id": "项目窗口身份",
  "node_id": "7",
  "run_id": "运行身份",
  "session_id": "Provider会话身份",
  "turn_id": "回合身份",
  "request_id": "仅approval事件具有此字段"
}
```

ID 必须是协议的 ASCII 标识，`node_id` 使用十进制字符串，保留完整 u64 范围。`kind` 为 `approval`、`completed` 或 `failed`；后两者不得带 `request_id`。`expires_at` 是 Unix 秒，必须在现在之后且最多 300 秒。有效期用于拒绝旧事件及设置 APNs expiration；手机仍必须回源核验。

| HTTP / body | 含义 |
|---|---|
| `200 {"status":"accepted"}` | APNs 已接受，不能宣称手机已收到 |
| `401 unauthorized` | relay key 错误 |
| `400 invalid_event` / `413 body_too_large` | 请求未发送 |
| `409 event_conflict` | 同设备、同事件 ID 对应不同内容，未再次发送 |
| `410 token_unregistered` + `invalidated_at` | APNs 返回失效时间（毫秒）；Mac 必须与该 token 最近注册时间比较，不能撤销较新的注册 |
| `422 token_invalid` | token 与服务环境/topic 不匹配；检查配置，不要撤销其他环境的注册 |
| `502 delivery_unknown` | 可能已送达 APNs，不自动重发 |
| `502 apns_configuration_error` | APNs 密钥/账号配置失败，不能误删设备授权 |
| `503 relay_busy` / `apns_unavailable` | 当前容量或 APNs 暂不可用 |

`GET /healthz` 仅证明服务已启动，不能证明 APNs 凭据已获 Apple 接受。

## 已验证边界

本地真实 HTTP 与 HTTP/2 测试覆盖入口认证、大小和有效期、未知字段拒绝、重复/冲突、ES256 签名、正确 topic/环境、JWT 复用、失效 token 和连接断开不重发。这些自动化测试不向 Apple 投递；真实 iPhone 诊断请求的 Apple accepted 证据见部署记录。

JWT 每 50 分钟更新；HTTP/2 连接复用，最多一个 APNs 流，入口最多 8 个在途请求。相同设备/事件在有效期内共用一次结果，包含未知结果；内存最多保留 4096 个未过期条目。重启不保留去重记录，不能在当前实现上部署多个副本或依赖自动重试。需要这些能力时再接持久化账本。

## 接入与真机验收顺序

1. iOS 通过用户明确开启通知取得 APNs token，用现有已认证、证书 pin 的通道注册给各 Mac；只读项目也可订阅提醒。
2. Mac 保存每个手机的 token/环境/注册时间；关闭提醒、忘记或撤销授权时停止投递。后台只从真实待审批和准确回合终态生成事件，并核验当前项目权限。
3. Mac 使用 relay key 投递；不把 `.p8` 下发给 Mac。手机点击通知后验证已配对电脑、项目和运行身份，打开对应页面；通知本身不执行批准。
4. 配置开发者 Team 与推送 entitlement 后，用真实 iPhone 验证锁屏、延迟、撤销和网络切换。

## 已接入的电脑订阅协议（v43）

两种 action 均使用现有 WSS 信封和独立手机凭据，仅可管理当前已认证手机：

- `push_subscription`：读取 `subscription { machine_id, device_id, revision, registration, registered_at_ms }`。没有订阅时为版本 `0` 和两个 `null`，配对不会自动订阅。
- `set_push_subscription`：提供 `expected_revision` 和显式 `registration`。启用时后者为 `{ token, environment }`，关闭时必须为 `null`，不能省略字段。

版本与当前值不一致返回 `push_subscription_changed`，手机先重新读取再确认，不自动重放旧更新。成功响应必须是预期版本加一，并且与所提交注册完全一致；响应丢失后只查询状态。

只读手机也可订阅，但项目权限不会扩大。Mac 在事务中复验设备授权；撤销手机时，在同一事务中清除 push token。APNs 失效只允许关闭投递时对应的订阅版本，并比较服务端记录的注册时间；迟到的失效不能关闭新 token。关闭提醒保留设备授权；失效或关闭后保留递增版本，以拒绝旧的启用请求。

依据（2026-09-08 核对）：[Apple token 鉴权](https://developer.apple.com/documentation/usernotifications/establishing-a-token-based-connection-to-apns)、[发送请求](https://developer.apple.com/documentation/usernotifications/sending-notification-requests-to-apns)、[响应处理](https://developer.apple.com/documentation/usernotifications/handling-notification-responses-from-apns)。


## Mac与iOS配置（v45）

Mac 在「手机连接 → 任务提醒 → 配置服务」填写完整 HTTPS `/v1/notifications` 或 `/norma/v1/notifications` 地址、64位小写 hex 的 relay 访问凭据及应用环境。开发调试选 Sandbox，TestFlight/App Store 选 production；服务端环境须一致。每次保存都需重新填写凭据，输入始终遮蔽，提交后清空。地址、环境和凭据保存在本机 AppDB；Apple `.p8` 只放服务器。

保存、读取及应用配置全部在后台。运行中更换地址或凭据只切换通知发送端，保留手机连接、项目授权和 Agent；替换会等待已发出的请求结束，处理期间有禁用反馈。旧配置读取中的事件会被代次检查丢弃，新终态游标从当前水位开始；同一个服务生命周期中已尝试发送的审批不会因切换配置重发。关窗后的迟到配置不能重启服务。

「清除配置并停用」删除已保存凭据，保留明确停用标记，防止环境变量恢复通知；已交给 Apple 的提醒不能撤回。保存配置不发送测试通知，也不代表服务可达或 iPhone 已展示通知。未首次配置时仍可用启动环境 `NORMA_PUSH_RELAY_URL`、`NORMA_PUSH_RELAY_KEY_FILE`（仅自己可读的64位hex凭据文件）、`NORMA_PUSH_ENVIRONMENT`（sandbox/production）；界面配置或停用标记优先。错误通知配置不会阻止开启手机连接，通知设置内回读会显示错误。

iOS的「电脑 → 任务提醒」先由用户允许系统通知，再按电脑单独开启。浏览页面或配对不会自动申请通知权限。系统token留在内存，保存的本地偏好只有启用的电脑ID；已启用电脑可在新token回调/回前台时续登记，Mac已关闭的订阅不会被自动重新开启。通知关闭遇到离线或未知结果，保留错误并要求回查；忘记电脑前先关闭提醒，已无可用授权的电脑可以直接移除本地记录。

`ios/App/Norma.entitlements` 和 `ios/App/Info.plist` 使用同一个 `APS_ENVIRONMENT`：Debug为development→sandbox，Release为production。使用开发者账号为 `com.norma-ai.os.ios` 开启Push Notifications并选择对应Team/签名；TestFlight/App Store应使用Release归档。更改签名环境时同时使用一致的APS_ENVIRONMENT与relay环境。未签名的iphoneos构建只证明能编译，不能安装到物理设备或证明Apple接受推送。

Mac通知线程随手机连接服务启动、关窗停止。复用原有有界快照/审批入口，每秒查询已订阅项目；终态从AppDB已提交事件分页读取（64条/项目/轮），不从卡片done推断。每次服务/项目订阅开始先建立历史游标，不补发旧完成任务；待审批只提示Provider实际提供可选项的请求。发送前再次检查窗口、运行、请求、设备授权与准确订阅版本。请求ID保留现有256字符上限，其他定位字段仍为128字符；预览包含项目名（≤160 UTF-8字节）、卡片名与编号（≤240字节）、最终输出摘要（≤640字节）；无摘要时使用明确的查看输出提示，不借用下一回合内容。完成/失败点击直接进入同一项目、运行和会话的卡片对话，活动回合清空或推进不使它失效；审批仍核验准确回合和请求。

通知为有界的尽力投递：事件最多存活五分钟；一次投递最多等待12秒，不重试不确定结果。活动审批最多保留4096个手机/请求去重项，确认消失后清理。完成事件由游标消费；应用退出或不可用时不建立离线重试队列。撤销可阻止后续投递，已经发送给APNs的请求无法收回。APNs 410只清除对应订阅版本且注册时间不晚于失效时间的token；保留电脑授权。HTTP 200仍只表示APNs接受。

手机点击通知时只从已保存的配对记录找电脑；通知不能提供新地址或控制动作。冷启动等待本机电脑列表加载后再回源。窗口、运行、会话、回合或具体审批已变化时显示无法打开，不把旧提醒交给新任务；匹配后进入真实对话或定位指挥台卡片，批准仍需用户再次操作。

本轮证据：本地WSS owner + 替代前台源 + 真实SQLite/HTTP验证审批、连续两回合终态、跨项目/环境隔离、读取订阅后撤销、丢回执不重发与410清理；Node HTTP/2验证签名和256字符请求ID。原生SwiftUI导航和注入的系统token回调测试通过，Computer Use实际打开提醒页面。这里没有声称已经取得系统APNs token、完成通知中心点击回源或真实锁屏验收。

签名账号已按桌面Packager.toml沿用公司Team `2BF8G9AN4L`。原通配描述文件缺少推送能力；Xcode自动更新已成功生成专用描述文件与已签名Debug构建，签名校验及实际aps-environment=development检查通过。没有进行App Store/TestFlight上传。当前登记的物理iPhone 11不可用，锁屏验收仍待连接设备和APNs服务配置。


## 原生验收（v46）

独立模拟器已通过系统授权取得真实APNs token，并从原生通知设置完成Mac登记/关闭。冷启动点击用 `simctl push` 本地注入验证，未经过relay/Apple投递；它曾发现UIKit主线程崩溃，现已通过显式主线程completion修复。该轮验收时服务尚未部署，不能将token就绪或本地通知可点击等同于物理iPhone锁屏成功。


## 正式 SQLite 跨进程验证

独立进程测试验证推送配置保存、重读、更换环境、停用、损坏拒绝和安装隔离；使用临时 AppDB 与合成凭据，不发送 APNs 请求。Runner 会拒绝测试存储分支；须分别构建/运行，避免 Cargo 特性统一引入 `e2e-test-support`。

```sh
cargo build -p app --features l3-test-support --bin norma-push-sqlite-l3
NORMA_PUSH_SQLITE_PROBE="$PWD/target/debug/norma-push-sqlite-l3" cargo test -p e2e --test mobile_push_sqlite -- --ignored --nocapture
```

此证据覆盖正式库的跨进程持久化，不包括正式App窗口重启或升级后的签名ACL兼容。该轮验收时尚缺服务部署和APNs密钥；当前部署状态见文首，仍需可用物理iPhone完成真实锁屏投递。


v49日志保全：用户删除/tmp后旧临时日志无法恢复；目录现已恢复，正式Keychain测试复验通过（1.16秒），新日志为target/norma-ios-evidence/v49/keychain-native.log。此结果仍不等于APNs投递或物理锁屏验收。
