# Windows runner 最小可行性验证证据

App 原116条已实测消除18条，仍98条；Windows关闭契约与Mac既有隔离问题未收束，迁移未验收。
范围包含 norma 编译修复、OS 资源回归、GPUI 基础图形与无认证 CLI 条件。
真实 Norma 图形、真实模型任务和候选包验收均未执行。
标准 runner 的实测 OS 为 Windows Server 2025，不能视作 Windows 11 设备验收。

## 实际运行与版本

| 运行 / attempt | 实际源码 SHA | 汇总结论 | 图形结果与调整原因 |
|---|---|---|---|
| [41295234651 / 1](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41295234651) | `cbd146d02e1d98ac51d3d8e9deb8879b01e0f9b4` | failure | ENV_UNSUPPORTED：未取得前台窗口控制 |
| [41296333985 / 1](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41296333985) | `366b47182f12c7ba68b589cc7bed8148fecdca11` | failure | FAIL：取得窗口原图，但探针误排除了 CSS green；键盘场景未执行 |
| [41297206819 / 1](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41297206819) | `58fe7ebdaee43f1e3cd6b12725e3f947282dcaa4` | failure | PASS：固定色样校准后，窗口原图与 OS Alt+F4 关闭通过 |
| [41306097667 / 1](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41306097667) | `bf03d7ef434a77dc5268d7ef257a4290c45cb031` | failure | 新增独立 norma 编译 gate，先复现原 14 个错误；GPUI 示例 PASS |
| [41307703559 / 1](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41307703559) | `dae96214474474dffe6e0fc96efabb9d2e14990f` | failure | norma 编译/43 项回归通过，canvas 52 项通过；App 出现 voice_input 的 5 个错误；GPUI 示例 PASS |
| [41312023075 / 1](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41312023075) | `e0e75071fb431d9ff93d432f7f95f899802f57a4` | failure | VAD 原 5 错误消除；43/52 项无回退；App storage 25 条诊断；语音测试 A1 编译失败 |
| [41312790317 / 1](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41312790317) | `e29f676e6fce66894b87a8ab7fb1e23e3538d8b0` | failure | 报告测试 9 项通过；VAD/43/52 无回退；App storage 与语音 A1 阻塞仍在；GPUI 示例 PASS |
| [41322579276 / 1](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41322579276) | `fbe37cf7a3aeddd36c34a4b72aa3eb321255bcbe` | failure | 新增三类平台回归，原样复现 storage 25 条、Review 入口和 A1 工厂失败 |
| [41325043580 / 1](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41325043580) | `f2838b21e04f7eb5c7ccecfc45271ec521ebe186` | failure | 语音 89/0/16；NT 文件 4 项通过；Git clone 2 项失败；App 新暴露 runtime/Codex 6 条 |
| [41327614822 / 1](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41327614822) | `747777db081a4f06005ee3b8b617de91db29fe8c` | failure | Git 修复后平台 30/0/0；语音 89/0/16；43/52 无回退；App 仍有 Codex 6 条阻塞 |
| [41335774179 / 1](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41335774179) | `c71b8e9f2327e6f8edf2735b030fd1406bb4d34e` | failure | 先复现6错、Windows随机身份与控制模块缺失 |
| [41338252762 / 1](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41338252762) | `9b9a4e0030714097f14cdd91fbe00e248410452c` | failure | 原6错消除；App116条；partial未进入Writing |
| [41341784394 / 1](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41341784394) | `15b284ce770a70d8b42ef6b6b2f8d4fa10bd398c` | failure | App116条；控制48/2/0；真实背压关闭失败 |
| [41345708985 / 1](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41345708985) | `e7a7a78a9a871748f28becf1a6de32702ce41cd9` | failure | App116条；控制50/0/0；平台47/0/0；语音89/0/16 |
| [41377422839 / 1](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41377422839) | `1d5b8e2441f3a0f9515b589ee4f2bf0fe92b7d51` | failure | App116条；独立App探针13条IPC编译RED；真实输出背压shutdown见证失败 |
| [41384140463 / 1](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41384140463) | `a53698f635c1501dd37aa4799add30fc4f68b164` | failure | App98条；App适配15/0/0；原43保留，新增shutdown见证失败 |
| [41385201505 / 1](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41385201505) | `bc8cdc1391abe2c6b87c61aa0fe3d23c740be5c5` | cancelled | 关闭见证前提修正后取消；日志下载缺失如实记录，不作通过证据 |
| [41386261217 / 1](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41386261217) | `120492ca29ec49b12ed4368eb0d10e01b5d34845` | failure | App98条；控制50/1/0；全部source释放仍未3秒关闭；OpenCode两项目标实际通过 |

前三次运行之间均有探针代码修正，不以无改动重跑覆盖断言失败。
新增回归门槛与产品修复分别提交，保留先失败、再通过的执行记录。
各轮原始报告均保留，没有把既有失败状态改成 PASS。
每轮结果来自其自身 SHA、run ID、attempt，不跨版本拼接成通过。

