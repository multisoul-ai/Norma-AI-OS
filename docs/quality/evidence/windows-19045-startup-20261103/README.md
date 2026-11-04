# Windows 10 19045 启动兼容修复证据

用户系统：Windows 10 专业版 22H2，19045.6093。原报错发生在进程加载阶段；本报告不得将 Server runner 结果标为 Windows 10 / 11 真机通过。

## 已确认的根因与 RED

- 旧包源码 `94e4bc83a7940a39ef0888d97d5f90669c5ccacd`，内层 ZIP SHA256 `87ee66f8985766bbc611e5d4530e9edd3533fa27f26777037ffd3fcdcde9eba7`。
- `norma-os.exe` 强制导入 `ws2_32.dll!ProcessSocketNotifications`，另三个发布 exe 没有该入口。此 API 最低系统 Build 20348，高于用户 19045；[微软文档](https://learn.microsoft.com/en-us/windows/win32/api/winsock2/nf-winsock2-processsocketnotifications)。
- [真实旧包 RED run 41669112175](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41669112175)，job `126032380263`，attempt 1，harness `89dca382fe8f4973f5f315f53c056ae2248f9ef8`。新 runner 下载旧同一包，真实 dumpbin `/imports` 检查四个 exe，因强制导入失败；不是编译/环境失败。
- RED artifact `13247980606`，SHA256 `67274862f1ef1e48dc457befa0a2966bed87bc4c014140ab7ffed5ff1c2e9915`，已下载校验，完整日志和解压证据保存在本目录。

## 修复与验证映射

源码 `676b7330ac9573518fcd06903bee9da3d31077d3`：从已加载且常驻的 WinSock DLL 动态解析可选入口；缺 API 时不创建 IOCP 和额外 socket，保留 5ms WSAPoll、250ms Ping 与 pending answer 的原写入归属。

| 要求 | 对应验证 | 当前结果 |
|---|---|---|
| 新包移除强制入口 | producer 与独立 consumer 对包内四个 exe 执行 dumpbin 导入表检查 | PASS，两次检查均无强制入口 |
| 反向探测不冲出 pending answer | 8KiB 帧实际部分写出 4KiB，恢复底层可写后强制超过 250ms 探测门槛，观察真实接收队列不增长 | PASS，两个 Server 各三轮重复 |
| 部分写入、取消、关闭保留发送归属 | 原 partial fixture 的 drop / tui-close / complete，自动和缺 API 两种工厂，原断言不变 | PASS，两个 Server 各三轮重复 |
| 真正输出背压时关闭收口 | 连续 8 次真实 WouldBlock、全部源句柄释放、原 3 秒预算，自动与缺 API 工厂 | PASS，两个 Server 各三轮重复 |
| 四方向关闭与双向慢端完整传输 | 原共享 fixtures，24MiB 数据、原预算和 Ping 断言，额外强制缺 API 执行 | PASS，两个 Server 各三轮重复 |
| 受影响 norma 回归 | macOS 全 crate；Server 2022 / 2025 的 lib/bins、全部 Codex targets 和平台契约；聚焦三轮、Clippy/fmt | PASS，Windows [run 41675679325](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41675679325) |
| 同包真实 App 流程 | 新 runner 下载同一 ZIP/hash，不重编译；中文创建、精确查询、退出重开保持 ID | PASS，[41686500929](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41686500929)，原图已核对 |
| 用户 Windows 10 实机 | 19045.6093 启动复测 | USER_VALIDATION_PENDING |

自动模式的测试日志报告真实 `notifications` 与 `registered` 状态，以便确认 Server 确实运行了新 API 路径。诊断专用 `socket_close_platform` 测试仍有静态新 API 导入，仅在支持它的 Server runner 执行，不进入发布包。

首轮 [41673358826](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41673358826) 保留不重跑覆盖：两个 Windows job 均在编译未改动的 `crates/norma/tests/orchestration_installer_security_regression_tests.rs` 时失败，因无条件导入 Unix `PermissionsExt` 并调用 `from_mode` / `mode`。该文件在 RED 基准与当前源码中的 Git blob 均为 `16da3c5e2155278407163cbc821d5205467cd3c6`。本次不为制造全绿增加 cfg 或 ignore；Windows 全 crate 状态仍为 COMPILE_BLOCKED，后续明确选择受影响测试。

同轮 macOS job 的完整 crate 测试为 351 通过、0 失败、1 既有 ignored（真实 Grok 安装发现，需要用户已安装 CLI，本次未修改），随后 relay 三轮及 Clippy/fmt 均通过。生产与测试源码在后续 workflow-only 提交未变，复用该证据。

第二轮 [41674619094](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41674619094) 两个 Server 各 81 通过、0 失败、0 忽略，聚焦各三轮均 8 通过。随后 Clippy 发现旧诊断 helper 的 `OVERLAPPED` default 后赋值。提交 `15338abf` 仅按原值改为结构体初始化，不改变逻辑或压制诊断；该失败也完整保留。

最终测试 [41675679325](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41675679325)，固定源码 `15338abf2ad49406b1447bda0da648e4e3052010`：Server 2022 / 2025 各 81 通过、0 失败、0 忽略，聚焦各三轮均 8 通过，Clippy/fmt 均通过。日志确认自动 API 路径 `notifications=true, registered=[true,true]`，缺 API 单测确实执行。结果摘要与环境见 `test-summary.json`。

## 修复测试包与同包验收

- release [41676826809](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41676826809)，attempt 1，源码 `15338abf2ad49406b1447bda0da648e4e3052010`；build job `126059602099` 和新 runner verify-candidate job `126092814171` 均 SUCCESS。
- [下载未签名 x64 便携 ZIP](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41676826809/artifacts/13255067769)，artifact `13255067769`，到期 `2026-11-04 17:37:16 UTC`。文件 `norma-os-0.1.77-windows-x64-15338abf.zip`，95,185,444 bytes；SHA256 `e590276bdb5ee5bf2b4a939257600180d79a2098bcfc35a50b1c5652a783e6d1`。本地副本位于 `/private/tmp/norma-windows-startup-winsock-20261103/build/windows/norma-os-0.1.77-windows-x64-15338abf.zip`。
- 外层 artifact 95,101,091 bytes，服务端 SHA256 `ad8caaec12680ee65e897465f524894443853ba4edbb23ef4a04c2a879044bdf`。已下载并先核对外层 digest，再核对内层 ZIP 与随包 sha256，manifest 的源码提交也一致。
- 生产者包内四 exe 的真实 PE 导入检查 PASS；之后独立新 Server 2025 runner [41686500929](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41686500929)，job `126093545507`，attempt 1，harness 同为 `15338abf`，下载并解压同一 ZIP，没有重编译（`rebuilt=false`），重复导入检查和包内 helper / 真实 App 启动检查均 PASS。
- 真实键盘输入名称 `Runner 中文 workspace 41686500929` 后创建成功，公共 CLI 精确查询只返回该完整名称；正常关闭重开后，同一 `230be8db-bfd6-4b7a-b02a-9f353217b92f` ID 保留，完整名称不变。两次查询 JSON、App stdout/stderr 和持久化日志完整保留。
- 原图核对 PASS：名称框完整显示中文；创建后画布顶部显示完整名称；重开后首页显示 1 个工作区和保留中文前缀的最近访问卡片，无错误弹窗。卡片截断处不推断完整文字，完整身份由 CLI JSON 确认。自动报告原始 `visual_verification=PENDING` 保持不变；已完成的原图审阅单独记录于 `visual-review.json`，各图片哈希一并留存。
- 同包 consumer artifact `13255013680`，SHA256 `be88b0ef850f1c3581420c70c5cb9e0423f86c7e67dfd15f11b74a5221b7aec4`，已下载验证。最终可机器核对的交付摘要见 `delivery-summary.json`。

以上证明固定哈希包在新 Server runner 上启动与限定工作区流程通过；Windows 10 19045.6093 真机仍为 USER_VALIDATION_PENDING，请用户解压完整新包复测。未将 Server 结果当作 Windows 10 / 11 真机验收。

## 独立审查

独立只读 reviewer 核验 `676b7330`，未发现阻塞问题；确认函数 ABI、借用模块与指针寿命、缺 API 资源回收、worker 内资源创建、pending guard，以及 fixture 搬移后原断言和超时保留。未运行本地测试/构建。

原四关闭 fixture 只见证外部 writer 首次 WouldBlock；严格输出背压证据来自独立的连续八次 WouldBlock 单测，不把两者混为一谈。单函数 pending guard 测试只证明其函数契约，完整 relay 归属另由 partial fixture 证明。

## 产物保留与范围

打包前库存：本 goal 保守上界 311.28 MiB。为新包保留空间，已备份并校验过时的 626debf2 包 artifact `13192906617`（外层 SHA256 `c1b6e932236bc8a16a2ed4a658ef7a0b858a6fb36e919fc922eccc016d2be3b8`）至 `/private/tmp/norma-winsock-archive/artifact-13192906617.zip` 后删除其远端副本，保留当前 94 包、全部失败诊断及其他任务产物。

本次不发布或合并。原 Windows storage 267 通过 / 15 失败的逐项清单及交付影响仍见 `docs/quality/evidence/windows-release-workspace-20261103/README.md` 和 `docs/quality/evidence/windows-workspace-create-20261103/storage-comparison.json`；本启动修复没有重跑或解决这 15 项。

手机命令并发根因排查保持现场、尚未修复。后续诊断 [41655531581](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41655531581) 已在同一 Server VM 上证明基线也可频繁暴露同类 DatabaseBusy（基线 2 成功 / 8 失败，观察提交 3 成功 / 7 失败，各十轮），所以不能将其直接认定为 7cf 的代码回归；具体 SQL 根因和修复仍缺证据。真实 Claude/Codex 验收由用户负责。
