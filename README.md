# Multisoul Desktop

macOS 空间工作台脚手架（垂直切片）。核心抽象 **Live Node on Infinite Canvas**，Terminal 是第一个 Live Node。

## 知识地图
- 系统架构：[`ARCHITECTURE.md`](ARCHITECTURE.md)
- Agent 短导航：[`AGENTS.md`](AGENTS.md)
- 展开工程手册：[`CLAUDE.md`](CLAUDE.md)
- 人类快速上手：本文
- 历史 SDD 施工记录：[`.superpowers/sdd/progress.md`](.superpowers/sdd/progress.md)

## 架构分层（依赖方向 app → {ui, runtime, canvas, terminal}；runtime → {canvas, terminal}）
- 域层（零 gpui，可独立 `cargo test`）：`canvas`（几何/节点/选择/命中/视口）、`terminal`（pty/grid/session/input）、`runtime`（焦点/路由/生命周期）
- 壳层（gpui/Metal）：`ui`（主题）、`app`（窗口/菜单/快捷键/画布与终端渲染）

## gpui 来源（R1 验证结论）
- 采用：crates.io =0.2.2
- 验证日期：2026-06-20
- 备注：API 与 brief 完全吻合，无需额外 patch。构建需 macOS Metal Toolchain（`xcrun metal --version` 可验证）。`cargo build -p app --release` 零错误通过（463 crates，0 errors，1 warning）。

## 开发命令
- 运行：`cargo run -p app`
- 域层单测：`cargo test -p canvas -p terminal -p runtime`
- 全量测试：`cargo test --workspace`
- 校验域层零 gpui：`cargo tree -p canvas -p terminal -p runtime | grep -i gpui`（应无输出）

## 打包（未签名）
- `cargo install cargo-packager --locked`
- `cargo build --release -p app`
- `cargo packager --release` → 产出 `Multisoul.app` / `.dmg`

## 快捷键
- `Cmd+N` 新建 Terminal；拖标题栏移动；单击选中；双击终端聚焦；`Esc` 退出聚焦
- 空格+拖 / 中键拖 = pan；滚轮 = zoom

## Phase 映射（占位，本切片未实现）
- Phase 2: undo/redo + 空间索引（`canvas/commands.rs`、`history.rs`）
- Phase 3: 多终端/resize/复制粘贴/mouse reporting
- Phase 4: snapshot 缓存/scrollback 虚拟化（`runtime/snapshot.rs`）
- Phase 5: 持久化 + 最近项目（新建 `workspace` crate）
- Phase 6: codesign/notarization
