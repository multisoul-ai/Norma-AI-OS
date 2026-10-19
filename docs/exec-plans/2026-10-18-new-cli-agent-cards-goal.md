# Goal：接入 Kimi、ZCode、Pi、Hermes 智能体卡片

状态：ACTIVE（2026-10-18 创建）。代码与单元/集成验证已合并到基线分支 `feat/new-cli-cards-base`；**真实 App 验收（desktop-e2e）未做、委派链路未验证、若干产品决策待定，G5 未通过**，因此 Goal 不能标记 complete。逐项进度见 §7 与 §9。

**用户决定（2026-10-18）**：Kimi 接 Kimi Code CLI（命令 `kimi`，不接已归档的旧 `kimi-cli`）；ZCode 指 `zai-org/ZCode`；图标取官方素材并转成终端可显示的像素风格；快捷键 Kimi=`Cmd+7`、Pi=`Cmd+8`、Hermes=`Cmd+9`、ZCode 默认不绑定，全部进 Settings；hooks、会话恢复、编排都要做。

提交与推送：本 Goal 暂未获得提交、推送授权。完成工程验证后向用户确认，再提交；不得 `--no-verify`。

- 分支：建议从 `main` 新开 `feat/new-cli-agent-cards`。当前 `docs/norma-assistant-prototype` 是原型文档分支，不作施工分支。
- 方案细节与改动清单：[接入计划 HTML](2026-10-18-new-cli-agent-cards.html)。
- 模板：Grok 接入调研 [`docs/research/2026-07-19-grok-build-integration.md`](../research/2026-07-19-grok-build-integration.md)，代码入口 `crates/canvas/src/node/agent_profile.rs`。

## 1. Goal 与完成边界

让 `AgentProfile` 新增 `Kimi`、`ZCode`、`Pi`、`Hermes`，每种卡片都能从菜单、Dock、快捷键、助手与 `norma` CLI 创建，在 PTY 中运行对应官方 CLI，保存并恢复会话，通过 hooks 展示真实状态，并参与跨 Agent 任务编排。手机端能识别并显示新卡片。

不做：ACP 或其他非 PTY 嵌入；自动安装 CLI；默认使用跳过审批参数；改动现有五种卡片的行为。

## 2. 已核实事实

| CLI | 命令 | 续接 | Hooks | 状态 |
| --- | --- | --- | --- | --- |
| Kimi Code | `kimi` | 旧版文档：`--continue`/`-C`、`--resume [ID]`/`-r` | 文档未提 | 新版参数与会话目录待 G0 验证 |
| ZCode | `zcode` | README 未列 | 未列 | 全部待 G0 验证；警惕同名第三方包 `zcode-app-cli` |
| Pi | `pi`（Node 22.19+） | `-c`、`-r`、`--session <path\|id>`、`--session-id` | TypeScript 扩展，`~/.pi/agent/extensions/`，事件 `session_start`/`agent_start`/`agent_end`/`tool_call`/`tool_result`/`turn_end` | 文档完整 |
| Hermes | `hermes` | `-c`、`--resume <id\|标题>`、`--in <dir>` | 会话文档未提 | 会话在 `~/.hermes/state.db`，hooks 待 G0 验证 |

启动方式沿用 Grok：`/bin/zsh -lic "exec <命令>"`。

## 3. 施工范围

按“穷尽匹配驱动”：先在 `canvas` 加枚举值，由编译器逐处报缺口。