## 首轮收束运行的结果（41297206819）

| 门槛 | 状态 | 实测证据 | 尚未证明 |
|---|---|---|---|
| 产品编译 | FAIL | norma/app 的 MSVC check 均退出 101，首个失败依赖 norma 有 14 个错误 | 所有下游依赖、默认二进制、release、helper 启动 |
| GPUI 基础图形 | PASS | 钉死 GPUI 0.2.2 示例构建成功；真实窗口、RGB 像素、前台归属、OS Alt+F4 后窗口销毁 | 真实 Norma、文本输入、画布交互、主题、IME、DPI、多显示器 |
| 真实 Agent | NOT_RUN | Codex 0.166.0、Claude Code 2.1.316 安装/help/version 成功；Codex stdio initialize 成功 | 认证、任务、提问、取消、恢复、卡片与 session 归属、Norma Unix TUI relay 的替代 |
| 产品候选包 | NOT_RUN | 未产出产品包 | 同一候选包在新 job 下载、校验、启动与恢复 |

另有 canvas lib 测试 52 passed、0 failed、0 ignored。
四个门槛为 1 PASS、1 FAIL、2 NOT_RUN，`goal_complete` 为 false。
产品 AC01 为 FAIL，AC02–AC12 均为 NOT_RUN；完整矩阵见原 Goal 计划。
采集 job 成功表示报告生成成功，最终 gate 仍失败，不能把 workflow 当作产品验收绿色记录。

图形示例使用 `test/windows-probe/Cargo.lock` 的独立依赖图。
它复用 vendor 的示例源码，未改变产品 Rust 代码，也不证明产品依赖图已适配。
模型已读取首轮收束原图，核验记录见 `docs/quality/evidence/windows-feasibility-20261101/41297206819/visual-observation.md`。
前批 Sherpa 修复原图也已读取，记录见 `docs/quality/evidence/windows-feasibility-20261101/41312790317/visual-observation.md`。
本批最终原图见 `docs/quality/evidence/windows-feasibility-20261101/41345708985/visual-observation.md`。

## norma 编译入口修复（前批，41307703559）

先建立独立编译失败门槛，再修复产品代码。
失败基线为 [41306097667](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41306097667)，源码 `bf03d7ef434a77dc5268d7ef257a4290c45cb031`。
该轮独立 norma gate 为 failure，原 14 个诊断仍在，canvas 52 项通过且无忽略。

| 原根因 | 个数 | 修复边界 |
|---|---|---|
| 无条件 Unix Socket 导入 | 1 | Windows AF_UNIX 使用锁内 uds_windows 1.2.1；Unix 仍为标准库 |
| Unix 权限方法与 trait 导入 | 8 | Unix 保留原权限；Windows 使用当前用户专属的受保护 DACL |
| 根模块 cfg 排除后仍有公共类型引用 | 5 | 开放可移植问题/历史模块；打开句柄上读取稳定文件身份 |

新增回归包括真实 Socket 往返/错 ID、权限放宽检测与修复、原子替换、目录复用、reparse point 拒绝、追加与同长度替换的文件身份。
Codex 的既有 POSIX relay 未移植；Windows App 本地服务、真实 Agent、完整安全存储及候选包仍须分别验收。

macOS `cargo test --locked -p norma` 为 340 passed、0 failed、1 ignored。
该 ignored 是基线已有的 `real_grok_discovers_installed_skills_and_hooks`，需要已安装 Grok Build；本次没有新增或修改 ignore。
新增 Socket 测试单独复验通过；正常提交 hook 的 workspace check 和 fmt check 通过。

额外 i18n 护栏仍在 `crates/app/src/settings/voice/view/gpt_live.rs:70` 报既有的 GPT-Live 硬编码。
已核对该文件与修复前基线逐字节一致；本批没有修改 App 文案，不把此检查标为通过。

Windows 修复版运行：[41307703559](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41307703559)。
实际源码 `dae96214474474dffe6e0fc96efabb9d2e14990f`；运行已结束，整体为 failure。

| 本批验收 | 结果 | 原始日志 |
|---|---|---|
| 原 14 个 norma 错误 | 已消除，当前 norma 编译错误数为 0 | compile/norma_compile.log |
| norma Windows lib | PASS，退出 0，185.69 秒 | compile/norma_compile.log |
| norma 默认 CLI/helper check | PASS，独立 gate 成功 | workflow.log |
| norma 平台回归 | 43 passed、0 failed、0 ignored，153.86 秒 | compile/norma_platform_tests.log |
| canvas | 52 passed、0 failed、0 ignored，58.67 秒 | compile/canvas_tests.log |
| App | FAIL，退出 101，808.72 秒；5 个 E0433 | compile/app_compile.log |
| 真实 Norma 图形、真实 Agent、候选包 | NOT_RUN | 未执行，不由示例或协议夹具替代 |

