# AGENTS.md - Multisoul Desktop 导航地图

> 本文是地图，不是说明书。它告诉 Agent 去哪儿查、先守哪些边界。
> 系统架构见 [`ARCHITECTURE.md`](ARCHITECTURE.md)；展开命令、工程细节、测试规范见 [`CLAUDE.md`](CLAUDE.md)。
> 与 `CLAUDE.md` 重叠的事实必须保持一致。

## 1. 这是什么

Multisoul Desktop 是 macOS 空间工作台脚手架。核心抽象是 **Live Node on Infinite Canvas**：无限画布承载可运行节点，Terminal 是第一个 Live Node。

Workspace 分层：

- `crates/canvas/` - 零 GPUI 域层：几何、节点、选择、命中、viewport。
- `crates/terminal/` - 零 GPUI 域层：PTY、ANSI grid、session、输入编码。
- `crates/runtime/` - 零 GPUI 域层：节点生命周期、焦点状态、输入路由。
- `crates/ui/` - GPUI 主题与共享 UI 基础。
- `crates/app/` - GPUI/Metal 壳层：窗口、菜单、快捷键、画布与终端渲染。

依赖方向：`app -> {ui, runtime, canvas, terminal}`，`runtime -> {canvas, terminal}`；域层不得依赖 GPUI。

## 2. 关键约束

- **先读本文**：每次会话开始先读 `AGENTS.md`，再按指针深入。
- **域层零 GPUI**：`canvas`、`terminal`、`runtime` 必须可独立 `cargo test`。
- **依赖方向不反穿**：不要让 `canvas/terminal/runtime` 反向依赖 `app` 或 `ui`。
- **版本钉死需审慎**：`gpui =0.2.2`、`alacritty_terminal =0.26`，升级前先验证 API。
- **GPUI 代码先查 skills**：编写或修改 `crates/app`、`crates/ui` 的 GPUI 代码时，优先参考本地已安装的 `gpui-*` skills，再结合本仓库现有写法落地。
- **单文件 ≤ 500 行**：Rust 源码由 [`scripts/check-max-file-lines.sh`](scripts/check-max-file-lines.sh) 拦截，超长先拆职责。
- **写代码前先 TDD**：新功能或 bugfix 先补能失败的聚焦测试，再实现。
- **测试注释必须完整**：测试上方写场景、数据构造、执行过程、预期结果；关键数值写推导。
- **断言必须具体**：每个 `assert!` / `assert_eq!` 带说明字符串；关键状态要有正断言和负断言。
- **PTY 生命周期要收口**：窗口关闭或 session 释放必须 kill/wait 子进程，避免僵尸进程。
- **交互决策放 runtime**：输入意图先通过 `runtime::route` 这类纯函数表达，再由 `app` 层执行副作用。
- **不要把知识塞进地图**：`AGENTS.md` 只放指针；长说明沉淀到 `CLAUDE.md` 或未来 `docs/`。

## 3. 技术栈速览

| 域 | 栈 |
|----|----|
| Shell/UI | Rust 2021, GPUI 0.2.2, Metal |
| Canvas | 自定义 geometry/document/selection/hit_test |
| Terminal | portable-pty 0.9, alacritty_terminal 0.26, crossbeam-channel |
| Runtime | 纯 Rust 状态机与会话注册表 |
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
| 焦点、输入路由、session 生命周期 | [`crates/runtime/src/`](crates/runtime/src/) |
| 主题色 | [`crates/ui/src/theme.rs`](crates/ui/src/theme.rs) |
| GPUI app shell、画布/终端渲染 | [`crates/app/src/`](crates/app/src/) |
| 历史 SDD 施工记录 | [`.superpowers/sdd/progress.md`](.superpowers/sdd/progress.md) |

## 5. 改完代码必跑的验证

| 改了 | 跑 |
|------|----|
| `crates/canvas/**` | `cargo test -p canvas` |
| `crates/terminal/**` | `cargo test -p terminal` |
| `crates/runtime/**` | `cargo test -p runtime` |
| `crates/ui/**` | `cargo test -p ui` |
| `crates/app/**` | `cargo check -p app` |
| 跨 crate 行为 | `cargo test --workspace` |
| GPUI/打包相关 | `cargo build -p app --release` |
| 域层依赖边界 | `cargo tree -p canvas -p terminal -p runtime | grep -i gpui` 应无输出 |
| 代码行数 lint | `bash scripts/check-max-file-lines.sh` |

## 6. 常用命令

```bash
cargo run -p app
cargo test -p canvas -p terminal -p runtime
cargo test --workspace
cargo check -p app
bash scripts/check-max-file-lines.sh
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
