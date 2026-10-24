# 代码结构与清理候选审计

日期：2026-10-24。基准：`dev`，HEAD `0aed07c2`，并读取当前工作区文件。
状态：下文保留清理前的审计快照。用户随后确认删除 spikes 和旧 planner；2026-10-24 已执行，并同步取消 planner 打包资源项。根目录的 `workflows/` 已按用户最新决定恢复并保留。历史位置可从 Git 提交 `0aed07c2` 查阅。
其余候选仍待处理。已有派工动效、存储及原型的未提交修改不属于清理范围。

## 1. 结论

- Cargo workspace 有 21 个 crate，目前没有证据支持直接删除其中某一个完整 crate。
- 明确遗留主要是未挂载源码、无实现占位、旧设置状态、工具输出和资源副本。
- App 的组织复杂度比 crate 数量更值得治理：约 67.3% 的 Rust 源码行数集中在 App；208 个源码文件包含 `impl MultisoulApp`。
- 架构导航有事实漂移：部分文档仍把多窗口、移动端控制、持久化、签名公证描述为未实现。
- 有一份有效麦克风测试文件未挂载，应恢复测试覆盖，不能当作无用文件删除。

统计口径：`git ls-files -z` 中的受跟踪文件，读取工作区内容；不计未跟踪文件。源码行数为各 crate 的 `src` 下 Rust 物理行数，包含注释、空行及内嵌测试，并非有效业务代码量。仓库受跟踪文件共 4,121 个；Rust 源码 207,388 行，App 源码 139,674 行。

## 2. 当前结构

### Rust workspace

| crate 入口 | 当前职责 | 源码行数 |
| --- | --- | ---: |
| `crates/app/src/lib.rs` | GPUI 启动、窗口、设置、桌面工作空间、媒体、移动端入口及各能力适配 | 139,674 |
| `crates/ui/src/lib.rs` | 主题、国际化、图标与组件库入口 | 958 |
| `crates/canvas/src/lib.rs` | 几何、节点、文档、选择、命中与布局 | 2,482 |
| `crates/terminal/src/lib.rs` | PTY、ANSI grid、输入、会话与子进程管理 | 3,899 |
| `crates/runtime/src/lib.rs` | LiveNode 生命周期、输入路由、运行状态、会话与桌面导航策略 | 12,356 |
| `crates/workspace/src/lib.rs` | 项目及工作空间元数据、想法等领域模型 | 2,880 |
| `crates/storage/src/lib.rs` | SQLite、迁移、工作空间、Agent 任务、会话与设置持久化 | 13,835 |
| `crates/agent/src/lib.rs` | NormaSession、模型调用、工具、记忆、Agent 协作规则 | 6,716 |
| `crates/norma/src/lib.rs` | CLI、协议、客户端、Agent hooks、编排安装、移动端通信协议 | 12,319 |
| `crates/audio/src/lib.rs` | 共享音频基础类型 | 53 |
| `crates/voice_input/src/lib.rs` | 麦克风、设备、采集、重采样、PCM 与 VAD | 1,471 |
| `crates/asr/src/lib.rs` | 语音识别协议、Provider、本地/云端识别 | 1,883 |
| `crates/dialogue/src/lib.rs` | 对话回合、聆听/播报策略、语音积压与报告队列 | 927 |
| `crates/tts/src/lib.rs` | TTS Provider、模型、Voice、流式合成能力 | 2,398 |
| `crates/voice_sherpa/src/lib.rs` | Sherpa 引擎适配及本地语音管线 | 1,628 |
| `crates/live_voice/src/lib.rs` | 连续实时语音会话、协议、上下文与音频节奏 | 1,472 |
| `crates/model_store/src/lib.rs` | 本地模型校验、解压与原子安装 | 408 |
| `crates/media_store/src/lib.rs` | 媒体目录、完整性、缓存安装与传输状态 | 460 |
| `crates/mobile_pairing/src/lib.rs` | 移动配对基础能力，兼有供 iOS 使用的 staticlib | 195 |
| `crates/license/src/lib.rs` | 授权验证；App 仍保留显式启用的 licenses 功能 | 214 |
| `crates/analytics/src/lib.rs` | 强类型产品分析事件、队列与发送 | 1,160 |