43 项包括 lib 的 26 项，以及 client_platform、client_tests、codex_question、cursor_questions、protocol_tests 共 17 项。
没有宣称全部 Windows integration tests 均已运行；该测试范围没有任何忽略项。
新增的 DACL 检测/修复、原子替换、junction/符号链接拒绝、文件身份正反测试均已在 Windows 实际通过。

App 首个失败依赖为 `voice_input`。
`crates/voice_input/src/vad.rs:31`、45、64–66 行无条件引用 sherpa_onnx。
`crates/voice_input/Cargo.toml` 却只在 macOS 目标声明该依赖，产生 5 个 E0433。
本批未修改或禁用 VAD；下一步适配 Windows 原生依赖与构建，再继续暴露其他阻塞。
App 日志另有 workspace 的既有 unused metadata 警告，未用 allow 压制。
这些错误不是下游问题总数，App/release/候选包均未通过。

本批规范入口为 `docs/quality/evidence/windows-feasibility-20261101/41307703559/compiler-blockers.json`。
原始日志、原图与 SHA-256 清单为同目录的 manifest；本地 Mac 测试和正常 hook 记录位于 local 子目录。
本批公开源码和 native OS 夹具未使用模型凭据，模型调用数仍为 0。
Windows Installer 的 /dev/urandom 随机数来源与 Cursor 的 Unix 项目路径编码仍待后续运行适配。

## Sherpa/VAD 依赖边界修复（前批）

原运行 41307703559 在 voice_input 的 VAD 中暴露 5 个 sherpa_onnx E0433。
同一依赖声明错误也存在于 asr 和 voice_sherpa：三者使用无条件 Sherpa 实现，却只为 macOS 声明依赖。
本批仅将三处现有依赖移入 normal 段，保持锁内 Sherpa 1.13.3 和所有产品 Rust 实现不变。
没有禁用 VAD、A1 或其他产品能力，没有引入 test-support 构建替代。

### 先失败、后修复

- 依赖图回归：修复前 Windows 三个子用例 FAIL、macOS 三个 PASS；修复后六个全部 PASS。
- 报告回归：采集先上传，gate 只读本次源码/run/attempt；汇总与 gate 共用检查，非零退出码不能显示 PASS。
- 新增状态/退出码矛盾回归先失败后修复，最终报告 CLI 测试 9 项通过。

首轮修复：[41312023075](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41312023075)。
源码 `e0e75071fb431d9ff93d432f7f95f899802f57a4`，已结束，整体 failure。
voice_input 默认 Windows check PASS、exit 0、116.39 秒，原 5 个 VAD 错误消除。
原生语音测试命令 FAIL、exit 101、151.64 秒，编译时被 ASR 的 A1 工厂引用阻塞，尚未实际执行测试。

准确位置为 `crates/asr/src/providers/mod.rs:114`。
该处使用 A1RecognizerFactory，但 `crates/asr/src/providers/local/a1/mod.rs:8` 的 cfg(unix) 在 Windows 排除了其定义。
这是新暴露的产品编译边界；未删除该分支、关闭能力或将失败标为跳过。
同轮 norma lib 与默认 CLI/helper gate 通过；norma 43 项、canvas 52 项均通过且无忽略。
App check FAIL、退出 101、879.12 秒，实际首个失败依赖为 storage，共 25 条诊断。
其中 2 条为 workspace::review 被 cfg(unix) 排除后仍被 storage 引用。
其余 23 条来自 memory/fs 的 Unix 目录、文件描述符、openat/renameat 与硬链接 API。
完整诊断见 `docs/quality/evidence/windows-feasibility-20261101/41312023075/compiler-blockers.json`。
并行编译只暴露当前失败前沿，不能声称其他下游均正常。

最终报告一致性复验：[41312790317](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41312790317)。
源码 `e29f676e6fce66894b87a8ab7fb1e23e3538d8b0`，attempt 1，运行已结束，整体 failure。
该版只补强报告退出码校验，产品依赖修复相同。

| 最终复验项 | 实际结果 | 日志与边界 |
|---|---|---|
| 原 5 个 VAD 错误 | 全部消除；voice_input 默认 check PASS，退出 0，117.0 秒 | speech/voice_input_compile.log；未禁用 VAD |
| norma Windows lib | PASS，退出 0，187.72 秒 | compile/norma_compile.log |
| norma 默认 CLI/helper | PASS，独立编译 gate success | workflow.log；不代表 helper 已启动 |
| norma 平台回归 | 43 passed、0 failed、0 ignored，149.73 秒 | compile/norma_platform_tests.log |
| canvas | 52 passed、0 failed、0 ignored，48.47 秒 | compile/canvas_tests.log |
| App 默认 check | FAIL，退出 101，866.02 秒；storage 25 条诊断 | compile/app_compile.log；不是完整下游清单 |
| 默认语音测试 | 命令 FAIL，退出 101，149.92 秒；A1 编译阻塞 | speech/speech_tests.log；实际测试 NOT_RUN，不填通过数 |
| GPUI 示例 | PASS，窗口、像素、前台与 OS 键盘关闭通过 | graphics/ 原始日志与 PNG；已看图，非真实 Norma |
| 真实产品能力 | 真实 Norma 图形、Agent、候选包、物理录音与模型 VAD 均 NOT_RUN | 未用示例、夹具或编译代替产品验收 |

