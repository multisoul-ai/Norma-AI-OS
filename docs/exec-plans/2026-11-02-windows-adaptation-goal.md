# Windows x64 适配 Goal

状态：ACTIVE。2026-11-02 用户要求重新规划，并将测试放到 GitHub runner。
本计划接替此前按编译错误逐轮推进的安排；历史证据继续保留。

## 目标与交付

完成当前默认产品能力的 Windows x64 原生适配，修复 PR #170，正常提交并推送代码，交付经过验证、可下载的 Windows 候选包。首个发行目标为 Windows 11 x64；测试执行环境使用标准 GitHub hosted `windows-2025`。Runner 的 Windows Server 2025 结果不写成 Windows 11 用户设备实测。

最终交付包含同一源码版本的便携 ZIP、SHA-256、逐文件 manifest、能力说明及 producer/consumer 两个 job 的运行证据。便携包保留主程序、三个 helper、必要 DLL 与 Resources。签名安装器和公开 Release 需要各自的发布条件，当前不创建公开 Release。

所有功能测试、Clippy、默认/release 构建和真实 App 验收在 GitHub runner 执行。macOS 回归使用 GitHub macOS runner。本地负责编辑、读取日志和整理证据，不主动运行测试、Clippy 或构建。唯一的本地检查例外是正常 `git commit` 自动执行的既有 hook（含 Rust 改动时的 `cargo check --workspace` 与格式检查）；它属于仓库强制提交完整性要求，不关闭、不替换，也不将其当成 Windows 验收证据。

## 当前事实与检查点

- 工作树：`/private/tmp/norma-windows-feasibility-20261101`。
- 分支：`codex/windows-feasibility-20261101`，PR #170。
- 已推送：`adf9cb21`、`76a21566`。最终 runner 验证尚未完成。
- `76a21566` 的 feasibility run `41469170895` 已失败：App IPC/附件 16 项通过；workspace lib 22 过/2 败（冲突恢复、子孙进程夹具）；norma lib 30 过/1 败（连续 WouldBlock 后关闭仍超出 3 秒）。
- 相同提交的 Windows build run `41469171059` 已失败：默认 App 从 98 条诊断缩至 1 条错误，`crates/app/src/mobile/controller/work.rs:192` 无条件引用 `objc2_foundation`；没有 release 包或真实 App 启动证据。
- macOS CI run `41469184105` 已失败：Brain 测试注释触发两条 Clippy `doc_lazy_continuation`。不能将旧提交的结果拼成新提交通过。
- 工作树尚有未提交的 Clippy 修复、PowerShell 准备、资源定位和交付脚本。这些属于待核对候选，不作为已通过证据。
- 重规划时的本地 diff、未跟踪文件哈希和 HEAD 已保存到 `artifacts/pr170-goal-handoff/checkpoint.json` 与 `working-tree.patch`。

## 执行顺序和完成门槛

| 阶段 | 工作 | GitHub runner 证据与完成条件 |
|---|---|---|
| G0 Runner 门禁 | 冻结输入；整理已有未提交改动；将规范工具链、日志、超时、缓存、source/run/attempt 校验接入 Actions；修复汇总遗漏与恒定失败 | Windows/macOS 工作流报告实际 rustc 版本；失败、缺报告、来源不符均不能假绿；Canvas 与所有必需原生检查进入汇总；已通过的可行性检查可通过 CI，完整迁移仍单独记录 |
| G1 传输与文件安全 | 关闭背压下的 socket；确认 WinSock 新通知是否适用于 AF_UNIX；完善目录能力与原子替换失败恢复 | 保留原 3 秒关闭契约、真实连续 WouldBlock 与全部 source 句柄释放见证；测试慢端、半关闭、取消、双向拥塞；附件交换/原地 reparse 反例、DACL、替换失败与恢复路径均有聚焦测试 |
| G2 原生终端和 Agent | PowerShell/ConPTY；Codex/OpenCode/Cursor 的实际生产启动包装器；版本判断、同一 TUI、私有 server/relay、原会话和 hook；Windows 进程树所有权 | 无模型 CLI 夹具经实际 controlled prepare 路径启动；真实 Agent 另按 W05/W06 必过；支持/不支持版本、中文空格/单引号路径、resume、提问、取消、恢复均验证；取消和关卡片后按 PID+创建时间确认父子孙全部退出，树外进程存活 |
| G3 完整 App 构建 | 逐个关闭默认产品构建前沿；WebView2、内容编辑/预览、手机网络与 QR、生命周期、资源定位 | 默认 `cargo check -p app` 和默认二进制通过；WebView2 真加载页面、导航和本地资源；内容冲突保护与附件能力通过；保留默认功能和依赖边界 |
| G4 其余默认能力 | 盘点并实现语音输入/输出、模型安装、媒体及 Ego 等 Windows 平台缺口；逐项保留结构化能力状态与双语提示 | 平台实现、真实 API/受控输入回归及能力清单一致；未实现项保持未完成，不能用隐藏入口或删功能关闭 Goal；硬件限制单独给出 ENV/NOT_RUN 和可执行验证入口 |
| G5 真实 App 与回归 | 在 hosted desktop 操作真正 Norma 程序；项目、画布、终端、Agent 夹具、保存重开、退出、手机后台生命周期；双语/主题关键状态 | Windows 的真实窗口、输入、最终状态、截图与进程证据；macOS 受影响测试与必要真实 App 回归通过；GPUI hello-world、TestAppContext、CLI version 均只作补充 |
| G6 候选包交付 | 从通过的同一 SHA 构建 release，打包资源/DLL/helper，上传 artifact，在新 job 下载并验证同一个包 | ZIP/逐文件 hash、必需资源/运行库、source SHA 校验通过；新 job 不重编译，源码位于不同目录；真实启动、关键用户流程、退出及遗留进程检查通过，产出截图与报告后交付 |

