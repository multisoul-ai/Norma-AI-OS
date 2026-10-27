<p align="center">
  <img src="assets/app-icon-macos.png" alt="Norma OS 应用图标" width="96" />
</p>

<h1 align="center">Norma OS</h1>

<p align="center">
  <a href="README.md">English</a> · <strong>简体中文</strong>
</p>

<p align="center">
  <strong>会思考，会成长，陪你把事做成。</strong><br />
  Norma AI，你的贾维斯式 AI 伙伴。自进化、辅助决策，管理你的 Agent 团队。
</p>

<p align="center">
  <a href="https://github.com/multisoul-ai/norma-os-releases/releases/latest/download/Norma-OS_aarch64.dmg"><img src="https://img.shields.io/badge/下载_Mac_版-Apple_Silicon-D6FF3B?style=for-the-badge&amp;labelColor=111214" alt="下载 Mac 版 · Apple Silicon" /></a>
</p>

<p align="center">
  <a href="https://norma-os-desktop-site.vercel.app/zh-cn">官网</a> ·
  <a href="#norma-ai">Norma AI</a> ·
  <a href="#产品演示">产品演示</a> ·
  <a href="#快速开始">快速开始</a> ·
  <a href="ARCHITECTURE.md">架构</a> ·
  <a href="https://github.com/multisoul-ai/Norma-AI-OS/issues">反馈</a>
</p>

<p align="center">
  <a href="https://github.com/multisoul-ai/norma-os-releases/releases/latest"><img src="https://img.shields.io/github/v/release/multisoul-ai/norma-os-releases?label=release&amp;color=111214" alt="最新版本" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-111214" alt="MIT License" /></a>
  <img src="https://img.shields.io/badge/macOS-12%2B-111214" alt="macOS 12 或更高版本" />
</p>

<p align="center">
  <img src="docs/assets/readme/norma-os-cover-zh.png" alt="Norma 品牌人物：会思考，会成长，陪你把事做成。你的贾维斯式 AI 伙伴。" width="960" />
</p>

**Norma OS 的核心是 Norma AI**：一个与你持续相处、积累记忆、辅助决策，并管理多个 Coding Agent 的人工智能伙伴。你说出想法与方向，她理解上下文、组织执行、跟进进展，把需要你判断的事情和已经完成的结果带回来。

我们希望把**贾维斯式交互体验**带到你的 Mac：自然地交谈，在工作中获得建议，在关键时刻听到汇报，始终掌握方向。无限画布和 **Live Node** 是 Norma 与你的执行工作台，让每个 Agent、终端、网页和成果都有可见、可操作的位置。

## Norma AI

### 自进化：越相处，越懂你

Norma 的成长从记忆开始。她能记住你明确交代的偏好、项目约定与工作经验，在后续对话中按需调用；你可以纠正已有记忆，让接下来的协作更贴合你的习惯。个人、项目共享与工作树记忆分别管理，保存后可在重启后继续使用。

**自进化是 Norma AI 的核心建设方向**：从积累记忆，走向复盘经历、提炼方法与成长能力。当前已实现明确授权下的记忆读写、纠正及设置页管理；普通闲聊后的自动学习、Dream 复盘与 Skill 成长仍在规划中。这里的成长指记忆与工作方法的改进，不涉及基础模型权重训练。

### 辅助决策：看清进展，再决定下一步

Norma 能查询真实的项目与 Agent 状态，把分散的信息整理成你能行动的判断。后台**幕僚**结合任务目标、Agent 对话和执行结果，梳理进展、发现遗留事项，并给出下一步建议及理由。

你可以让她帮你判断先做什么、哪里还缺验证、哪些事情需要拍板。当前的后续任务提议会进入待处理事项，供你确认后派发；你也可以直接给出新的方向。

### 多 Agent 管理：你定方向，她组织执行

把 Codex、Claude Code、Cursor、OpenCode 等组织成你的 Agent 团队。通过自然语言创建 Agent、分配任务、续接已有会话、查询进度和打开成果；多个项目的状态也可以统一查看与协调。

一个 Agent 可以把明确的子任务交给另一个 Agent，并接收结果。Norma 帮你减少重复交代、手动转述和逐个检查，让你把注意力放在目标与判断上。

