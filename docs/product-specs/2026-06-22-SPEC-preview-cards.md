# Preview Cards SPEC

## 1. 背景与目标

Multisoul Desktop 的核心形态是无限画布上的 Live Node。当前已有 Terminal / Agent 卡片，但 agent 生成的交付物、Markdown 文档、图片等结果还不能作为独立产物留在画布中。

本功能目标是先实现“预览卡片”本体：用户可以从本地文件创建一个 Preview Card，卡片作为一等 canvas node 显示、参与现有平铺/主从布局、可持久化恢复，并支持 Markdown/文本与图片预览。CLI/agent 推送暂不实现，只预留内部 API。

## 2. 范围

### 2.1 In Scope

- 新增 `Preview` 画布节点类型。
- 新增 `PreviewCard` renderer，并接入现有 `card_for(NodeType)` registry。
- 临时工具栏或菜单入口：选择本地文件并创建预览卡片。
- 预览 payload 以本地文件路径为主，卡片读取文件内容。
- 支持显式指定 preview type；未指定时按扩展名/MIME 推断。
- 首批 renderer：
  - Markdown / 文本。
  - 图片。
- Markdown 引入 Markdown 库，尽量完整渲染常见 Markdown。
- 图片按自然尺寸显示，超出内容区时滚动，不裁剪、不强制铺满。
- 卡片元数据和文件路径持久化到 canvas snapshot；重开 workspace 后重新读取文件。
- 读取文件异步执行，加载中显示 loading 状态。
- 标题栏/操作区提供刷新按钮，点击后重新读取文件。
- 不支持的文件类型显示错误态。

### 2.2 Out of Scope

- 不做 agent 自动推送卡片。
- 不实现公开 CLI 命令。
- 不做自动文件监听或自动刷新。
- 不持久化文件内容快照。
- 不做 inline payload。
- 不做 PDF、视频、音频、HTML、代码高亮等高级预览。
- 不为不支持类型显示通用文件信息。
- 不修改 `canvas/terminal/runtime/storage` 的依赖方向约束。

## 3. 用户与使用场景

典型用户是 Multisoul Desktop 早期使用者或开发者，在 workspace 中查看 agent 或人工生成的本地文件。

关键场景：

- 用户打开 workspace，点击工具栏/菜单的 Preview File，选择一个 `.md` 文件，画布中出现预览卡片。
- 用户选择图片文件，画布中出现图片预览；图片大于卡片内容区时可滚动查看。
- 用户修改源文件后，点击预览卡片刷新按钮，卡片进入 loading，然后显示新内容。
- 用户关闭再打开 workspace，预览卡片位置和路径恢复，并重新读取当前文件内容。
- 用户选择不支持类型，卡片仍创建，但内容区显示不支持错误。

## 4. 业务流程与信息架构

创建流程：

1. 用户在 Canvas 状态点击 Preview File 入口。
2. App 打开文件选择器。
3. 用户选择本地文件，获得绝对路径。
4. App 根据当前可视区域创建 `Preview` node，接入现有平铺/主从布局。
5. App 异步读取文件，进入 `Loading` 状态。
6. 类型识别成功后渲染 Markdown/文本或图片；失败时显示错误态。
7. Canvas snapshot 延迟持久化 Preview node 元数据。

刷新流程：

1. 用户点击预览卡片刷新按钮。
2. App 对该卡片重新触发异步加载。
3. 加载期间保留卡片位置与 chrome，内容区显示 loading。
4. 加载成功后替换内容；失败后显示错误态。

## 5. 数据模型与接口

新增 canvas 域模型：

```rust
pub struct PreviewSpec {
    pub path: String,
    pub explicit_type: Option<PreviewType>,
}

pub enum PreviewType {
    Markdown,
    Text,
    Image,
}
```

新增节点类型：

```rust
NodeKind::Preview(PreviewSpec)
NodeType::Preview
```

App 层运行时状态，不进入 canvas snapshot：

```rust
enum PreviewLoadState {
    Idle,
    Loading,
    Ready(PreviewContent),
    Error(PreviewError),
}

enum PreviewContent {
    Markdown(String),
    Text(String),
    Image { path: PathBuf },
}
```

内部 API 预留：

```rust
spawn_preview_from_path(path: PathBuf, explicit_type: Option<PreviewType>, bounds: CanvasRect)
```

该 API 供后续 CLI/agent 推送复用；MVP 只由文件选择入口调用。

持久化：

- `DocumentSnapshot` 保存 `NodeKind::Preview(PreviewSpec)`、bounds、z_index。
- 不写入 `session_checkpoints`，因为 Preview 不是 live PTY session。
- 恢复 workspace 时，Preview node 进入 loading，并根据路径重新读取文件。

## 6. 技术实现概览

`crates/canvas`：