按 `cargo metadata --no-deps --offline --format-version 1` 核对的主要内部依赖：

| 上层 | 主要下层依赖 |
| --- | --- |
| app | UI、runtime、canvas、terminal、storage、workspace、agent、norma、语音与媒体能力等 |
| runtime | canvas、terminal、storage、norma |
| storage | agent、canvas、workspace |
| agent | norma |
| norma | mobile_pairing |
| voice_sherpa | asr、audio、dialogue、tts、voice_input |
| dialogue | audio、norma、voice_input |
| tts | audio、model_store |
| asr / live_voice / voice_input | audio；voice_input 另有 asr 测试依赖 |

`agent` 是模型与协作运行时；`norma` 是协议、CLI、hooks 和通信基础，两者不能按名称近似合并。`canvas` 与 App 内画布、`workspace` 与 App 内工作空间也是领域模型和界面适配的分层。

### App 内部

| 目录 | 文件数 / 行数 | 主要职责 |
| --- | ---: | --- |
| `crates/app/src/app/` | 223 / 42,571 | MultisoulApp、Agent 编排、Browser、语音、渲染与生命周期 |
| `crates/app/src/settings/` | 151 / 30,796 | 设置界面、状态、配置与能力安装 |
| `crates/app/src/workspace/` | 122 / 27,392 | 桌面场景、项目区域、抽屉、想法及项目会话 |
| `crates/app/src/mobile/` | 68 / 11,925 | 手机请求、任务、审批、通知与服务适配 |
| `crates/app/src/cards/` | 38 / 6,379 | Terminal、Agent、Browser、Preview 等卡片 |
| `crates/app/src/norma/` | 24 / 4,945 | App 侧协议入口、路由与副作用执行 |

其余主要是画布交互、预览、动效、媒体和应用级入口。App 另有 407 个受跟踪 Rust 测试文件、88,191 行测试代码。上述目录行数是组织规模证据，不能据此判断功能多余或性能差。

### Rust 之外

| 入口 | 职责与处理建议 |
| --- | --- |
| `ios/project.yml` | SwiftUI 手机 App 的工程源配置；界面在 `ios/App/`，共享逻辑在 `ios/Sources/NormaCore/` |
| `ios/Package.swift` | NormaCore、Rust pairing bridge、PairingProbe 与单元测试 |
| `services/norma-cloud/server.mjs` | 统一云网关、语音与 APNs relay；独立于桌面 Cargo 构建 |
| `test/desktop-e2e/Package.swift` | 当前真实 App 的 SwiftPM 验收驱动，不是已移除的 Rust e2e 套件 |
| `scripts/desktop-e2e.sh` | 当前桌面验收入口；其余脚本承担构建、发布、护栏和专项验收 |
| `tools/cargo-packager-wrapper/src/main.rs` | 公证环境隔离与显式公证开关；由 `.cargo/config.toml` 的 alias 使用 |
| `packaging/media/resources.json` | 发布资源的路径、字节数、哈希及打包策略 |
| `vendor/gpui-0.2.2/`、`vendor/sherpa-onnx-sys-1.13.3/` | Cargo patch 依赖源，不能按第三方副本删除 |
| `spikes/norma-spatial-prototype/package.json` | 独立 Web/VR 探索原型；退出探索后可整体归档 |
| `spikes/keygate-mvp/README.md` | Keygate 本地验证环境；生产授权不通过其脚本实现 |
| `docs/README.md` | 文档现行入口；历史、原型和研究材料需要按引用关系整理 |

## 3. 第一批：高置信清理候选