原始证据与校验清单：`docs/quality/evidence/windows-feasibility-20261101/41312790317/manifest.json`。
App 的 2 条 Review 错误位于两个 mobile_task store 导入；根因是 workspace 的 cfg(unix)。
其余 23 条位于 memory/fs 的 POSIX 目录和文件句柄实现。
语音的 A1 错误位于 `crates/asr/src/providers/mod.rs:114`。
`crates/asr/src/providers/local/a1/mod.rs:8` 在 Windows 排除了该工厂定义。
两轮均保留失败日志，没有删除产品分支或绕过必验门槛。

前批五个探针门槛：1 PASS、2 FAIL、2 NOT_RUN，goal_complete=false。
前批 AC 矩阵：0 PASS、1 FAIL、1 ENV_UNSUPPORTED、10 NOT_RUN。
G1、G2 与整体 Goal 均未完成。

原始失败记录与各 SHA 证据分别保存，不跨 run 拼接通过结果。

### 本机验证及限制

Mac workspace check 与 fmt check 通过，边界、性能、代理和依赖图检查通过。
默认三个 crate 的首次测试止于 ASR 的 4 项 A1 host 失败；仅重试一次并使用 no-fail-fast 收齐其他目标。
重试合计 90 passed、4 failed、17 个既有 ignored，命令退出 101。

- ASR：37 passed、4 failed、2 ignored。
- voice_input：26 passed、0 failed、6 ignored。
- voice_sherpa：27 passed、0 failed、9 ignored。

四项失败均在未修改的 A1 资源护栏报 system memory pressure elevated。
实测 kern.memorystatus_vm_pressure_level=2（WARN），实现要求 1（NORMAL）。
保留两次日志、内核查询与未修改源码的 SHA；没有放宽护栏或新增忽略。
本批证据入口：`docs/quality/evidence/windows-feasibility-20261101/41312790317/local/local-validation.json`。
同目录包含两轮原生测试日志、内核查询与 `unchanged-product-sources.json`。
i18n 护栏仍在未改动的 `crates/app/src/settings/voice/view/gpt_live.rs:70` 失败。
物理录音、模型 VAD、真实 Norma 图形、真实 Agent 和候选包仍为 NOT_RUN。

## storage/Review 与 A1 平台边界修复（前批）

失败基线：[41322579276](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41322579276)。
源码 `fbe37cf7a3aeddd36c34a4b72aa3eb321255bcbe`，已结束，整体 failure。
三个新增平台回归命令均退出 101；storage 原 25 条诊断、Review 入口和 A1 工厂错误均复现。
先保存失败版本，再提交产品修复；没有新增忽略或关闭产品能力。

| 修复范围 | 保留的契约 | 验证证据 |
|---|---|---|
| storage 的 23 条 POSIX 诊断 | Windows 使用 NT 目录句柄相对访问；拒绝 reparse、ADS、设备别名和硬链接；创建时施加私有 DACL | 既有 Unix 适配原样保留，338 项 storage/workspace 测试通过 |
| Review 的 2 条模块诊断 | 开放公共 Review；固定根及祖先身份，逐组件读取；保留链接类型和读前后检查 | Mac Review 定向复验通过；Windows 中文 capture 已通过 |
| 原子替换与持久化 | 使用同一目录句柄替换；普通读者不阻塞发布；同步失败不伪装成功 | Windows 旧读者/新指针、目录 flush 与创建冲突均实测通过 |
| A1 工厂与通信 | 保留真实工厂；Windows 管道有界收发；专属 Job 收口后代、取消与超时 | Mac 语音 96 通过、0 失败、17 既有忽略 |
| Review 子进程 | 父进程提前退出后也回收持 stdout 的后代，reader 不能永久等待 | Windows 父先退出、孙进程持 stdout 的 Job 生命周期反例已通过 |

App 探针扩为默认主程序与 helper，保留 norma lib 与独立 CLI/helper gate。
新增回归先证明只检查 lib 不充分，再修正命令；最终报告测试 10 项通过。
受影响语音测试使用 no-fail-fast，实际执行的通过、失败、忽略逐项保留。

### Mac 压力失败复查

