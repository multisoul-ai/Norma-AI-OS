# 独立 Vercel 语音服务与 iOS 免配置

2026-09-14：功能已实现，独立生产网关已部署。用户授权新建并部署服务，同时要求iOS无需配置任何Key；未改companion或旧线上项目。

- 稳定地址：`https://norma-voice.vercel.app`；项目`norma-voice`，团队`multisoul-ai-s-projects`。
- 最终部署：`https://norma-voice-3berppbf2-multisoul-ai-s-projects.vercel.app`。仅上传`services/voice`运行文件，排除环境文件、模型、客户端、测试与验收数据。
- Node.js 22、Fluid Functions、东京`hnd1`、300秒上限。新加坡/香港节点曾连接Ark接口超时；东京并保留IPv4优先，已得到真实Chat/TTS HTTP 200；网络抖动仍可能触发重连，未把单次成功当作长时稳定性验收。
- 手机和Mac需要更新到包含本次变更的版本；本轮没有替换用户正在运行的Mac App或发布新的TestFlight构建。

## 使用与授权

Mac开启Mobile Connection，手机配对并取得项目控制权限，选择项目后开聊。默认豆包服务自动连接，不填URL、Key或访问口令；旧默认地址自动迁移，已有自定义开发地址保留。

手机通过已配对的pin TLS通道领取固定用途授权。Mac使用已有P-256安装身份签名，网关仅信任登记的公开证书，不接收Mac私钥或配对token。当前登记此用户正式Mac安装；新增/重装Mac由管理员登记公开证书，手机仍免Key。

授权有效24小时，手机钥匙串保存；最后一小时尝试刷新，Mac离线时未过期授权仍可聊天。过期需要连接Mac更新。只读/错误凭据/已撤销设备不能领取新授权；已签发授权最长到原24小时期限。任务执行继续实时核对项目权限。

## 云端边界

复用现有Redis集成，使用独立`norma-voice:<管理口令SHA256前24位>:`命名空间。一台私有部署同时一通付费电话；跨实例一次性票据、45秒独占租约、15秒续期及设备隔离的已播历史。240秒安静时轮换，280秒强制结束，避开300秒平台上限。

媒体与Mac工具WebSocket可位于不同实例；只有有界控制消息经过Redis（每队列最多16条/1MiB，单条512KiB），不自动重放写操作，音频不进入Redis。合法50帧/秒输入可吸收最多3秒网络积压；超过150帧同刻突发仍被拒绝，保留持续速率和上游背压限制。超限以可重试的`audio_rate_limit`结束当前通话，由iOS既有重连流程恢复，不误报为需要重新配置凭据。

七项环境变量已配置。供应商Key来自可用的本机服务配置；不能拿旧导出文件中的替代值部署。生产证书列表经Vercel解密读取核对与预期完全一致。仅保留不包含Key、正文或音频的供应商状态码诊断。

## 本轮断流修复

同一东京节点、同一合成输入，云端`fetch`两次连续出现`UND_ERR_CONNECT_TIMEOUT`（约10秒），本地网关正常。只替换供应商传输为Node原生HTTPS后，诊断部署连续三次完成ASR、Chat、两轮PCM及关闭确认（23.1/21.4/21.0秒）。最终实现复用该原生HTTPS适配；真实本机TLS回归验证流式首包、完整正文、取消响应体和不跟随307跳转，未新增依赖。

另一个确定的关闭竞态已先红后绿：上游已确认关闭，但1秒强制计时器会抢在Redis释放前关闭手机连接。测试以1200ms释放延迟复现；现于上游关闭时取消该计时器，再完成有超时上限的云端清理并送达`session.closed`。

网络大幅积压时仍会断开，不扩大150帧安全上限。新增回归要求第151帧不上传且错误标为可重试。真实Redis工具测试将配置握手放在票据重放验证之前，并按跨实例网络预算等待，保留生产5秒握手限制。修复后Node三组及真实Redis跨实例测试通过；临时代理探针、机器绝对依赖路径和DEBUG日志已删除。

## 验证证据