| 对象 | 证据 | 实施时的配套动作 |
| --- | --- | --- |
| `crates/app/src/settings/hooks.rs` | 371 行旧 Hooks 设置；当前设置模块及 `#[path]` 加载链均不挂载它；`hooks_settings` 和五个旧 toggle 方法没有文件外调用 | 删除旧文件，并处理下一项状态。现行入口为 `crates/app/src/settings/content.rs:167` 与 `crates/app/src/settings/orchestration.rs`，安装控制在 `crates/app/src/settings/orchestration/controller.rs` |
| `crates/app/src/settings/panel.rs:103` | HookInstallStatus、五个 provider status 字段及默认值，消费者仅剩未挂载的旧 Hooks 文件 | 一并删除状态与初始化；保留当前 orchestration 状态和真实 hook 服务 |
| `crates/app/src/app/browser/ry_backend.rs` | 文件为 0 字节，未挂载；现行模块实际使用 `crates/app/src/app/browser/wry_backend.rs` | 删除空文件 |
| `crates/canvas/src/commands.rs`、`crates/canvas/src/history.rs` | 各只有两行未来功能注释，无实现 | 同时删除 `crates/canvas/src/lib.rs:2`、`crates/canvas/src/lib.rs:6` 的声明及文档占位指针 |
| `crates/runtime/src/snapshot.rs` | 仅一行 Phase 4 注释，无实现 | 同时删除 `crates/runtime/src/lib.rs:21` 的声明；保留真正使用的 session_snapshot |
| `scripts/desktop-workspace-e2e.swift` | 只输出迁移提示并 `exit(2)`；仓库未找到外部引用 | 删除报错占位入口，保留当前 SwiftPM 工具 |
| `./error.log` | 已被 Git 跟踪的一次历史工具调用日志，未找到实际消费者 | 移出版本管理；按需增加精确 ignore 规则 |
| `./brand_names.txt` | 品牌命名草稿，未找到实际消费者 | 可从当前树移除，Git 历史保留 |
| `./pelican-bicycle.html` | 独立插画演示，未找到构建、产品或文档引用 | 可从当前树移除 |
| `.playwright-mcp/page-2026-09-30T18-14-47-317Z.yml` | 一次浏览器界面快照，未找到消费入口 | 可移除，并避免后续临时快照入库 |

删除 hooks 时需修正仍指向旧页面的文档，例如 `docs/research/2026-10-18-kimi-code-cli.md:271`、`docs/research/2026-10-18-zcode.md:210`、`docs/research/2026-10-18-hermes-agent.md:279`。历史记录可加失效说明，不必改写成当前实施记录。

## 4. 第二批：整组退役或去重

### 旧 OpenAI planner

`sidecars/openai-card-planner/openai-card-planner.mjs` 共 144 行，仍处理旧 VoicePlanningRequest / VoicePlan。当前 Rust、Swift 与云服务源码未找到启动它的调用链。其路径引用集中于 `Packager.toml:21`、`Packager.dev.toml:25`、`scripts/test-packager-config.sh:116` 及打包/历史文档。

建议作为整组退役候选：删除脚本、两处资源项、只保护旧资源存在的断言，更新 `docs/references/packaging.md`。实施前复查是否存在仓库外手工调用；完成后运行打包配置检查及正常发布验证。打包声明是实际依赖，不能只删脚本而保留配置。

保留旧配置兼容读取：`crates/app/src/settings/voice/state/values/load.rs:9` 读取旧 planner model，并在同文件第 123 行回退到现行 OpenAI provider 模型。该迁移逻辑不能随 sidecar 一起删除。

### 壁纸副本

`assets/wallpapers/norma-space-2026-10-19/` 中五张 PNG，与 `crates/app/assets/wallpapers/` 下对应的五张 norma 壁纸逐一 SHA-256 相同；该目录内 ZIP 解包后的图片也逐一相同。

| 原始副本 | 现行 App 资源 |
| --- | --- |
| `assets/wallpapers/norma-space-2026-10-19/01-blue-horizon.png` | `crates/app/assets/wallpapers/norma-blue-horizon.png` |
| `assets/wallpapers/norma-space-2026-10-19/02-golden-orbit.png` | `crates/app/assets/wallpapers/norma-golden-orbit.png` |
| `assets/wallpapers/norma-space-2026-10-19/03-violet-nebula.png` | `crates/app/assets/wallpapers/norma-violet-nebula.png` |
| `assets/wallpapers/norma-space-2026-10-19/04-emerald-aurora.png` | `crates/app/assets/wallpapers/norma-emerald-aurora.png` |
| `assets/wallpapers/norma-space-2026-10-19/05-silver-moonscape.png` | `crates/app/assets/wallpapers/norma-silver-moonscape.png` |