本批修复前原样重跑 A1 host，0 passed、4 failed；内核压力为 WARN（2）。
资源策略文件与前批基线逐字节一致，前批 Sherpa 依赖调整没有修改该策略或宿主 Rust 实现。
四项生命周期测试使用协议夹具，却从真实系统读取压力，环境条件影响了测试前置。

修复把压力输入作为宿主内部依赖；生产 Default 仍调用原内核检查，NORMAL 阈值仍是 1。
夹具显式提供 NORMAL，并新增高压力拒绝启动和取消优先的反向断言。
5 项聚焦 host 测试通过；完整语音测试 96 passed、0 failed、17 个既有 ignored。

A1 模型引擎仍要求 Apple Silicon macOS，该既有限制未改。
本轮 Windows 测试只验证工厂、协议与进程。
物理录音、模型 VAD、真实 Norma GUI、真实 Agent 与候选包均未验收。

本机 storage/workspace 全部测试 338 passed、0 failed、0 ignored。
正常提交 hook 的 workspace check、fmt 与相关边界护栏通过。
i18n 仍在未修改的 `crates/app/src/settings/voice/view/gpt_live.rs:70` 失败，证据保留。
本机基线和修复使用 Homebrew Rust 1.93.1；Windows CI 与独立 API 检查固定为 Rust 1.91.1。
独立 API 检查只证明绑定与目标语法，不冒充完整产品编译或运行证据。

### Windows 修复版实测

运行：[41325043580](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41325043580)。
源码 `f2838b21e04f7eb5c7ccecfc45271ec521ebe186`，attempt 1，已结束，整体 failure。

| 首轮修复项 | 实测结果 | 结论边界 |
|---|---|---|
| 原 storage 25 条诊断 | 已消除；memory 平台测试编译并实际通过 | 不代表全部 Windows storage 测试都已执行 |
| norma lib / 默认 CLI-helper | PASS；43 项平台回归、0 失败、0 忽略 | 未启动真实 App helper |
| canvas | 52 passed、0 failed、0 ignored | 原测试未修改 |
| voice_input / 语音测试 | 默认 check PASS；实际 89 passed、0 failed、16 既有 ignored | 高压力、取消、父先退出/孙进程与无关进程反例通过；真实模型未验 |
| memory 平台回归 | 1 passed、0 failed、0 ignored | 真实保存、重开、硬链接拒绝 |
| Windows NT 文件回归 | 4 passed、0 failed、0 ignored | 目录 flush 实际 Ok；相对替换、旧读者、根/祖先固定、junction/ADS |
| Review 与平台 lib | 中文 capture、进程 Job 反例通过；总计 25 passed、2 failed | 两项失败为既有 Git clone runner 的 GitFailed，原始路径含 verbatim 前缀 |
| A1 工厂平台回归 | 1 passed、0 failed、0 ignored | 取消不启动模型；真实模型引擎仍要求 Apple Silicon macOS |
| 默认 App 编译 | FAIL，退出 101，852.70 秒；runtime 6 条诊断 | 默认主程序与 helper 均未通过，不由库或采集 job success 替代 |

App 新失败位置包括 `crates/runtime/src/nodes/agent/codex.rs:4` 与 UnixStream 导入。
`codex_host`、`codex_turns` 与 `live` 也引用 Windows 未开放的 native_questions/relay。
这需要 Codex 控制传输的真实 Windows 实现；不删除 Agent 入口或把 cfg 去掉后假报支持。
该历史运行的六条诊断保存在其 `compile/app_compile.log`，是当时的编译前沿。

### Git clone 路径修复与复验

Windows 新暴露的两个 clone 失败发生于 canonical 目标路径 `\\?\C:\...`。
补丁只给 Git argv 使用已锁定 dunce 1.0.5 的兼容路径。
OwnedTarget、Started/Completed、清理、安全收口与返回值仍保留原 canonical 身份。
新增回归记录原始路径和普通路径的真实 Git 对照，覆盖中文与空格目录。

后续运行：[41327614822](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41327614822)。
源码 `747777db081a4f06005ee3b8b617de91db29fe8c`，attempt 1，已结束，整体 failure。

| 最终复验项 | 实测结果 | 验收边界 |
|---|---|---|
| 原 storage 25 条诊断 | 全部消除；公开 memory 平台测试实际通过 | 完整 Windows 存储/恢复矩阵仍未齐 |
| 原 Sherpa/VAD 5 条错误 | 未复发；voice_input 默认 check PASS、退出 0、196.91 秒 | 物理录音与真实模型 VAD 未验 |
| norma lib / CLI-helper | PASS；库检查 185.66 秒，独立默认 gate success | 只代表编译检查，不代表 helper 已启动 |
| 原平台回归 / canvas | 43 / 52 项全部通过，各 0 失败、0 忽略 | 本轮没有新增忽略 |
| 语音默认测试 | 实际 89 passed、0 failed、16 既有 ignored；退出 0、442.66 秒 | A1 协议/取消/进程收口通过，Windows 真实 A1 模型未验 |
| 新增平台回归 | 30 passed、0 failed、0 ignored；三个命令均退出 0 | memory 1、workspace lib 23、Review 1、NT 文件 4、A1 工厂 1 |
| Git clone 路径 | 两项原失败与新增中文空格路径回归通过 | 原始 verbatim 参数退出 128/Invalid argument；普通路径与公开 Runner 退出 0 |
| 默认 App 编译 | FAIL，退出 101，840.84 秒；runtime/Codex 6 条诊断 | 主程序、App helper 与 release 均未验收 |
| 真实 Norma GUI / Agent / 候选包 | NOT_RUN | GPUI 示例和协议夹具不作为产品场景通过 |

