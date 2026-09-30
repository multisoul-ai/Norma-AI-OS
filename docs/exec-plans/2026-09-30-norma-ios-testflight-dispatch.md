# TestFlight：单次扫码注册与 iPhone → Mac 派工验收

## 当前状态（2026-09-30）

- **0.1.0（45）已可安装**。2026-09-30 14:28（北京时间）Apple 回读确认 `VALID / IN_BETA_TESTING`，已关联原「Norma 内测」组。Build/Delivery UUID `950c0463-4f23-4ac9-883c-2ddceed60424`，中文说明已核对。
- 43 的双码流程已被替代。44 的真实页面验收发现“添加电脑”闪退，已在 Apple 标记 `expired=true` 并回读确认，不能用于验收。45 已修复环境注入并通过对应 XCTest UI 回归。
- Mac 专用验收版和常驻本地中继已升级，配对身份与数据保留；原安装 `/Applications/Norma OS.app` 未替换。备份在私有验收目录的 `backups/single-scan-44/`。
- 当前模拟器已恢复常驻服务。用户原 `Test / 2222` 线程中的 `@Alice 123123` 自动送达 Mac，Alice 返回真实澄清回复，错误消失；未替用户验收成果。

## 你如何验收

1. 在 TestFlight 更新到 **0.1.0（45）**。手机和 Mac 连接同一 Wi-Fi，Mac 保持唤醒。
2. 保持 **Norma iPhone Acceptance** 与本地中继运行。Mac 打开 `Codex Task` 项目，在 Settings → Mobile Connection 确认连接开启。
3. 已配对手机：打开 Norma 后自动补注册，原待发送消息继续发送，不需要重新扫码或第二个邀请码。新手机：更多模块 → 已连接电脑 → 添加电脑，在 **Norma 内扫描 Mac 设置页二维码**，在 Mac 确认控制权限后自动完成注册。
4. 在工作空间设置关联 `Codex Task` 与 `Alice`；进入工作线程，通过 `@` 选择 Alice，发送：**创建 iphone-acceptance.txt，写入 hello from iPhone；只修改这个文件，不提交 Git，完成后告诉我。**
5. 检查消息“已送达 Mac”、任务状态、Alice 回复与成果；查看真实文件，再自行选择验收或退回。退出并重开后应回到同一线程，不能重复执行。

本轮测试入口是 **直接 @Agent 派工**。`@Norma` 真实模型编排、实时通话、公网跨网络尚未开放，不应据本次注册通过宣称全部 Goal 完成。

本版本固定访问 `https://192.168.31.165:9444/`，TLS 证书固定校验。Mac/IP 或网络变化后需要重新准备验收环境；没有部署云端，也无需用户输入供应商 Key。

## 常驻环境与模拟器

私有目录：`~/Library/Application Support/Norma Local Acceptance/`。

- `启动本地中继.command`：回环端口 8787、手机 HTTPS/WSS 端口 9444，无临时测试到期计时。
- `打开 Mac 验收版.command`：专用二进制、原数据和原 Codex 配置；重开后在设置确认手机连接已开启。
- `打开 iPhone 模拟器验收版.command`：给当前模拟器绑定常驻服务。模拟器需用此脚本重新启动，**TestFlight 不依赖该脚本或启动环境变量**。后续自动化使用独立模拟器，禁止把用户正在验收的模拟器切换到会到期的临时服务。

本轮用户遇到的 `Could not connect to the server` 根因：验收者留下临时测试端口 49165，该服务 10 分钟到期退出；用户随后在同一模拟器中配对了真实 Mac，但手机仍访问临时网关。已改回常驻 8787 与固定 namespace，没有删除配对或待发送记录。此问题属于本地测试交接，不能通过让用户重复扫码解决。

## 单码注册修复与证据

| 验收点 | 结果 |
|---|---|
| 单码真实协议 | Rust TLS 配对设备请求短期注册证明 → Node 验签和独立设备注册 → HTTPS/WSS 中继；`final-green.log` 1/1 PASS |
| 注册边界 | 错配对 token、撤销、篡改、未知 Mac 证书、过期证明拒绝；相同身份重试幂等、重启恢复；同一集成测试覆盖 |
| 原生身份与持久化 | `final-native-run.log` 1/1 PASS；`native-test.log` 2/2 PASS，真实二维码图像解码、Mac 审批、App 注册、Keychain 恢复、真实中继认证；零邀请 |
| 页面闪退回归 | `ui-guide-red.log` 真实失败；修复 RootView modal 环境注入后 `ui-guide-green.log` 1/1 PASS |
| Computer Use | 实际进入添加电脑、扫码和系统图片选择器；用户配对后，实际打开原 Test/2222 线程，确认原消息送达、真实 Alice 回复、无注册错误。`user-thread-recovered.png` |
| 常驻服务 | 更新后 HTTPS/WSS 与 Mac 在线路由 PASS，`persistent-ready.json`；没有把临时测试服务当成用户环境 |
| 只读 UI 审查 | `ui-review.md`：修复后台状态、共享错误、只读权限及并发注册反馈；最终结论见报告限定范围 |