| 验收 | 结果 |
|---|---|
| 生产HTTP/WSS/豆包 | 最终`3berppbf2`部署由Vercel云端发起三次完整通话，16.659/15.733/16.934秒，均含匿名401、真实ASR/Chat/PCM、工具通道、独立TTS与`session.closed`；见[最终线上记录](../design-docs/assets/norma-realtime/vercel-voice-v52.json)。[v50](../design-docs/assets/norma-realtime/vercel-voice-v50.json)保留为历史记录 |
| Mac签名→Node换票 | 实际TLS与Node跨语言验签；控制设备成功，错误/只读/撤销、篡改/到期/错audience/未登记Mac拒绝 |
| iOS免Key | v53原生iOS实际从Mac自动领取签名，再连接最终生产网关：HTTP200、豆包WSS就绪及关闭确认通过；未登记身份对照返回401。见[原生生产证据](../design-docs/assets/norma-realtime/native-voice-authorization-v53.json)与[执行日志](../design-docs/assets/norma-realtime/native-voice-authorization-v53.txt)。UI自动配置/无Key输入的独立测试保持通过 |
| 原生回归 | 豆包音频、入口/资源、自动授权3个L3 runner通过；Swift79/0 |
| Node/Redis | 三组HTTP/WS回归通过；真实Redis双实例票据、历史隔离、轮换与工具回执/播报通过；网络抖动先红后绿，过量突发仍拒绝 |
| Rust/构建 | `cargo test --workspace -- --test-threads=1`：1851通过/0失败/104忽略；App check、Release及边界/行数/no-allow/diff检查通过 |
| Computer Use | v53重试确认Xcode可操作，但Simulator绑定持续`timeoutReached`；经原生测试重新启动及Xcode菜单打开仍未恢复。实际模拟器此前已观察并保存[自动配置截图](../design-docs/assets/norma-realtime/cua-voice-automatic-v50.png)；点击被工具`noWindowsAvailable`/`timeoutReached`阻断，未标为完成，未将XCTest点击冒充Computer Use |

本机直接访问最终部署仍出现TLS连接失败和超过3秒的音频积压。最终云端对照在独立、带管理鉴权、未绑定正式域名的临时部署发起，正式域名始终指向`3berppbf2`；三次完整通过，确认服务端链路可用，但不代表本机网络问题已消失或已完成长时稳定性验收。第三次云端调用曾在本机TLS阶段失败、未到达验证函数，重发后才形成第三份结果。临时验证部署和源码已删除，CLI临时生成的部署保护凭据也已撤销。

部署探针含两轮TTS及工具绑定，截止时间按连接12秒、输入4秒、三次服务请求各20秒、关闭4秒与网络余量10秒设为90秒；要求实际`session.closed`，不能只收到音频就通过。原40秒聊天探针曾在两轮音频已收到后等关闭ACK时超时，已单独记录。

v52云端完整语音探针使用管理凭据；v53另外以原生iOS自动领取的Mac签名完成同一正式网关换票与豆包连接，全程未向iOS提供服务端Key。此处使用隔离模拟器和独立项目/权限库，Mac测试服务仅在明确指定账户与公开指纹时加载已登记Keychain身份，不导出私钥、不改正式配对；测试后清理模拟器临时配对。它们仍不是同一次物理iPhone云端音频通话。合成输入不含现场麦克风音频，PCM接收不等同扬声器实测。GPT云端、物理耳机/锁屏及完整Computer Use点击仍保持未验收。

复跑入口：`npm test`、带真实Redis环境的`npm run test:cloud`；v52只调整Node网关与测试，没有改Rust/Swift运行代码，因此沿用前述已通过的全仓、Swift和原生构建结果。原生runner见`crates/e2e/tests/ios_realtime_companion.rs`，线上合成音频探针配置见`services/voice/README.md`。本轮未commit或push。

v53仅扩展验收测试，生产仍为`3berppbf2`，未重新部署或改客户端运行代码。原生生产授权测试已通过，runner要求实际成功标记，跳过不能算通过；`cargo test -p e2e -- --test-threads=1`为270通过/0失败/75忽略，格式、行数、no-allow和diff检查通过。唯一尚未完成的本次强制检查是Simulator的Computer Use点击验收。

阻塞审计（2026-09-14，连续第三轮）：再次调用`cua.getApp("com.apple.iphonesimulator")`，约5秒后返回`timeoutReached`。此前已在原生测试重启后重试，并通过可操作的Xcode开发工具菜单打开Simulator，仍未恢复。当前没有残留Xcode测试进程；v52/v53证据中的源码哈希与工作树完全一致。生产语音、原生免Key授权及E2E回归均已验证，剩余强制Computer Use点击验收需工具/环境恢复；Goal已标记`blocked`，未标记完成。