### 贾维斯式交互：开口交流，关键时刻有回应

通过文字或实时语音与同一个 Norma AI 交流：说出想法、追问进度、调整任务，或让她解释一个建议。启用幕僚与语音汇报后，她会汇总进度、在关键变化时主动汇报；你开口时，播报为对话让路。

你随时可以进入某个 Agent 的终端直接接手，也可以回到全局继续与 Norma 交谈。**自然交流、持续记忆、决策支持与团队协调，共同构成 Norma OS 的核心体验。**

可以从这些指令开始：

| 你可以这样说 | Norma 帮你做什么 |
| --- | --- |
| “记住，我希望你先给结论，再解释原因。” | 保存明确的交流偏好，供后续对话调用 |
| “现在谁还在忙？哪里需要我决定？” | 查看真实状态，整理进度与待处理事项 |
| “看看他们的结果，建议下一步先做什么。” | 交给幕僚分析，提出后续建议及理由 |
| “让 Codex 实现功能，让 Claude Code 补测试。” | 创建或选择 Agent，分工派发任务 |

### 你的执行工作台

Norma OS 用**无限画布上的 Live Node**承载真实工作：Agent 编写代码，Terminal 运行构建与测试，Browser 查看网页，Preview 阅读文件与产物。

在 App 运行期间，切换项目或焦点后，后台任务仍可继续。你可以平移、缩放、重排工作区，或放大一个节点专注处理；布局与会话检查点保存在本机，节点状态与通知帮助你找到需要介入的地方。

## 产品演示