证据目录：`target/norma-ios-evidence/single-scan-20260930/`。模拟器图片识别不等于物理相机扫码；真机权限、扫码、Apple Speech 与实际手感仍由用户验收。

注册复用 Mac 已有 TLS 身份和服务的可信 Mac 名单。Mac 仅签发初次注册证明，不发语音通行证；手机独立生成长期凭据，收到服务成功回执才保存为已授权。失败仅保留待注册凭据以便幂等重试。只读 Mac 授权仅显示“可查看项目”，不会承诺派工。

45 IPA：88,399,037 字节，SHA256 `fa3b0666ddff362bb5c075b5a00d3ac7e0875286c17f72bfc452a8bfb98452ef`。

45 发布证据目录：`target/norma-ios-testflight/single-scan-45/`。186 项生产输入在归档前后核验哈希；未提交或推送当前工作区。Issue #57/#59 与整体 Goal 仍保留剩余验收项。

## 43 历史验收记录（不再使用旧邀请步骤）

## 本次已验证

| 检查 | 结果与证据 |
|---|---|
| Swift Core | 142 项，8 项按条件跳过，0 失败；`target/norma-ios-evidence/physical-dispatch-20260930/swift-core.log` |
| 真实 Mac/Codex 原生闭环 | 原生页面建空间、绑定项目/Agent、发送任务、真实生成两文件、冻结成果、重启恢复；执行记录仅一行。`real-dispatch.log`，L3 用例 1/1 PASS |
| Computer Use | 实际操作 Mac 打开项目/手机服务；模拟器点击原线程成果、查看文件和返回，确认未自动验收；`cua-result.md`、`cua-real-result.png`、`cua-real-thread.png` |
| 本地 TLS | 错 pin 拒绝、邀请只能兑换一次、真实 HTTPS/WSS、管理端点不暴露；`tls-green.log` |
| 原生打包配置 | App 默认模型读取 Info.plist，真实 HTTPS 邀请兑换、Keychain 恢复及 WSS；`tls-native.log`，1/1 PASS |
| 中继边界 | 认证、隔离、撤销、断开、重启检查通过；`relay-regression.log` |
| 常驻验收环境 | 私网 HTTPS/WSS 与配套 Mac 实际在线路由通过；`persistent-ready.json` |
| 发布输入与签名 | 185 项生产输入归档前后哈希一致，`aps-environment=production`，禁止调试，分发签名通过 |

本轮未把模拟器测试算作物理手机扫码/麦克风/Apple Speech 验收。**直接 @Agent 是此次入口**；`@Norma` 真实模型编排、实时语音、跨公网、完整断网/后台与后续修改闭环仍按既有 Goal 跟进，不能视为全部完成。

## 实现与发布记录

- iOS 复用现有证书固定校验，同时用于邀请 HTTPS 和中继 WSS；端点/证书/隔离 namespace 只可由构建配置提供，邀请不能改变端点。默认配置为空，普通正式构建仍走原服务。
- Mac 继续使用现有中继注册、端到端加密和项目授权。Node 本地 TLS 入口复用正式网关；管理员接口与 Mac 注册仅监听回环，手机端不含管理员或供应商 Key。
- 配套 Mac 复用本轮真实派工通过的同一二进制，独立副本和数据目录；本地 ad hoc 签名，不作为正式 DMG 发布。
- IPA：`target/norma-ios-testflight/local-dispatch/export/Norma.ipa`，88,365,682 字节，SHA256 `99256e2fbbdee810609daad9fb4db8e2cbb845bfce7b928b251052c16aaabb6d`。
- 发布证据：`target/norma-ios-testflight/local-dispatch/`。真实 L3 全量证据：`/private/var/folders/kr/106cttn15rv7xlgv2y_7v4wr0000gn/T/norma-thread-real-D9JclA/`。
- 发布期间 Node 直连 Apple API 两次 TLS 连接重置；使用本机已配置的网络代理后恢复。关联组后第一次回读误用了 Apple 不支持的 `builds/{id}/betaGroups` GET 路由，改为读取现有组的 builds 列表并核对 Build ID；最终可安装状态、组成员关系、中文说明与未过期状态全部通过。未重复上传 IPA。
- 发布来自当前未提交工作区；未执行 commit/push，未部署云端。Issue #57/#59 在剩余验收满足前保持 OPEN，整体 Goal 保持 active。