保留现行 App 资源，去掉五张原始副本及 `assets/wallpapers/norma-space-2026-10-19/norma-space-collection.zip`，可减少当前检出约 20.3 MiB。需同步调整该目录的 `manifest.json` 与 `prompts.json`，保留生成来源并指向现行资源。这不会自动缩小 Git 历史或安装包，因为去掉的副本不在发布清单中。

图标、原型截图也有同哈希副本，但存在不同入口和用途；不按哈希自动删除。发布媒体必须继续满足 `packaging/media/resources.json` 与 `scripts/prepare-release-media.py` 的完整性约束。

### 旧语音编辑测试

`crates/app/tests/unit/voice_settings_edit_tests.rs` 共 80 行，没有模块挂载，仍调用 VoiceSettingsState 已不存在的 handle_key_down / handle_text_input。当前输入由 `crates/app/src/settings/voice/view/editable.rs:29` 的 VoiceTextInput 和 EntityInputHandler 处理。

建议确认 `crates/app/tests/unit/voice_text_input_tests.rs` 对现行文字输入、选区与保存行为的覆盖后删除旧测试；覆盖缺口应迁入当前测试模块。不要为让旧测试编译而恢复已退役 API。row_value 仍有 cfg(test) 实现，不能把它当作已删除的 API。

## 5. 应恢复的测试

`crates/voice_input/tests/unit/device_tests.rs` 有 5 个仍针对现行行为的测试：默认设备、指定设备、缺失设备、空白 ID、macOS 12 麦克风枚举符号兼容性。

`crates/voice_input/src/devices.rs` 没有挂载这份测试，Cargo 也没有显式测试 target；嵌套在 tests/unit 下不会被自动发现。源码实现仍存在，因此建议恢复挂载，运行 voice_input 测试并核对测试清单确实包含这 5 项。这里是验证缺口，不是可删除功能。此结论来自模块与 target 静态检查，本轮未执行测试枚举或编译。

## 6. 已排除的误删候选

- `crates/app/src/settings/keyboard.rs` 通过 `crates/app/src/settings/panel.rs:7` 的 path 覆盖与 `crates/app/src/settings/content.rs:17` 加载；其布局测试也已挂载。仅看通常的文件夹推导会误判。
- `crates/app/src/bin/multisoul_agent_hook.rs` 与 `crates/norma/src/bin/multisoul_agent_hook.rs` 虽相同，但 App 单包构建需要产生 helper。`scripts/test-app-release-hook-binary-config.sh` 明确保护这一约定，不能只保留 norma 包入口后直接删 App 入口。
- `crates/app/src/settings/agent_hooks.rs`、`crates/app/src/settings/agent_hook_ingress.rs` 是现行运行服务，与旧 Hooks 设置文件职责不同。
- `crates/app/src/workspace/legacy_agent_migration.rs` 仍由项目打开流程调用；旧数据升级与数据库 migrations 需要保留。
- `crates/license/` 与 Chromium 后端是显式可选功能；默认关闭不代表已退役。
- 语音链式模式、Sherpa 本地适配、连续实时语音承担不同功能，不应仅按名字相似合并或删除。
- `scripts/desktop-workspace-e2e.sh` 是仍被文档说明的兼容转发；`scripts/test-voice-e2e.sh` 调用现行所属 crate 测试，两者不能按“旧 e2e”名字删除。
- `ios/Resources/Avatar/Milltina.scn` 虽约 90.7 MB，仍由 `ios/App/CommandCenter/AvatarScene.swift:40` 加载；大文件不是无用文件的证据。

## 7. 文档与缓存

优先纠正文档事实，避免下一轮继续向旧入口补代码：

- `ARCHITECTURE.md:397` 仍列多窗口、移动端控制、签名公证为非目标，与当前实现不符。
- `README.md:44` 的 Phase 占位路线包含已经存在的持久化和发布能力。
- `CLAUDE.md:52`、`CLAUDE.md:103` 的无条件关窗退出/杀进程表述，与手机后台运行例外冲突。实际条件分支在 `crates/app/src/lib.rs`。
- `docs/references/crate-file-map.md` 的 Agent 文件导航仍包含已经变化的旧组织方式，需要更新成当前 module map。