| 层 | 主要位置 |
| --- | --- |
| canvas | `node/agent_profile.rs`、`node.rs` |
| runtime | `nodes/agent.rs`、`command.rs`、`session_snapshot.rs`、`agent_hooks.rs` |
| storage | `session_store.rs`、`agent_definition_store.rs`（稳定值 `kimi`/`zcode`/`pi`/`hermes`，兼容旧库） |
| norma | `protocol.rs`、`cli.rs`、`cli/*`、`client.rs`、`agent_hook/*`、`orchestration/*` |
| agent | `adapter.rs`、`adapter/commands.rs`、`master_prompt.rs`、`environment.rs`、`conversation_tail.rs` |
| app | 菜单、action、快捷键与 Settings、Dock、Settings Agent 选择与 hooks 面板、初始化动画、`agent_runtime/*`、`norma/server/*`、图标资源 |
| ios | `Sources/NormaCore/Snapshot.swift` 的 profile 解码 |
| 文档 | `CLAUDE.md`、`AGENTS.md`（保持同步且各 ≤300 行）、`docs/references/keyboard-shortcuts.md` |

约束：单个 Rust 文件 ≤500 行，`menu_bar.rs`、`settings/hooks.rs`、`agent_runtime/coordination.rs` 逼近时先拆；会话 ID 取自 hook 缓存，不在 PTY drain、渲染、关闭路径扫描目录或查 SQLite（Hermes 尤其如此）；不加 `#[allow]`；UI 遵循 `apple-design`，菜单与选择器遵循 Material 菜单规范，所有控件覆盖 hover、pressed、focus、selected、disabled。

## 4. 阶段

| 阶段 | 任务 | 完成标志 |
| --- | --- | --- |
| G0 探测 | 安装（经用户同意）并记录四个 CLI 的 `--help`、版本、登录流程；核实 Kimi Code 新版续接、hooks、会话目录；读 `apps/zcode-cli/` 源码；确认 Hermes hooks；确认 `command -v zcode` 为官方二进制；结论写入 `docs/research/2026-10-18-new-cli-agents.md` | 每个 CLI 的 hook 方式与恢复命令有书面结论；没有的标注“不支持”及降级方案 |
| G1 图标 | 取官方素材并记录来源与授权；脚本（放 `scripts/`，路径由参数传入）按现有图标网格像素化；产出 `toolbar-*.svg` 与卡片动画帧 | 浅色/深色真实 App 中 16、24 px 可辨认；授权不允许的改字母占位并标注 |
| G2 基础接入 | 先写失败测试，再实现 canvas/runtime/storage/norma/agent/app 的 P1 项：创建、启动、保存、菜单、Dock、Settings、快捷键、图标、动画、手机端解码 | 范围测试与 `cargo check -p app` 通过；四种卡片在真实 App 中创建并显示 CLI 首屏 |
| G3 hooks 与状态 | Pi 先做（写带 Norma 标记的扩展，拒绝覆盖用户文件，卸载只删自己的）；Kimi、Hermes、ZCode 按 G0 结论；归一化为 Ready/Thinking/RunningTool/Done/Failed | hook 单测通过；真实 App 提交任务后状态灯正确变化 |
| G4 恢复与编排 | 恢复命令（`kimi --resume`、`pi --session`、`hermes --resume`，ZCode 按 G0）；编排 Provider 枚举与安装器；`create_agent` 与委派 | 重启后恢复到同一会话；助手能创建并向新卡片派任务；不误标为其他 Agent |
| G5 验收与文档 | desktop-e2e 新增场景并登记模块；同步文档；跑全部护栏 | 完成定义全部成立 |

每阶段先补聚焦测试再实现，测试注释写明场景、数据、过程与预期，断言带说明字符串，并含正负断言。

## 5. 验收

| 验收项 | 证据 |
| --- | --- |
| 四种 profile 序列化、旧库兼容 | canvas、storage 单测 |
| 启动命令正确且无跳过审批参数 | runtime 单测断言参数 |
| 助手、语音、`norma` CLI 能创建新卡片 | agent schema 测试、norma CLI 测试 |
| profile 互转不混淆 | `identity`/协议映射表驱动测试 |
| 快捷键可在 Settings 录制与恢复 | shortcuts 单测 + desktop-e2e |
| hooks 安装不覆盖、卸载不误删 | norma 单测（参照 `agent_hook_opencode_tests.rs`） |
| 真实 App 创建、运行、恢复 | `scripts/desktop-e2e.sh` 新场景；覆盖不到再降级 `computer-use:computer-use` |
| 浅色/深色 UI 状态 | desktop-e2e 截图并看图清单 |
| 手机端显示新卡片 | `swift test --package-path ios` 与 Snapshot 解码测试 |

