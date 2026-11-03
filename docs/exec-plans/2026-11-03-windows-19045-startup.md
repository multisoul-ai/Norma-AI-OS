# Windows 10 19045 启动缺少 WinSock 入口

用户实际系统为Windows10专业版22H2，19045.6093；旧94e4bc83便携包在窗口出现前报ProcessSocketNotifications入口缺失。优先处理此真实启动阻塞，手机命令并发工作树保留现场，其他范围不恢复。全部测试/构建在GitHub runner；正常提交hook照常，不发布或合并。

## 证据与边界

- 旧包内层SHA256：87ee66f8985766bbc611e5d4530e9edd3533fa27f26777037ffd3fcdcde9eba7。
- 直接分析实际PE导入表：仅norma-os.exe硬导入ws2_32.dll的ProcessSocketNotifications；另三个exe没有该导入。旧包不包含ws2_32.dll替代文件。
- [微软API要求](https://learn.microsoft.com/en-us/windows/win32/api/winsock2/nf-winsock2-processsocketnotifications)：该入口最低Build20348；用户19045不具备。Server2025测试不能证明19045可启动。
- 来源位于crates/norma/src/codex/relay/transport/windows.rs；必须移除函数的静态导入，不能只在调用外做版本if。

## 验收映射

| 条件 | 证据 |
|---|---|
| 旧包真实回归RED | 新import检查在新Windows runner下载同一94包，dumpbin识别强制导入并失败 |
| 新包不再强制依赖此入口 | 同包四个exe真实导入检查；缺API分支测试 |
| 保留关闭/取消/提问/背压契约 | 真实AF_UNIX测试、relay/backpressure/取消测试，强制API不可用的实例级用例；不放宽断言 |
| release交付 | 固定修复SHA、新release ZIP及hash，新runner下载相同hash实际启动与中文创建重开 |
| Windows10真实验证 | 用户19045设备复测；Server2022/2025或强制缺API测试都不标为Win10真机通过 |

## 当前进度

- 已取得真实 RED：[run 41669112175](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41669112175)，检查同一旧包真实导入表，失败为 `Mandatory ProcessSocketNotifications import prevents loading on Windows 10 build 19045`。artifact 13247980606 SHA256 为 67274862f1ef1e48dc457befa0a2966bed87bc4c014140ab7ffed5ff1c2e9915。
- 实现改为通过已加载的 ws2_32.dll 动态解析可选 API；缺 API 时不创建 IOCP/额外 socket，保留 5ms WSAPoll、250ms Ping 及 pending answer 的写入所有权。
- 验证 seam：包内真实 PE 导入表；真实 AF_UNIX relay 的部分写入、全部源句柄关闭；底层恢复可写时 pending answer 不能被探测 flush。缺 API 通过实例级工厂强制选择，无全局覆盖或环境开关。
- 新旧系统路径将在 windows-2022、windows-2025 runner 运行，macOS 只回归受影响 norma crate；测试、Clippy、构建均不在本地运行（正常 commit hook 除外）。
- 受影响 Windows 测试已完成：[41675679325](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41675679325)，Server 2022 / 2025 各 81 通过、0 失败、0 忽略，聚焦各三轮均 8 通过，Clippy/fmt 均通过。macOS 复用同实现 [41673358826](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41673358826)，351 通过、0 失败、1 既有真实 Grok ignored，三轮和 Clippy/fmt 通过。
- 首轮 Windows 完整 norma crate 编译被未改动的 Unix 权限测试阻塞，完整失败日志保留；该全套状态仍为 COMPILE_BLOCKED，不以受影响测试通过替代。第二轮旧诊断 helper 的 Clippy 初始化警告已原值修正，不增加 allow。
- release 固定提交 `15338abf2ad49406b1447bda0da648e4e3052010`，[41676826809](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41676826809) 的 build / verify-candidate 均 SUCCESS。修复 ZIP SHA256 `e590276bdb5ee5bf2b4a939257600180d79a2098bcfc35a50b1c5652a783e6d1`；[下载](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41676826809/artifacts/13255067769)。
- 独立新 runner [41686500929](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41686500929) 下载同一哈希包、不重编译；四 exe 导入检查、helper / App 启动、中文创建、精确查询、关闭重开保持 ID 均 PASS，原图已核对。完整证据见 `docs/quality/evidence/windows-19045-startup-20261103/README.md`。
- Windows 10 19045.6093 真机仍待用户复测；存储 15 项失败和其他暂停范围不恢复，真实模型由用户验收。不发布、不合并。