运行已结束，整体 failure；源码 `747777db081a4f06005ee3b8b617de91db29fe8c`，attempt 1。
五个探针门槛为 2 PASS、1 FAIL、2 NOT_RUN；独立平台 job 为 PASS。
该批 AC 矩阵为 0 PASS、2 FAIL、10 NOT_RUN：AC01 与 AC11 未通过。
G1、G2、完整 Mac 回归和整体 Goal 均未完成，goal_complete=false。

本轮完整日志与 SHA-256 清单：`docs/quality/evidence/windows-feasibility-20261101/41327614822/manifest.json`。
编译与测试派生索引：`docs/quality/evidence/windows-feasibility-20261101/41327614822/compiler-blockers.json`。
本机基线、完整失败与隔离诊断：`docs/quality/evidence/windows-feasibility-20261101/41327614822/local/local-validation.json`。



### Mac 完整 workspace 结果与边界

按仓库规范执行完整默认 workspace：3415 passed、15 failed、52 个既有 ignored，退出 101。
失败为 App lib 的 14 项，以及 norma 的 OpenCode transport 1 项。
App 日志出现中英文期望差异及锁中毒连锁失败；OpenCode 夹具报 WouldBlock。

未改动相关断言或产品文案，仅各做一次隔离诊断。
对话组 38 项、提议回执 1 项和 OpenCode 1 项均通过。
这不能把完整运行改成 PASS；原始全仓失败日志继续保留，测试隔离问题尚未收口。
当前 Mac 4 项 A1 压力问题已复现并修复，但 Mac 整体验收仍未通过。



## Runtime/Codex 平台边界与最终复验（前批）

原六条 Runtime/Codex 编译错误已消除；App 仍未达到编译通过门槛。
前批运行：[41345708985](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41345708985)，源码 `e7a7a78a9a871748f28becf1a6de32702ce41cd9`，attempt 1，整体为 failure。

| Windows 实测项目 | 结果（通过/失败/忽略） | 证据与边界 |
|---|---|---|
| norma 库、CLI/helper 编译 | PASS | 默认 feature，MSVC check；不代表 App/release/启动通过 |
| 原平台回归及 relay lib | 47/0/0 | 原43项执行覆盖以本轮 `compiler-blockers.json` 为准；新增激活3项旧单测及1项partial单测 |
| canvas | 52/0/0 | 实际执行，无新增忽略 |
| 随机控制身份 | 1/0/0 | Windows OS CSPRNG；Unix保留原行为 |
| 控制传输及 relay | 50/0/0 | 本轮实际通过；历史关闭失败未因本轮通过被判定修复 |
| Runtime Codex 集成 | 11/0/0 | session、提问、输入阻塞与turn生命周期 |
| 语音 | 89/0/16 | 16项既有忽略，原Sherpa/VAD/A1错误未复发；非物理音频或真实模型验收 |
| storage/Review/A1独立平台回归 | 30/0/0 | 原生OS文件与进程夹具，不是完整产品场景 |
| App默认检查 | FAIL：116条诊断 | 全部位于App；原6条Runtime诊断未复发 |

各测试组包含重叠的lib用例，不能相加为唯一测试总数。

先提交失败测试：[41335774179](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41335774179)。
首轮实现：[41338252762](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41338252762)，原6条消除，App新暴露116条。
中间复验：[41341784394](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41341784394)，partial缓冲假设与真实背压关闭均失败。
保留各轮失败，不把新增夹具修正追写成旧运行通过。

partial单测现在真实写出4KiB后注入WouldBlock。
它保留Writing→Unknown/Sent、完整RPC及禁止重发断言。
它是确定性故障注入，不能替代真实OS背压关闭集成测试。

本轮关闭测试实际PASS，但前轮FAIL仍未关闭，readiness生产代码未改。
测试只确认外部writer的瞬时WouldBlock，未确定relay输出已背压；本轮绿色不能作为修复证明。
关闭可靠性记UNRESOLVED；不能用超时杀慢连接或有限预读冒充等价修复。