G1、G2 的纯协议夹具可以与 G3 的编译调查交错进行；发布包的成功判定必须等必需门槛通过。每轮先取得一个能失败的聚焦验证，再实现并复验原失败；发现新失败时只扩大到有证据的范围。

## Actions 编排

保留普通 PR CI，规范工具链固定 Rust 1.91.1。Windows 工作流按验证职责组织，避免一个长串命令阻塞所有日志：

1. `windows-contracts`：norma、Canvas、存储/文件边界、ConPTY、实际原生 Agent 启动与关闭。
2. `windows-app`：默认 App 编译、App adapter、WebView2 与真实桌面场景。
3. `macos-regression`：受影响 crate、workspace Clippy 和必要真实 App 回归。
4. `live-agents`：受保护 GitHub environment 的手动 Windows job，冻结 Claude/Codex 两个 CLI 的版本；各执行真实模型最小任务，并验证会话归属、提问回答、取消与恢复。只读无凭据探针和无模型夹具不能满足此 job。缺安全凭据/预算时保留 NOT_RUN，Goal 继续未完成。
5. `package`：必需 job 成功后构建 release、校验资源并上传候选包。
6. `verify-candidate`：独立新 runner 下载同一 artifact，校验、启动和检查最终结果。

工作流必须监听源码、manifest/lock、验证脚本、打包脚本、资源准备及能力说明的变更。已被具体编译错误阻塞的旧轮次可取消，保留取消事实；取消不作为通过记录。缓存按源码/依赖/工具链区分，不能以缓存成功代替执行。

## 已知阻塞，按优先级处理

1. **汇总假绿**：当前未提交的 summary 漏掉 `canvas_tests`；已发现 Canvas FAIL 仍可返回 0。先补缺失、FAIL、PASS+非零退出码三个反例，再修改门槛。
2. **生产 CLI 链未覆盖**：简单 `exec` 到 PowerShell 转换不支持 Codex/OpenCode 的多行版本/等待脚本、Cursor 环境变量；Codex 尚有 `/tmp`。按平台生成实际包装器，保留控制契约，用生产 prepare 路径验证。
3. **进程树关闭缺口**：PowerShell 不具有 POSIX exec 语义。证明 ConPTY/进程树是否收口；若不足则引入受控 Job Object。父进程退出本身不算子孙退出。
4. **完整 App 前沿**：已缩至手机截图路径的一处 Objective-C 引用，修复后以 runner 复验默认目标与新暴露错误；另有九条 warning，不能先宣称完整编译通过。
5. **交付验证不足**：package verify 必须要求 Resources/catalog/CRT，而非只验证已列出的文件；CRT 发现改用明确获取目录；Smoke 要检查并收口遗留子树；补 workflow 触发路径。
6. **Clippy 候选待确认**：自动修复涉及调用签名和共享测试夹具。先审查差异并以 runner 编译/测试验证；恢复无关格式噪音，保持单文件上限与测试语义。

## 默认能力与验收矩阵（G0 冻结）

以下软件门槛全部要求本轮来源一致的 PASS；FAIL、ENV、NOT_RUN 都阻止 Goal 完成。实现时可细分条目，不能删除范围或把软件缺口降为设备例外。

