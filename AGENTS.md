# AGENTS.md - Multisoul Desktop 导航地图

> 本文是地图，不是说明书。它告诉 Agent 去哪儿查、先守哪些边界。
> 系统架构见 [`ARCHITECTURE.md`](ARCHITECTURE.md)；展开命令、工程细节、测试规范见 [`CLAUDE.md`](CLAUDE.md)。
> 与 `CLAUDE.md` 重叠的事实必须保持一致。

## 1. 这是什么

Multisoul Desktop 是 macOS 空间工作台脚手架。核心抽象是 **Live Node on Infinite Canvas**：无限画布承载可运行节点，Terminal 是第一个 Live Node。

Workspace 分层：

- `crates/canvas/` - 零 GPUI 域层：几何、节点、选择、命中、viewport。
- `crates/terminal/` - 零 GPUI 域层：PTY、ANSI grid、session、输入编码。
- `crates/runtime/` - 零 GPUI 域层：LiveNode 生命周期、焦点状态、输入路由。
- `crates/storage/` - 零 GPUI 存储层：SQLite store、migration、recent/settings/canvas/session checkpoint。
- `crates/workspace/` - 零 GPUI 元数据层：最近项目类型、workspace source、路径展示。
- `crates/ui/` - GPUI 主题与共享 UI 基础。
- `crates/app/` - GPUI/Metal 壳层：窗口、菜单、快捷键、画布与 cards 渲染。

依赖方向：`app -> {ui, runtime, canvas, terminal, storage}`，`runtime -> {canvas, terminal, storage}`，`storage -> {canvas, workspace}`；`storage` 不得依赖 `app/runtime/ui`，`canvas/terminal` 不得依赖 `storage`。

### 源码组织

**Crate 分能力，文件夹分职责，逻辑沉底、适配上浮。** 目录示例见 [`CLAUDE.md`](CLAUDE.md) §3。

- 纯逻辑、零 GPUI → 域 crate；绑 UI、跑副作用 → `app`/`ui`
- 目录名写职责（`route`、`settings`），不写 `utils/`、`handlers/`、`views/`
- 新代码直接进目标目录；文件近 500 行或 ≥3 个 `prefix_*` 再建子目录

| 改什么 | ✓ | ✗ |
|--------|---|---|
| 命中测试 | `canvas/hit_test.rs` | `app/canvas/` |
| 滚轮缩放 | `app/canvas/` | `runtime/` |
| PTY / grid | `terminal/` | `app/cards/term/` |
| 终端绘制 | `app/cards/term/` | `terminal/` |

## 2. 关键约束

- **先读本文**：每次会话开始先读 `AGENTS.md`，再按指针深入。
- **域层零 GPUI**：`canvas`、`terminal`、`runtime` 必须可独立 `cargo test`。
- **依赖方向不反穿**：不要让 `canvas/terminal/runtime/storage` 反向依赖 `app` 或 `ui`；不要让 `canvas/terminal` 依赖 `storage`。
- **存储/性能热路径禁令**：不要在 `Runtime::drain_all`、PTY output chunk、`canvas_view::on_mouse_move`、`runtime::route`、`send_node_input`、关闭无脏 flush 中同步写 SQLite、扫描会话目录或阻塞查库。
- **GPUI 前台禁阻塞**：语音结束、LLM planning/refine、recognizer join、网络/文件 IO、长 sleep 或 CPU 重活必须移到 `background_executor`/后台线程；前台只做状态更新和 runtime UI 副作用。
- **视频/动画壁纸低拷贝**：live 视频/动画帧不得每帧经 CPU RGBA/`RenderImage`/`img()` 路径上屏；解码推进必须后台化，上屏必须用 420f `surface()` 或等价低拷贝路径，并由 `scripts/check-video-wallpaper-performance.py` 护栏拦截回退。
- **版本钉死需审慎**：`gpui =0.2.2`、`alacritty_terminal =0.26`，升级前先验证 API。
- **GPUI 代码先查 skills**：编写或修改 `crates/app`、`crates/ui` 的 GPUI 代码时，优先参考本地已安装的 `gpui-*` skills，再结合本仓库现有写法落地。
- **单文件 ≤ 500 行**：Rust 源码由 [`scripts/check-max-file-lines.sh`](scripts/check-max-file-lines.sh) 拦截，超长先拆职责。
- **Agent 指南 ≤ 300 行**：`AGENTS.md` 与 `CLAUDE.md` 由 [`scripts/check-agent-guide-lines.sh`](scripts/check-agent-guide-lines.sh) 拦截；超长内容归档到 `docs/`，并在 §4 增加指针。
- **禁止 `#[allow(...)]`**：Rust 源码不得用 allow 压制编译器或 clippy 诊断；由 [`scripts/check-no-allow.sh`](scripts/check-no-allow.sh) 拦截，必须修根因。
- **本地 hook 可安装**：运行 `bash scripts/install-git-hooks.sh` 后，原生 pre-commit 会跑 staged 约束和轻量 Rust 检查。
- **写代码前先 TDD**：新功能或 bugfix 先补能失败的聚焦测试，再实现。
- **测试目录按 crate 维护**：大型独立单元测试放 `crates/<crate>/tests/unit/` 并从源码模块用 `#[path]` 挂载；集成测试放 `crates/<crate>/tests/*.rs`；crate 专用 helper 放 `crates/<crate>/tests/support/`。
- **测试注释必须完整**：测试上方写场景、数据构造、执行过程、预期结果；关键数值写推导。
- **断言必须具体**：每个 `assert!` / `assert_eq!` 带说明字符串；关键状态要有正断言和负断言。
- **PTY 生命周期要收口**：窗口关闭或 session 释放必须 kill/wait 子进程，避免僵尸进程。
- **关窗必须退出进程**：GPUI 在 macOS 上关闭最后一个窗口后不会自动 quit，必须在 `main.rs` 的 `run` 闭包内注册 `cx.on_window_closed(|cx| { if cx.windows().is_empty() { cx.quit(); } }).detach()`；不得省略，否则进程常驻后台。
- **交互决策放 runtime**：输入意图先通过 `runtime::route` 这类纯函数表达，再由 `app` 层执行副作用。
- **新增可配置快捷键必须进 Settings**：任何新的 app shortcut 都要注册到 `shortcuts.rs`，并在 Settings → Keyboard Shortcuts 支持录制与恢复。
- **不要把知识塞进地图**：`AGENTS.md` 只放指针；长说明沉淀到 `CLAUDE.md` 或未来 `docs/`。