没有安装对应 CLI 的机器上，卡片须显示安装命令与官方链接，而不是一闪而过的 zsh 报错。

## 6. 验证命令

```bash
cargo test -p canvas -p terminal -p runtime -p storage -p norma -p agent
cargo test -p app
cargo check -p app
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
bash scripts/check-agent-guide-lines.sh
bash scripts/check-storage-boundaries.sh
bash scripts/check-performance-guardrails.sh
swift test --package-path ios
scripts/desktop-e2e.sh <模块>
```

## 7. 完成定义

- [ ] G0–G5 有实际产物、命令结果与记录。（G0 探测、G2 基础接入、G3 hooks、G4 恢复与编排的 norma 层已有产物与测试；G1 图标仅 Pi 完成官方像素转换，Kimi/ZCode 为字母占位，Hermes 已转换但商标条款未确认；G5 真实 App 验收未做。）
- [ ] 四种卡片的创建、启动、保存、恢复、hooks 状态、编排全部有对应测试与真实 App 证据；不支持的能力有书面依据和降级行为。（单元/集成测试已覆盖，不支持项见各调研文档的“已验证/未验证”表；**真实 App 证据缺失**；四个 CLI 的“模型按 skill 调用 norma”均未验证，设置页已标注“实验性：委派链路未验证”。）
- [ ] 图标来源与授权已记录，像素图标在深浅色下验收通过。（Pi 来源与 MIT 许可已记录；Kimi、ZCode 许可未验证，保持字母占位；Hermes 的官方图标商标条款未验证，待用户确认是否保留；深浅色真实 App 验收未做。）
- [ ] 快捷键进入 Settings 并可恢复；文档与两份 Agent 指南同步。（Kimi=`Cmd+7`、Pi=`Cmd+8`、Hermes=`Cmd+9`、ZCode 默认不绑定已注册到 `shortcuts/catalog.rs`，CLAUDE.md / ARCHITECTURE.md / keyboard-shortcuts.md 已同步；Settings 中录制与恢复的真实 App 验收未做。）
- [ ] 现有五种卡片行为和性能护栏无回归；PTY 清理路径无新增泄漏。（域层与 app lib 测试、6 个护栏脚本通过；app lib 有 1 个既有的顺序相关测试 `voice_redesign_task_layout_and_mode_persistence` 单独运行失败，基线复现，与本 Goal 无关；PTY 泄漏未专门验证。）
- [ ] 获用户授权后提交；hooks 未被绕过。（本地提交均经 pre-commit，未使用 `--no-verify`。）
- [ ] 最终报告注明可运行 App 与证据路径；仅此时将 Goal 标记 complete。

## 8. 可直接创建 Goal 的提示词