文档受跟踪内容约 165.3 MiB，App assets 约 178.5 MiB，iOS Resources 约 87.2 MiB。历史方案和原型应在确认停止使用、处理现行规格引用后归档或删除；日期早不能单独作为删除理由。

本机另有被 Git 忽略的 build（约 1.6 GiB）、artifacts（约 1.7 GiB）、iOS SwiftPM 缓存（约 357 MiB）及桌面 E2E SwiftPM 默认缓存（约 332 MiB）。这些属于本地空间治理，不能计为源码删减；验收证据需要先保留，构建缓存需确认无任务使用。已有其他 worktree，不能批量清除。

## 8. 建议实施顺序与验收

1. 单独恢复麦克风测试挂载，检查 5 个测试真实进入执行清单，再运行 `cargo test -p voice_input`。
2. 删除第一批遗留代码与杂项，修正失效指针。按实际变更运行 `cargo check -p app`、`cargo test -p canvas -p runtime` 和必要的设置聚焦测试；遵守项目的发布构建与 lint 要求。
3. 整组退役旧 planner、迁移或删除旧编辑测试；对应运行打包配置检查、语音输入聚焦验证及所需真实 App 验收。迁移键保持兼容。
4. 壁纸去重，更新来源 manifest；验证现行媒体清单与打包资源仍完整。
5. 更新架构导航，再按实际生命周期和状态所有权逐步整理 App；不要把大规模目录搬迁混入清理提交。

本轮证据为离线 Cargo 元数据、受跟踪文件统计、模块/path/include/feature/入口引用核验、SHA-256 与 ZIP 内容比对，以及独立反证审查。不宣称已证明所有潜在运行时入口均不可达；涉及可执行脚本和产品功能退役的项目仍需在实施阶段验证。

## 9. 本次已执行的清理

用户确认后，已删除 `spikes/` 和 `sidecars/openai-card-planner/`，共 133 个受跟踪文件、30,786 行文本；空的 `sidecars/` 也已移除。根目录的 `workflows/` 及原有文档已按用户最新决定恢复并保留。两份 Packager 配置及相关文档已移除旧 planner 资源声明，旧设置迁移键仍保留。

两份保留的 iOS 原型测试原先借用 spike 内的 Playwright，现由 `docs/prototypes/package.json` 和 lock 文件独立管理，固定沿用 Playwright 1.63.0。

验证结果：

- 两套原型测试均在本机 Chrome 通过，包括想法/工作空间流程和归档的 W01–W09 九屏流程。
- `cargo build -p app --release` 通过，耗时 12 分 21 秒；编译器报告了现有派工动效代码的未使用项警告。
- 发布 hook 二进制配置、release feature 检查、Rust 文件行数、禁止 allow、Agent 指南行数、修改脚本的 shell 语法及 `git diff --check` 均通过。
- 独立审查未发现阻塞性的漏依赖或误删。
- 完整打包配置检查被既有壁纸预算问题阻断：155,109,352 字节（147.92 MiB），超过 146,800,640 字节（140 MiB）上限。本次未修改壁纸或预算。

用户随后确认目录合并，已将打包 wrapper 移至 `packaging/cargo-packager-wrapper/`，与 `packaging/macos/`、`packaging/media/` 并列，并移除空的根目录 `tools/`。该工具保留独立 workspace，三个源码/配置文件内容保持一致；`.cargo/config.toml`、`scripts/test-cargo-packager-wrapper.sh` 和打包文档已同步新路径。上文保留的旧工具路径属于清理前快照。

迁移后的 wrapper 聚焦测试通过，覆盖普通/公证模式的参数转发、调用工作目录和公证环境变量处理；三个源码/配置文件与 Git 中的原文件逐字节一致，脚本语法和 `git diff --check` 通过。迁移后再次运行 `cargo build -p app --release` 通过，耗时 7 分 05 秒，仅报告现有派工动效代码的未使用项警告。
