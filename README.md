# Norma OS

Norma OS 是一个面向 vibe coding 的 macOS AI 伙伴：你给出想法与方向，Norma 陪你协调多个 Coding Agent，把终端、过程与产物留在同一张无限画布上，直到想法真正落地。

产品形态是 **Live Node on Infinite Canvas**：无限画布承载持续运行、随时可接管的工作节点，Terminal 是第一个 Live Node。

## 知识地图
- 系统架构：[`ARCHITECTURE.md`](ARCHITECTURE.md)
- Agent 短导航：[`AGENTS.md`](AGENTS.md)
- 展开工程手册：[`CLAUDE.md`](CLAUDE.md)
- 人类快速上手：本文
- 历史 SDD 施工记录：[`.superpowers/sdd/progress.md`](.superpowers/sdd/progress.md)

## 架构分层
- 域层（零 GPUI，可独立 `cargo test`）：`canvas`（几何/节点/选择/命中/视口）、`terminal`（PTY/grid/session/input）、`runtime`（焦点/路由/生命周期）、`storage`（SQLite）、`asr`（ASR 目录与安装验证）、`model_store`（通用本地模型安装原语）、`tts`（TTS 模型/语音目录与安装验证）
- 引擎适配：`voice_sherpa -> {asr, tts, dialogue, audio}`；`tts -> model_store`。
- 壳层（GPUI/Metal）：`ui`（主题）、`app`（窗口/菜单/快捷键、下载与设置编排、画布与卡片渲染）

## gpui 来源（R1 验证结论）
- 采用：crates.io =0.2.2
- 验证日期：2026-06-20
- 备注：API 与 brief 完全吻合，无需额外 patch。构建需 macOS Metal Toolchain（`xcrun metal --version` 可验证）。`cargo build -p app --release` 零错误通过（463 crates，0 errors，1 warning）。

## 开发命令
- 运行：`cargo run -p app`
- 自定义工作台壁纸：`MULTISOUL_WALLPAPER=/path/to/image.png cargo run -p app`（路径无效时回退到内置背景）
- 域层单测：`cargo test -p canvas -p terminal -p runtime -p storage -p asr -p model_store -p tts`
- 全量测试：`cargo test --workspace`
- 校验域层零 gpui：`cargo tree -p canvas -p terminal -p runtime | grep -i gpui`（应无输出）

## Quest 3 开发

Quest 3 原生客户端在 [`apps/quest/`](apps/quest/README.md)，与 Mac Rust workspace 共处一个 monorepo。
Agent 和 CLI 仍在 Mac 执行；Quest 工程包含空间面板、协议与独立的 mock 联调服务。

```sh
bash scripts/quest.sh setup
bash scripts/quest.sh mock
# 另一个终端：连接 Quest 后构建并启动
bash scripts/quest.sh run
# CI / 本地验证
bash scripts/quest.sh check
```

## macOS 打包
- `cargo install cargo-packager --locked`
- `cargo build --release -p app`
- `cargo packager --release -c Packager.toml` → 产出已签名但不公证的 `Norma OS.app` / `.dmg`
- `cargo packager --notarize --release -c Packager.toml` → 产出签名、公证并 staple 的发版包
- `./scripts/release-local.sh` → 产出已签名但不公证的 `Norma OS.app` / `.dmg`
- `./scripts/release-local.sh --notarize` → 产出签名、公证并 staple 的发版包

## 快捷键
- `Cmd+1`～`Cmd+5` 依次新建 Terminal、Claude、Codex、Cursor、OpenCode；`Cmd+D` 删除当前聚焦或唯一选中的卡片；`Cmd+Esc` 退出聚焦（裸 `Esc` 直达 PTY）
- 拖标题栏移动；单击选中；双击终端聚焦；空格+拖 / 中键拖 = pan
- 画布滚轮平移默认关闭，可在顶部工具栏开启；Terminal / Preview 卡片内部滚轮独立滚动

## Phase 映射（占位，本切片未实现）
- Phase 2: undo/redo + 空间索引（`canvas/commands.rs`、`history.rs`）
- Phase 3: 多终端/resize/复制粘贴/mouse reporting
- Phase 4: snapshot 缓存/scrollback 虚拟化（`runtime/snapshot.rs`）
- Phase 5: 持久化 + 最近项目（新建 `workspace` crate）
- Phase 6: codesign/notarization