| ID | 必需能力 | runner 验收证据 | 当前状态 |
|---|---|---|---|
| W01 | 默认 App/helper、release、依赖边界 | MSVC 默认目标；护栏；macOS CI | FAIL |
| W02 | 文件/附件、DACL、原子替换、恢复与持久化 | 真实 NTFS 并发/reparse 反例；保存重开 | FAIL |
| W03 | PowerShell/ConPTY、中文路径、环境、输入与进程树 | 原生 PTY 及 PID+创建时间；树外 sentinel | NOT_RUN |
| W04 | Codex/OpenCode/Cursor 启动、relay、权限、提问和取消 | 生产 prepare 路径无模型夹具；严格关闭契约 | FAIL |
| W05 | Claude、Codex 各执行真实任务 | 受保护 live-agents job；产物、session/卡片回执 | NOT_RUN |
| W06 | 两个真实 Agent 的提问、回答、取消和同会话恢复 | live-agents job 的真实控制与迟到动作反例 | NOT_RUN |
| W07 | 项目、画布、终端、退出与手机后台生命周期 | 真正 Norma 窗口输入、最终状态、退出树 | NOT_RUN |
| W08 | WebView2、内容编辑/预览、资源和浏览器认证 | 真实页面导航、本地资源、编辑冲突及回调 | NOT_RUN |
| W09 | ASR/VAD、TTS、模型安装、音频软件管线 | 真实引擎与受控 PCM；安装完整性；错误状态 | NOT_RUN |
| W10 | 媒体/视频壁纸、Ego、图形及移动端网络/QR | 真正平台 API、帧推进、连接和权限生命周期 | NOT_RUN |
| W11 | 中英文、亮暗主题、焦点和键盘 | 真实 App 双语/主题；Ctrl+C 中断、截图 | NOT_RUN |
| W12 | 同一候选包可消费且 macOS 无回归 | ZIP/hash/manifest；fresh runner 核心流程；Mac 定向回归 | NOT_RUN |

仅物理麦克风/扬声器、摄像头、蓝牙和实体手机链路可单列设备例外：标准 hosted runner 缺设备时保留 ENV，并交付可执行验证入口。例外只限制物理设备结论，不能替代 W09/W10 的驱动/API 实现、受控输入、资源及生命周期证据；不把 Server 2025 结果写成 Windows 11 实机覆盖。

## 证据与状态

每次记录 source SHA、run ID、attempt、runner OS、实际工具链、命令、退出码、通过/失败/忽略数量、artifact 与 SHA-256。日志下载缺失单独记录，不补写成功状态。详细原始输出放 artifact，仓库保留简短索引和关键诊断，避免大量原始日志膨胀 PR。

`PASS` 表示该门槛真实通过；`FAIL` 表示产品或断言失败；`ENV` 表示运行条件不足；`NOT_RUN` 表示没有执行。可行性 CI 的绿色仅对应其列出的原生技术检查，`goal_complete` 必须与完整验收矩阵一致。

不把图形样例、协议夹具、编译通过或有截图单独等同产品验收。真实模型调用只使用已授权的 GitHub environment/secrets，不复制本机凭据；没有安全执行条件时明确保留未执行并继续其他可独立工作。

## Goal 完成条件

- W01–W12 所有必需软件门槛均 PASS，含两个真实 Agent 的 W05/W06；没有安全执行条件时不得完成 Goal。
- 所有已确认的数据安全、生命周期、会话/权限边界阻塞均关闭；既有失败不得以重跑或放宽断言替代修复。
- 默认能力适配矩阵逐项有实现与证据；设备相关限制清楚归类，未实现项仍属未完成。
- 正常提交并推送；PR 的状态、描述和证据入口反映最终实现。
- 用户得到可下载的 Windows 包、SHA-256、来源提交、验证链接和准确的剩余限制。

## G0 执行记录

- `999b39a7`：计划冻结及 Canvas 门禁回归。GitHub Windows run `41475394387`，attempt 1，job `125379500444` 执行 `python scripts/test-windows-probe.py`：18 tests，三条 Canvas 反例全部如预期暴露错误（missing/FAIL/PASS+101 均被错误判绿）。原始日志保留在该 job，未在本地执行测试。
- 下一提交将 `canvas_tests` 纳入独立必需门槛；只有本轮明确 PASS 且退出码为 0 才可通过。修复结果等待下一轮 runner，不将红灯提交记成通过。

- `a0616877`：Canvas 汇总修复。Windows run `41475988222`，attempt 1，job `125384598403`：Rust 1.91.1，18 tests 全过；完整 run 仍在执行，不宣称其余原生门槛通过。前一个红灯 run `41475394387` 已取消剩余编译任务，红灯 job 的日志保留。
- `deae23ec`：独立只读审查未发现 Clippy/fixture 候选中的调用或隔离回归；正常 hook 的 check/fmt 已通过，Windows/macOS runner 仍待执行此提交。下一批增加独立原生契约 workflow，保留冲突恢复和关闭原断言，仅增强失败诊断；Windows App 的配对名称改用系统环境名称，移除未受平台条件保护的 Objective-C 引用。

- `2c315b29`：Windows native contracts run `41477762062`，attempt 1，script job `125387736138` 的 report 18 项通过；package 3 项中 1 项失败，确认仅四个 MZ 文件和自洽 manifest、完全缺 Resources 的目录会被旧 verify 错误接受。先修必需资源和实际二进制检查，再继续 CRT/目录清单完整性；native job 尚在执行。