以下素材来自官网的真实产品录屏。**点击图片观看完整视频**，也可以进入[官网演示区](https://norma-os-desktop-site.vercel.app/zh-cn#in-action)。录屏中的界面与快捷键可能随版本变化，当前默认键位见下文。

### 一句话，指挥六个 Agent

告诉 Norma 你的目标，她把语音指令转为任务派发，六个 Agent 在同一工作区分别执行，过程持续可见。

<p align="center">
  <a href="https://norma-os-desktop-site.vercel.app/media/demo-voice-command.mp4">
    <img src="docs/assets/readme/demo-voice-command.webp" alt="观看语音群控演示：一条语音指令向六个 Agent 派发任务" width="960" />
  </a>
</p>

<table>
  <tr>
    <td width="50%" valign="top">
      <h3>Agent 之间，直接协作</h3>
      <a href="https://norma-os-desktop-site.vercel.app/media/demo-agent-collaboration.mp4"><img src="docs/assets/readme/demo-agent-collaboration.webp" alt="观看 Agent 协作演示：派发子任务并接收结果" width="480" /></a>
      <p>把明确的子任务交给另一个 Agent，在原工作区接收结果。</p>
    </td>
    <td width="50%" valign="top">
      <h3>用键盘展开工作区</h3>
      <a href="https://norma-os-desktop-site.vercel.app/media/demo-live-node-shortcuts.mp4"><img src="docs/assets/readme/demo-live-node-shortcuts.webp" alt="观看键盘操作演示：创建终端和不同类型的 Agent Live Node" width="480" /></a>
      <p>快速创建终端与 Agent，把需要的工具带进同一画布。</p>
    </td>
  </tr>
  <tr>
    <td width="50%" valign="top">
      <h3>一个快捷键，重排布局</h3>
      <a href="https://norma-os-desktop-site.vercel.app/media/demo-layout-command-m.mp4"><img src="docs/assets/readme/demo-layout-command-m.webp" alt="观看布局演示：切换主从布局与平铺布局" width="480" /></a>
      <p>用 ⌘M 切换主从与平铺布局，在当前任务和工作全貌之间移动。</p>
    </td>
    <td width="50%" valign="top">
      <h3>知道何时轮到你</h3>
      <a href="https://norma-os-desktop-site.vercel.app/media/demo-agent-notifications.mp4"><img src="docs/assets/readme/demo-agent-notifications.webp" alt="观看状态通知演示：识别正在执行和已经完成的 Agent" width="480" /></a>
      <p>完成与注意力提示出现在对应节点上，找到变化，再接着推进。</p>
    </td>
  </tr>
</table>

## 工作方式

```text
你的想法与方向
      │  文字 / 实时语音
      ▼
Norma AI · 你的 AI 伙伴
      ├── 记忆          个人偏好、项目约定、用户纠正
      ├── 决策与幕僚    理解目标、分析进展、提出下一步
      ├── 团队协调      分工派发、跟进状态、汇总结果
      └── 执行工作台    无限画布上的 Live Node
            ├── Agent      Claude Code / Codex / Cursor / OpenCode …
            ├── Terminal   Shell、构建、测试、开发服务
            ├── Browser    网页与本地应用
            └── Preview    文件、文档与生成结果

执行进展与结果 → Norma 汇报与建议 → 你决定下一步
```

**工作发生在你的 Mac 上。** 项目文件、工作区布局和会话检查点在本机保存；Coding Agent 使用你自己的安装与账号。调用云端模型或语音服务时，相关请求仍会发往你配置的供应商。

## 快速开始

### 1. 安装 Norma OS

下载 [Apple Silicon 版 DMG](https://github.com/multisoul-ai/norma-os-releases/releases/latest/download/Norma-OS_aarch64.dmg)，打开后将 **Norma OS** 拖入 **Applications**，再启动应用。

当前分发包面向 **Apple 芯片 Mac，macOS 12 或更高版本**，已签名并经过公证。版本记录与校验文件见 [Releases](https://github.com/multisoul-ai/norma-os-releases/releases/latest)。

### 2. 准备你常用的 Coding Agent

安装并登录至少一种 Agent CLI，先确认它能在系统终端中正常运行。常用入口如下：

| Coding Agent | Norma 调用的命令 | 项目画布内的默认快捷键 |
| --- | --- | --- |
| Claude Code | `claude` | `⌘2` |
| Codex | `codex` | `⌘3` |
| Cursor Agent | `cursor-agent` | `⌘4` |
| OpenCode | `opencode` | `⌘5` |

也可以只从普通终端开始使用。更多 Agent 类型可在新建菜单中选择；模型、账号与服务费用由对应供应商提供和管理。

> 当前内置 Claude Code 与 Codex 启动配置使用自动执行权限，跳过各自的常规审批；请在你信任的项目和环境中使用。

### 3. 创建空间，打开项目

1. 创建一个工作空间（`⌘N`）。
2. 添加本地项目或克隆仓库（`⌘⇧A`），进入项目画布。
3. 新建终端或 Agent，直接输入任务；再按需添加 Browser 与 Preview 查看结果。

例如，让一个 Agent 实现功能，让另一个 Agent 编写测试，再在同一画布运行开发服务、打开页面检查结果。

### 4. 让 Norma AI 进入工作状态

在 **Settings** 中配置所需的模型服务，并按需启用以下能力：

- **记忆**：在 **Norma → 记忆** 中查看与编辑记忆，控制读取和保存；也可以直接告诉她“请记住……”。
- **语音**：在 Norma 的语音设置中选择交互模式、识别与输出供应商，按提示准备模型或凭据，并授予麦克风权限。
- **幕僚**：选择已安装的编码 Agent 后端，按提示准备适配器，启用进度摘要、下一步提议与语音汇报。

配置完成后，通过底部指令入口或 `⌘E` 开始交流：

> “Norma，创建两个 Agent，一个分析项目结构，另一个梳理测试入口。”

任务推进后，可以继续问：“现在进展怎么样？你建议下一步做什么？”

语音快捷键的按住、单次或持续聆听行为由 Settings 中的交互模式决定。

## 常用操作

以下为当前默认键位，可在 **Settings → Keyboard Shortcuts** 中录制与恢复。

| 操作 | 默认快捷键 / 手势 |
| --- | --- |
| 创建工作空间 / 添加项目 | `⌘N` / `⌘⇧A` |
| 新建终端 | `⌘1` |
| 新建 Claude Code / Codex / Cursor / OpenCode | `⌘2` / `⌘3` / `⌘4` / `⌘5` |
| 聚焦底部指令输入框 | `⌘K` |
| 语音交互 | `⌘E` |
| 切换主从布局 / 平铺重排 | `⌘M` / `⌘⇧L` |
| 打开文件预览 | `⌘⇧O` |
| 退出节点聚焦，回到画布 | `⌘Esc` |
| 打开设置 | `⌘,` |
| 移动节点 / 平移画布 | 拖动标题栏 / 空格拖动或中键拖动 |

完整的项目导航、想法派发与输入规则见[快捷键参考](docs/references/keyboard-shortcuts.md)。

## 从源码运行

需要 Apple Silicon Mac、Git LFS、[Rust / rustup](https://rustup.rs/) 和带 Metal 工具链的 Xcode。Rust 版本由仓库的 `rust-toolchain.toml` 固定。

开发启动脚本会创建 **Norma OS Dev.app**，需要钥匙串中有可用的 **Apple Development** 签名身份。可在 Xcode 中登录开发账号并创建开发证书；若只想体验产品，直接使用上面的 DMG。

```bash
git clone https://github.com/multisoul-ai/Norma-AI-OS.git
cd Norma-AI-OS
git lfs install
git lfs pull

# 确认 Metal 工具链与开发签名身份
xcrun metal --version
security find-identity -v -p codesigning

# 编译并启动真实 macOS 开发应用
cargo run -p app
```

常用开发检查：

```bash
cargo check -p app
cargo test -p canvas -p terminal -p runtime -p storage
cargo build -p app --release
```

完整测试、依赖边界与工程规范见 [CLAUDE.md](CLAUDE.md)；签名、公证和 DMG 构建见[打包指南](docs/references/packaging.md)。

## 技术与架构

Norma OS 的桌面端使用 **Rust + GPUI + Metal**。画布、终端、运行时和存储独立于 UI，界面负责呈现状态与执行交互。

| 能力 | 实现 |
| --- | --- |
| macOS 原生界面 | GPUI、gpui-component、Metal |
| 无限画布 | 几何、选择、命中测试与 viewport |
| 真实终端 | portable-pty、alacritty_terminal |
| Norma AI 与决策支持 | NormaSession、后台 brain、真实状态查询与下一步提议 |
| 多 Agent 协调 | typed tools、AgentCardAdapter、`norma` CLI |
| 长期记忆与持久化 | 分范围 Markdown 记忆、SQLite、工作区与会话检查点 |
| 语音输入与输出 | 共享音频采集、本地与云端 ASR、多个 TTS Provider |
| iOS 配套端 | SwiftUI，连接 Mac 查看状态与进行轻量控制 |

模块关系和核心流程见 [ARCHITECTURE.md](ARCHITECTURE.md)。iOS 配套端的构建与连接说明见 [ios/README.md](ios/README.md)，Coding Agent 与终端进程仍运行在 Mac 上。

## 文档与参与

| 想了解什么 | 从这里开始 |
| --- | --- |
| 文档总览 | [文档索引](docs/README.md) |
| 系统如何工作 | [系统架构](ARCHITECTURE.md) |
| Norma 的记忆与成长 | [已实现的记忆能力](docs/exec-plans/2026-10-03-norma-memory-access-and-settings.md) · [成长路线](docs/design-docs/2026-10-02-norma-master-memory-and-growth.md) |
| 后台决策与协作 | [幕僚设计](docs/design-docs/2026-10-23-norma-brain-acp.md) |
| 本地开发与测试 | [工程手册](CLAUDE.md) |
| 使用 Coding Agent 参与开发 | [Agent 导航](AGENTS.md) |
| 快捷键与输入 | [快捷键参考](docs/references/keyboard-shortcuts.md) |
| 品牌与视觉素材 | [品牌资料](docs/brand/README.md) |
| 构建与分发 | [打包指南](docs/references/packaging.md) |

欢迎通过 [GitHub Issues](https://github.com/multisoul-ai/Norma-AI-OS/issues) 报告问题或提出想法。反馈问题时，请附上 macOS 与 Norma OS 版本、复现步骤，以及去除敏感信息后的日志片段。

## 许可

[MIT](LICENSE) · Copyright © 2026 multisoul-ai。第三方组件与资源说明见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。