Mac完整基线为3427/3/52，首轮改后3418/17/52，修正Codex回执等待后3419/16/52，均退出101。
原15项在基线与改后逐项隔离均15/0/0；完整运行中的既有失败继续保留。
本轮观察到的Codex历史回执竞态已修正：原3秒内同时等Attached身份与SessionUsed历史；原断言保留。
最后仅替换partial故障注入后，norma/runtime实际687/0/12，退出0。

Mac使用Homebrew Rust1.93.1；Windows使用固定Rust1.91.1，环境区别已记录。

最终完整Mac另有OpenCode输入锁断言失败；未改基线额外20次中18次通过、2次失败于另一条状态断言。
最终输入锁症状未在基线复现，归因未收束，不能宣布Mac无新增回归或全仓通过。
i18n既有GPT-Live失败、52项全仓既有忽略和全部原始日志保留；没有新增ignore/allow或关闭默认feature。

逐项六错、Mac15项前后表、验收映射与剩余阻塞见 `docs/quality/evidence/windows-feasibility-20261101/runtime-codex-audit.md`。
完整日志及SHA-256清单位于 `docs/quality/evidence/windows-feasibility-20261101/41345708985/manifest.json`。
真实Norma图形、真实Agent、候选包仍为NOT_RUN；迁移Goal未完成。

## App IPC/图片边界与 Mac 16 项复验（本批）

原 116 条诊断已按 12 类根因逐项归档。
实现复验：[41384140463](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41384140463)，源码 `a53698f635c1501dd37aa4799add30fc4f68b164`，attempt 1，整体 failure。
先行 RED：[41377422839](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41377422839)，源码 `1d5b8e2441f3a0f9515b589ee4f2bf0fe92b7d51`。

| 项目 | 实测 | 门槛 |
|---|---|---|
| App 默认 check | 原 13 条 IPC、5 条图片文件错误消除；仍有 10 类 98 条 | FAIL，未默认编译成功 |
| 实际 App 适配 | Windows 15/0/0，含请求响应、端点身份、超时、取消、图片边界 | 仅前置通过，不替代 App/权限完整验收 |
| norma 与原平台 | 47/1/0；原 43 项通过名全部保留 | 新背压 shutdown 见证失败，命令 FAIL |
| Codex/Runtime 控制 | 控制 49/2/0；Runtime 11/0/0 | 强见证与原 shutdown 契约失败 |
| 独立平台/canvas/语音 | 30/0/0、52/0/0、89/0/16 | 16 个语音 ignored 为既有项 |
| Mac 完整 A/B | A 3428/14/52；B 3443/0/52 | A 的 14 项既有语言/锁失败仍保留，不能宣称修复 |
| Mac 两项 OpenCode | 确定性输入锁 RED、HTTP 非阻塞继承；修复后 A/B 完整均通过 | 原 session、唯一 POST、不重发与门闩均保留 |
| 真实 Norma GUI/Agent/候选包 | NOT_RUN | 未以图形示例、协议夹具或 CLI version 冒充 |

App IPC 保留 32/64 队列与 4 个 worker，增加可取消的绝对 IO 预算。
Windows 以 AF_UNIX reparse tag 与 volume/fileIndex 识别端点，通过 pinned HANDLE 删除。
图片复用私有 DACL，拒绝 reparse；Unix 文件与 Socket 行为保留。
没有关闭 VAD、Sherpa、Codex、Browser 或默认产品 features，没有新增 ignore/allow。

关闭日志记录 131072 字节未读，mask0 没有事件，RDNORM 只有可读。
当时 writer 仍持有 source 句柄，只能证明 shutdown 后未在 3 秒退出，不能证明真实退出后的收口。
强见证已修正为 writer 先 drop 并 ack，join、释放最后 clone 后才开始原 3 秒窗口。
旧 shutdown 契约继续保留，不能用新测试覆盖旧 FAIL。

最终复验：[41386261217](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41386261217)，源码 `120492ca29ec49b12ed4368eb0d10e01b5d34845`，attempt 1，整体 failure。
App 默认 check 仍为 98 条，原 18 条消除；norma 库/CLI/helper 编译通过。
原平台目标为 47/1/0，原 43 项通过名完整保留；失败只有新增的严格 close 见证。

控制 50/1/0、Runtime 11/0/0、identity 1/0/0；两个 OpenCode 目标各 1/0/0。
实际 App 适配 15/0/0、独立平台 30/0/0、canvas 52/0/0、语音 89/0/16。

强见证记录真实输出 8 次连续 WouldBlock、5 个源帧、全部 source 句柄释放 true，原 3 秒内仍未退出。
原 shutdown 测试本轮通过，但历史 FAIL 没有生产修复证据，可靠性仍未收束。

当前 AC 为 0 PASS、3 FAIL、9 NOT_RUN；AC01、AC02、AC11 不标完成。

中间运行 41385201505 已因关闭见证前提修正主动取消，不用作通过证据。
报告 SHA、run/attempt 一致，未缺报告；完整日志和哈希清单保存在对应 run 目录。