- 扩展 `PreviewSpec`、`PreviewType`、`NodeKind`、`NodeType`。
- 保持零 GPUI、可独立 `cargo test -p canvas`。
- 增加 node type/title/serde snapshot 测试。

`crates/runtime`：

- 新增无 live session 的 preview spawn 方法，或通过 app 层直接向 `document.add_node` 添加 Preview node。
- `node_caps` 对 Preview 返回 `Caps::NONE`，即不接收键盘输入、不需要 terminal resize。
- 不引入文件读取或 GPUI 依赖。

`crates/app`：

- 新增 `cards/preview/`。
- `PreviewCard.resize()` 返回 `None`，让卡片自然参与现有 layout，但不触发 runtime resize。
- 新增 preview load/cache 状态，按 node id 管理。
- 文件读取在 async task 中执行，避免阻塞 UI。
- Markdown renderer 使用新增 Markdown dependency，把 Markdown 结构映射为 GPUI 元素。
- 图片 renderer 使用文件路径作为 image source，自然尺寸渲染，内容区支持滚动。
- 标题栏显示文件名或 `Preview`，操作区提供刷新按钮和现有关闭按钮。
- 临时工具栏/菜单入口打开文件 picker 并创建 Preview node。

## 7. UI/UX 需求

- Preview Card 使用现有节点 chrome：可选中、可拖拽、可关闭、参与布局。
- 标题优先显示文件名；可在副信息中显示推断类型。
- 内容区状态：
  - Loading：显示加载中。
  - Ready Markdown/Text：显示排版后的 Markdown 或文本内容。
  - Ready Image：自然尺寸显示，超出可滚动。
  - Error：显示清晰错误，如文件不存在、读取失败、不支持类型。
- 刷新按钮放在标题栏右侧/操作区，点击后异步重读文件。
- 不做自动刷新，避免后台文件监听复杂度和不可见状态变化。

## 8. 状态、错误与边界情况

- 文件不存在：显示错误态，保留卡片。
- 无权限读取：显示错误态，保留卡片。
- 路径不是文件：显示错误态。
- 类型无法识别或不支持：显示不支持错误。
- Markdown 解析失败：退化为错误态或纯文本显示，实施时需统一。
- 图片解码失败：显示错误态。
- 重开 workspace 后文件内容变化：显示当前文件内容，而不是创建时快照。
- 刷新期间旧内容是否保留由实现决定，但必须明确显示 loading 或 refreshing 状态。

## 9. 非功能性需求

- 文件读取不得阻塞 GPUI 主线程。
- 不在 `Runtime::drain_all`、PTY output chunk、`canvas_view::on_mouse_move`、`runtime::route`、`send_node_input` 中同步读写 SQLite 或阻塞查库。
- Preview metadata 通过现有 canvas snapshot 延迟持久化。
- 新依赖应审慎：Markdown 库允许新增；文件监听依赖不新增。
- 图片和 Markdown 渲染应避免每帧重新解析。
- 任意绝对路径允许读取，但不持久化内容副本。

## 10. 风险、权衡与未决问题

已做取舍：

- 先做预览卡片本体，不做 agent/CLI 推送，降低第一版集成面。
- 只持久化路径，不持久化内容，避免数据库膨胀，但文件变化会影响恢复显示。
- 不做自动刷新，改为刷新按钮，减少后台监听和性能风险。
- Preview 不作为 live node，简化 runtime 生命周期。

风险：

- Markdown 完整渲染可能比预期复杂，尤其表格、HTML、代码块样式。
- GPUI 滚动容器能力需要实施时验证；图片自然尺寸滚动依赖该能力。
- 任意绝对路径可能暴露本地隐私路径，UI 和日志应避免输出过多敏感内容。

未决：

- Markdown 解析失败时是错误态还是纯文本 fallback。
- 是否设置文件大小上限。
- 刷新 loading 期间是否保留旧内容。

## 11. 验收标准

- 画布中可以通过临时工具栏/菜单入口选择文件并创建 Preview Card。
- Preview Card 是 canvas node，支持选中、拖拽、关闭、平铺布局、主从布局。
- `.md` 文件能以 Markdown 方式渲染。
- 普通文本文件能显示文本内容。
- 图片文件能自然尺寸显示，超出内容区可滚动。
- 不支持类型显示错误态。
- 文件读取期间显示 loading 状态，且 UI 不阻塞。
- 点击刷新按钮会重新读取文件并更新内容。
- 保存并重开 workspace 后，Preview Card 恢复位置和路径，并重新读取当前文件。
- Preview node 不创建 session checkpoint。
- CLI/agent 后续可复用内部 `spawn_preview_from_path` 入口。
- 对应 crate 验证通过：
  - `cargo test -p canvas`
  - `cargo test -p runtime`
  - `cargo check -p app`
  - `bash scripts/check-storage-boundaries.sh`
  - `bash scripts/check-max-file-lines.sh`
  - `bash scripts/check-no-allow.sh`