```text
在 feat/new-cli-agent-cards 分支（从 main 新开）实施 Kimi、ZCode、Pi、Hermes 四种智能体卡片接入。

唯一实施计划：docs/exec-plans/2026-10-18-new-cli-agent-cards-goal.md。
细节与改动清单：docs/exec-plans/2026-10-18-new-cli-agent-cards.html。
模板：docs/research/2026-07-19-grok-build-integration.md 与 crates/canvas/src/node/agent_profile.rs。

目标：AgentProfile 新增 Kimi（Kimi Code CLI，命令 kimi，不接已归档旧版）、ZCode（zai-org/ZCode）、Pi、Hermes。每种卡片可由菜单、Dock、快捷键（Kimi=Cmd+7、Pi=Cmd+8、Hermes=Cmd+9、ZCode 默认不绑定，全部进 Settings）、助手与 norma CLI 创建；在 PTY 中以 /bin/zsh -lic "exec <命令>" 运行，不加跳过审批参数；保存并恢复会话；通过 hooks 展示真实状态；参与跨 Agent 编排；手机端能识别。图标取各家官方素材并转成终端像素风格，记录来源与授权，授权不允许则用字母占位。

按 G0–G5 推进：先在真实 CLI 上探测 hooks、续接参数与会话目录并写入 docs/research，结论缺失的能力写明降级方案，不得臆测。安装 CLI 前须获用户同意。会话 ID 取自 hook 缓存，不在 PTY drain、渲染、关闭路径扫描目录或查 SQLite。

遵守 AGENTS.md：先 TDD；测试注释与断言说明完整；不加 #[allow]；单文件 ≤500 行；GPUI 部分先加载 gpui-* skill 并按 apple-design 与 Material 菜单规范，控件覆盖全部交互状态；新快捷键进 Settings。页面与样式验收优先 scripts/desktop-e2e.sh，覆盖不到再用 Computer Use；单测或构建通过不代表真实验收通过。不使用已移除的 crates/e2e。

持续更新计划的阶段状态与证据路径。只有完成定义全部成立才能将 Goal 标记 complete。未获授权前不要提交或推送；提交时 hooks 不得跳过。不要替换用户常驻 App。
```

## 9. 实时执行记录

| 日期 | 阶段 | 内容 | 结果 |
| --- | --- | --- | --- |
| 2026-10-18 | 创建 | 用户确认 5 项决定；盘点 `AgentProfile::OpenCode` 的全部匹配点形成改动清单 | 未写代码，G0 待开工 |
| 2026-10-18 | G0 探测 | 四个 CLI 真机探测：Pi 1.3.0、Kimi Code 2.2.0、Hermes 0.22.2、ZCode 0.16.9（源码构建）；调研文档与证据在 `docs/research/2026-10-18-*.md` | 完成；每项结论标“已验证/未验证”，未验证项不实现 |
| 2026-10-18 | G2 基础接入 | `AgentProfile` 骨架贯通 canvas/storage/runtime/norma/agent/app/ui；四分支并行实施、独立审查、修复后合并 | 域层 152 个测试二进制通过；app lib 1054+ 通过 |
| 2026-10-18 | G3/G4 hooks 与恢复 | Pi 扩展、Kimi `config.toml` 标记区块、Hermes 目录插件、ZCode JSON 配置的 hooks；恢复命令 `--session-id`/`--session`/`--resume`；会话发现一律 None | 完成；ZCode 无失败/中断事件，卡片会停在 Thinking |
| 2026-10-18 | G4 编排 | 四个 CLI 的 skill 目录实测后接入 `OrchestrationProvider::ALL` 与 hooks 安装；授权隔离；hook Conflict 不授权；设置页标注“委派链路未验证” | 完成 norma 层与设置页；Kimi/ZCode 的 hook 是共享配置合并写入，无独占冲突检测，待产品决定 |
| 2026-10-18 | 集成修复 | 合并后补 `cfg(test)`、拆分 `install.rs`（530→343 行）、补全测试夹具字段、`All` 安装测试不再写根目录 | 已修 |
| 2026-10-18 | iOS | `Snapshot.swift` 校验白名单缺新 profile，会让整份手机快照失败；抽出 `MobileAgentProfile.displayable`，手机端可创建名单不变 | 修复并补 3 个测试；NormaCore 114 个测试 0 失败 |
| 2026-10-18 | 文档同步 | CLAUDE.md、ARCHITECTURE.md、keyboard-shortcuts.md 补快捷键与新建入口 | 完成 |
| 待办 | G5 | 真实 App desktop-e2e 验收；Hermes 图标商标确认；Kimi/ZCode 冲突检测决策；用真实模型凭据验证委派链路 | 未做 |