## 3. 技术栈速览

| 域 | 栈 |
|----|----|
| Shell/UI | Rust 2021, GPUI 0.2.2, Metal |
| Canvas | 自定义 geometry/document/selection/hit_test |
| Terminal | portable-pty 0.9, alacritty_terminal 0.26, crossbeam-channel |
| Runtime | 纯 Rust 状态机与 LiveNode 注册表 |
| Storage | rusqlite bundled SQLite、recent/settings/canvas/session checkpoint |
| Packaging | cargo-packager, 未签名 `.app` / `.dmg` |

## 4. Where to find

| 问题 | 去哪儿 |
|------|--------|
| 人类快速上手、Phase 路线 | [`README.md`](README.md) |
| 系统拓扑、数据模型、核心流程 | [`ARCHITECTURE.md`](ARCHITECTURE.md) |
| Agent 短导航 | [`AGENTS.md`](AGENTS.md) |
| 展开工程手册、命令、测试规范 | [`CLAUDE.md`](CLAUDE.md) |
| Workspace 依赖、版本 pin | [`Cargo.toml`](Cargo.toml) |
| 打包配置 | [`Packager.toml`](Packager.toml) |
| 几何/节点/选择/命中/viewport | [`crates/canvas/src/`](crates/canvas/src/) |
| PTY、ANSI grid、按键编码 | [`crates/terminal/src/`](crates/terminal/src/) |
| 焦点、输入路由、LiveNode 生命周期 | [`crates/runtime/src/`](crates/runtime/src/) |
| SQLite store、migration、持久化边界 | [`crates/storage/src/`](crates/storage/src/) |
| Workspace 元数据、路径展示 | [`crates/workspace/src/`](crates/workspace/src/) |
| 主题色 | [`crates/ui/src/theme.rs`](crates/ui/src/theme.rs) |
| GPUI app shell、画布/cards 渲染 | [`crates/app/src/`](crates/app/src/) |
| 源码组织 | [`CLAUDE.md`](CLAUDE.md) §3；展开见 [`docs/references/crate-file-map.md`](docs/references/crate-file-map.md) |
| Crate 文件地图 | [`docs/references/crate-file-map.md`](docs/references/crate-file-map.md) |
| 快捷键与输入路由 | [`docs/references/keyboard-shortcuts.md`](docs/references/keyboard-shortcuts.md) |
| 测试注释与断言规范 | [`docs/quality/test-requirements.md`](docs/quality/test-requirements.md) |
| 打包与 release | [`docs/references/packaging.md`](docs/references/packaging.md) |
| 卡片 RuntimeEvent 扩展 | [`docs/references/card-runtime-events.md`](docs/references/card-runtime-events.md) |
| 机械化约束（lint 脚本） | [`docs/quality/mechanized-constraints.md`](docs/quality/mechanized-constraints.md) |
| 历史 SDD 施工记录 | [`.superpowers/sdd/progress.md`](.superpowers/sdd/progress.md) |