原 116 与当前 98 逐项对照、Mac 16 项前后与所有日志入口：
`docs/quality/evidence/windows-feasibility-20261101/app-platform-audit.md`。
`docs/quality/evidence/windows-feasibility-20261101/app-116-comparison.json`。
`docs/quality/evidence/windows-feasibility-20261101/app-platform-local/macos-16-comparison.json`。
本分支提交 Draft PR，不合并；AC01、AC02、AC11 均未关闭，迁移 Goal 未完成。

## 实测环境

- runner label：`windows-2025`；实际镜像 `win25-vs2026`，版本 `20261021.278.1`。
- OS：Windows Server 2025 Datacenter x64，10.0.26100。
- Rust 固定 1.91.1，target 为 `x86_64-pc-windows-msvc`；Node 22.23.4。
- UserInteractive=true；进程与活动控制台均为 session 2；显示器为 1024×768。
- 显卡为 Microsoft Hyper-V Video；D3D11 hardware 与 WARP 创建均 HRESULT=0、feature level 11.0。
- GPUI 原图为 640×560；RGB 采样为 100、100、267；固定色样自检通过。
- 仅记录各任务结束时的磁盘余量，未测峰值，也未宣称标准镜像永远保持上述能力。

## 证据布局与校验

`docs/quality/evidence/windows-feasibility-20261101/` 下按 run ID 分目录，当前每轮均为 attempt 1。
每个目录包含以下文件，具体字节数和 SHA-256 以该轮清单为准；本批还增加 local 本机验证日志：

- `manifest.json`：run ID、attempt、源码 SHA、运行链接与逐文件 SHA-256。
- `run.json`、`jobs.json`、`artifacts.json`：GitHub API 原始运行、job 和产物元数据。
- `workflow.log`：完整 Actions 日志；与各任务的独立日志互相补充。
- `environment/report.json`、`environment/native.json`：环境与 DirectX 实测。
- `compile/report.json`：命令、退出码、耗时；同目录保存 canvas、norma、app 的实际日志。
- `graphics/report.json`、`graphics/native.json`：构建、窗口、像素和输入结果；有图像时保存 PNG 原图。
- `agents/report.json`：固定 CLI 版本、无认证检查及明确的 NOT_RUN 项；同目录保存帮助、版本和握手日志。
- `feasibility.json`、`summary.md`：当前源码和 attempt 的汇总结论。
- `compiler-blockers.json`：从各轮日志提取错误与测试计数；包含初始 norma 14 个、前批 App 5 个及本批的新阻塞。

上述布局中的短文件名用于说明各 run 目录内部结构。
首轮收束汇总入口为 `docs/quality/evidence/windows-feasibility-20261101/41297206819/feasibility.json`。
首轮收束校验清单为 `docs/quality/evidence/windows-feasibility-20261101/41297206819/manifest.json`。
CI 临时产物有 3–7 天留存期，本目录持久保存小报告、日志与原图；没有上传构建二进制。

## 边界与下一步

未提供模型凭据，未新增 Secrets，真实模型调用数为 0。
未复制本机登录信息，未启用付费 runner、签名、发布或机器采购。
真实 Agent 的后续验收需要受保护环境与已授权调用预算，缺失时继续保持 NOT_RUN。

迁移顺序为 Windows背压关闭 → App IPC与生命周期及其余原生后端 → Mac时序与存储恢复矩阵 → PowerShell/ConPTY/进程树 → 真实Norma → 真实Agent → 同版本候选包。
原 14 个错误只代表已观察到的编译前沿，不能视作全部适配工作，也不能据此下调估算。
G1 尚未完成，两个 Agent 的产品支持矩阵尚未冻结。

本批聚焦验证通过：报告语义 10 项、依赖图 6 个子用例、契约检查 4 项、域契约检查与 actionlint。
各轮 environment job 均执行依赖图与契约检查；历史报告测试从 8 到 9 项，本批加默认 App 目标回归后为 10 项。
前批语音测试曾被编译阻塞；本批已实际执行并通过。完整 Mac workspace 仍另列失败状态。
首轮未重建本机 App/workspace；本批已按正常 hook 执行 workspace check，未运行真实 App 或 release 构建。

现行计划：`docs/exec-plans/2026-11-01-windows-github-runner-goal.md`。
HTML 展示：`docs/exec-plans/2026-11-01-windows-github-runner-goal.html`。

逐项最新入口：`docs/quality/evidence/windows-feasibility-20261101/app-platform-audit.md`。
最终校验清单：`docs/quality/evidence/windows-feasibility-20261101/41386261217/manifest.json`。

前两批已分别回收独立缓存 1.8 GiB 与 2.1 GiB。
前批确认独立target无进程使用后，回收21.5GiB及API检查缓存70.5MiB。
主工作区缓存未动；日志见最终运行的 local 子目录。