## 5. 改完代码必跑的验证

| 改了 | 跑 |
|------|----|
| `crates/canvas/**` | `cargo test -p canvas` |
| `crates/terminal/**` | `cargo test -p terminal` |
| `crates/runtime/**` | `cargo test -p runtime` |
| `crates/storage/**` | `cargo test -p storage` |
| `crates/ui/**` | `cargo test -p ui` |
| `crates/app/**` | `cargo check -p app` |
| 跨 crate 行为 | `cargo test --workspace` |
| GPUI/打包相关 | `cargo build -p app --release` |
| 域层依赖边界 | `cargo tree -p canvas -p terminal -p runtime | grep -i gpui` 应无输出 |
| 存储层依赖与热路径边界 | `bash scripts/check-storage-boundaries.sh` |
| 拖拽/关闭/session 扫描性能护栏 | `bash scripts/check-performance-guardrails.sh` |
| 视频壁纸低拷贝护栏 | `python3 scripts/check-video-wallpaper-performance.py` |
| 代码行数 lint | `bash scripts/check-max-file-lines.sh` |
| 禁止 `#[allow]` lint | `bash scripts/check-no-allow.sh` |
| Agent 指南行数 lint | `bash scripts/check-agent-guide-lines.sh` |

## 6. 常用命令

```bash
cargo run -p app
cargo test -p canvas -p terminal -p runtime -p storage
cargo test --workspace
cargo check -p app
bash scripts/check-storage-boundaries.sh
bash scripts/check-performance-guardrails.sh
python3 scripts/check-video-wallpaper-performance.py
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
bash scripts/check-agent-guide-lines.sh
bash scripts/install-git-hooks.sh
cargo build -p app --release
cargo packager --release
RUST_LOG=trace cargo run -p app
```

Metal 工具链检查：

```bash
xcrun metal --version
```

## 7. 给 Agent 的协作约定

- 行动前先检索本地文件，不凭记忆回答。
- 涉及 GPUI API、元素、样式、actions、async、testing 或 troubleshooting 时，先加载对应 `gpui-*` skill；若 skill 与 `gpui =0.2.2` 或现有代码冲突，以本仓库验证结果为准。
- 修改代码后按第 5 节跑对应验证；验证失败先定位根因。
- 新增测试必须遵守第 2 节测试结构规范。
- 若新增长期文档，优先建立 `docs/product-specs/`、`docs/design-docs/`、`docs/exec-plans/` 这类 canonical 路径；`.superpowers/sdd/` 只作为历史上下文，除非明确继续旧 SDD 流程。
- 生成截图或图片时，保存为文件并在回复里用 Markdown 图片语法引用绝对路径。

## 8. 添加新规则的原则

新规则应来自真实踩坑。流程：

1. 复盘 Agent 或实现哪里失误。
2. 把短指针写进 `AGENTS.md`，把展开说明写进 `CLAUDE.md` 或 `docs/`。
3. 如果能机械化，补测试、脚本或 CI 约束。
4. 保持 `AGENTS.md` 和 `CLAUDE.md` 的重叠事实一致。
<!-- msctl-inject-start -->
## msctl Quick Reference

### Agent

```bash
msctl agent list             # List all registered agents
msctl agent get <id>         # Show agent details
msctl agent codex            # Quick register current project with Codex
msctl agent claude-code      # Quick register current project with Claude Code
msctl agent cursor-cli       # Quick register current project with Cursor
msctl agent kodax            # Quick register current project with KodaX
msctl agent register --name <name> --project <path> --runtime claude-code
msctl agent register --name <name> --project <path> --runtime codex
msctl agent register --name <name> --project <path> --runtime cursor-cli
msctl agent register --name <name> --project <path> --runtime kodax
msctl agent invoke <id> --message "<task>"
msctl agent delete <id>
```

### Ask User Question (`msctl ask-question`)

**MANDATORY**

- For structured decisions (approach, continue/stop, risk trade-offs, etc.), **prefer** the `AskUserQuestion` tool when available.
- **If the current runtime does not expose `AskUserQuestion`, you MUST call `msctl ask-question` to push a question card to iOS.** Never list options in free text or ask the user to type an answer.
- After pushing, continue other work. Do not poll; iOS answers are injected into the same conversation automatically.

Run `msctl ask-question -h` for flags, JSON shape, and copy-paste examples.

### Image Output

When generating images (charts, screenshots, diagrams), save them as files and
reference them in your reply using standard Markdown image syntax:

![description](/absolute/path/to/image.png)

Supported formats: png, jpg, jpeg, gif, webp.
The MultiSoul mobile app will automatically render these images inline.

<!-- msctl-inject-end -->
