# GitHub 项目克隆功能设计文档

> 状态：Phase 1 URL Clone 已实现并验证；Phase 2/3 待实施
> 范围：产品需求、用户流程、交互原型和技术方案概要，不包含实现代码
> 目标平台：macOS / Norma OS

## 1. 结论

该功能值得建设，但边界应严格限定为：

> 将远程 GitHub 仓库安全地下载到本地，并立即进入 Norma OS Workspace。

它不应在首期扩张为完整 Git 客户端。核心设计为两条并列路径：

1. **通过 URL 克隆**：始终可用，不要求登录 GitHub。
2. **从 GitHub 账号选择**：登录后搜索有权限访问的仓库。

GitHub 授权是“仓库发现与私有仓库认证能力”，不应成为公开仓库和 SSH 克隆的前置条件。

### 1.1 当前实施状态

截至 2026-08-01，Phase 1 已实现：Launchpad/File 入口、GitHub HTTPS/SSH URL、目标目录选择、后台系统 Git、进度与取消、安全清理、`.multisoul` 信任门、`WorkspaceSource::Clone`、Recent 和多窗口行为。

Phase 2 尚未实现：GitHub OAuth/GitHub App、账号仓库列表、Keychain 凭据桥接和私有 HTTPS 临时凭据。这部分需要先确定 GitHub App、回调协议及 Auth Broker 部署配置，不能由桌面端代码单独完成。

Phase 3 的 Enterprise、多账号、分支/浅克隆、Submodule/LFS 和后台任务中心仍按本文路线保留。

## 2. Norma OS 现状

现有架构已经预留了大部分接缝：

- Launchpad 已有禁用的 Clone Repo 入口：[launchpad/mod.rs](../crates/app/src/workspace/launchpad/mod.rs#L92)
- Workspace 数据模型已有 WorkspaceSource::Clone：[recent.rs](../crates/workspace/src/recent.rs#L7)
- Recent SQLite 已能编码、解码 clone 来源：[recent_store.rs](../crates/storage/src/recent_store.rs#L106)
- 目前 Workspace 打开流程仍硬编码为 WorkspaceSource::Open：[flow.rs](../crates/app/src/workspace/flow.rs#L263)
- File 菜单和快捷键目录目前只有 Open Project，没有 Clone：[menu_bar.rs](../crates/app/src/menu_bar.rs#L62)

因此无需重做项目管理系统，只需补齐：

~~~text
远端发现 → 授权/凭据 → 后台克隆 → 安全检查
       → Workspace 注册 → 打开项目窗口
~~~

## 3. 目标与非目标

### 3.1 产品目标

- 用户无需打开终端即可完成 GitHub 项目克隆。
- 公开仓库可以直接通过 URL 克隆。
- 用户可以连接 GitHub 账号并搜索自己有权访问的仓库。
- HTTPS 和 SSH 都是一等支持路径。
- 克隆任务具有明确、可取消、可恢复操作的状态反馈。
- 克隆成功后项目自动进入 Norma OS 的 Recent、持久化和窗口管理体系。
- 私有仓库凭据以及远端携带的工作区状态得到安全隔离。

### 3.2 非目标

首期不建设：

- Commit、Push、Pull、Fetch、Branch、Merge 等完整 Git 客户端能力
- GitHub Issues、Pull Requests 和 Actions 管理
- Fork 或 Create Repository
- GitHub Enterprise Server
- 多仓库批量克隆
- Sparse Checkout、Partial Clone、Shallow Clone
- 自动安装 Git、Git LFS 或 SSH Key
- 跨 App 重启继续未完成的克隆任务
- 自动执行仓库安装脚本、开发命令、Agent 或远端 Norma 会话

## 4. 竞品参考

| 产品 | 可借鉴点 | Norma OS 取舍 |
|---|---|---|
| VS Code | 从欢迎页、Source Control 或命令入口克隆；支持 GitHub 仓库搜索；需要时再登录；使用本机 Git | 采用“按需授权”和系统 Git，不要求先配置账号。[官方说明](https://code.visualstudio.com/docs/sourcecontrol/github) |
| GitHub Desktop | GitHub.com、Enterprise、URL 分页；仓库列表和本地路径在同一窗口完成 | 采用 Repository URL / GitHub 双标签；首期不支持 Enterprise。[官方流程](https://docs.github.com/en/desktop/adding-and-cloning-repositories/cloning-and-forking-repositories-from-github-desktop) |
| Cursor | 基于 VS Code Git 工作流；官方快速上手仍要求安装 Git | 不单独复制 Cursor 交互，以 VS Code 的成熟 Git 模型为基础。[官方快速上手](https://docs.cursor.com/en/get-started/quickstart) |
| Norma OS | 克隆后不是进入传统编辑器，而是恢复或创建空间 Workspace | 成功后直接进入 Canvas，Recent 中记录为克隆项目。 |

竞品共同模式是：

1. Clone 是打开项目之前的独立入口。
2. URL 克隆不依赖账号授权。
3. 账号授权主要解决私有仓库和仓库发现。
4. 仓库选择与本地路径确认在开始克隆前完成。
5. 克隆完成后立即进入产品的主工作界面。

## 5. 功能需求分析

### 5.1 P0：首期必须支持

#### 5.1.1 入口

- Launchpad 启用 Clone Repository。
- File → Clone Repository…。
- 从 Launchpad 发起时，成功后在当前窗口打开。
- 从已打开的 Workspace 发起时，成功后默认在新窗口打开，避免替换当前工作现场。
- 首期不设置默认快捷键。
- 未来新增快捷键时，必须进入 Settings → Keyboard Shortcuts，支持录制和恢复默认值。

#### 5.1.2 URL 克隆

接受并规范化以下形式：

~~~text
https://github.com/owner/repo
https://github.com/owner/repo.git
git@github.com:owner/repo.git
ssh://git@github.com/owner/repo.git
~~~

要求：

- 自动去除首尾空格和末尾斜杠。
- GitHub 网页地址自动推导为 Clone URL。
- 明确显示当前协议：HTTPS 或 SSH。
- 选择协议后在 HTTPS 和 SSH URL 间转换。
- 首次默认 HTTPS，此后记住最近使用的协议。
- 首期只接受 github.com。
- 拒绝本地路径、file://、未知协议、换行、NUL 和可能被解释为命令参数的输入。
- 校验失败时在输入框下方原地显示原因，不弹出额外对话框。

#### 5.1.3 GitHub 仓库列表

- 未登录状态显示 Sign in with GitHub。
- 登录后展示：
  - 用户拥有的仓库
  - 协作仓库
  - 有权限访问的组织仓库
  - 公开与私有仓库
  - Fork 和 Archived 状态
- 支持按 owner/name 和描述搜索。
- 默认按最近推送时间排序。
- 支持分页加载和手动刷新。
- 显示账号头像、登录名和 Disconnect。
- 未授权给 Norma 的仓库不应出现在列表中。
- 选择仓库后可切换 HTTPS 或 SSH。
- Archived 仓库允许克隆，但应展示只读提示。
- 仓库目录加载失败时保留登录状态和搜索输入，并提供 Retry。

GitHub API 已提供 clone_url、ssh_url、可见性、默认分支等字段，并支持按可见性、归属和推送时间查询。[GitHub REST 文档](https://docs.github.com/en/rest/repos/repos?apiVersion=latest)

#### 5.1.4 本地位置

- 显示完整目标路径，而不只显示父目录。
- 用户可选择父目录并修改最终文件夹名。
- 记住最近一次成功克隆的父目录。
- 目标路径必须满足：
  - 父目录存在且可写
  - 最终目录尚不存在
  - 文件夹名不为空
  - 文件夹名不包含斜杠
  - 文件夹名不是 . 或 ..
- 如果相同远端已经存在于 Recent：
  - 主操作：Open Existing
  - 次操作：Clone Another Copy
  - 自动建议 repo-2 等新名称
- 首期使用默认分支和完整历史。
- 首期不暴露深度、分支、Sparse Checkout 等高级参数。

#### 5.1.5 HTTPS

- 公开仓库无需登录。
- 私有仓库优先使用已连接 GitHub 账号的临时凭据。
- 没有账号凭据时允许系统 Git Credential Helper 接管。
- 认证失败后提供：
  - Sign in to GitHub
  - Try SSH
  - Retry
- 不允许用户输入 GitHub 账号密码。
- Personal Access Token 仅作为系统 Credential Helper 的既有能力，不在 Clone Sheet 中新增明文 Token 输入框。

#### 5.1.6 SSH

- 继承本机 ~/.ssh/config、SSH Agent、macOS Keychain 和 Git 配置。
- 支持密钥口令交互。
- 区分：
  - 没有 SSH Key
  - Permission denied (publickey)
  - Host Key 校验失败
  - 网络或端口阻断
- 错误页提供 Try HTTPS。
- 可在帮助中说明 GitHub 支持通过 ssh.github.com:443 处理部分网络限制，但首期不自动修改用户 SSH 配置。[GitHub SSH 443 文档](https://docs.github.com/en/authentication/troubleshooting-ssh/using-ssh-over-the-https-port)

#### 5.1.7 克隆任务

状态机：

~~~text
Idle
  → Validating
  → Authenticating（按需）
  → Preparing Destination
  → Connecting
  → Receiving Objects
  → Resolving Deltas
  → Checking Out
  → Securing Workspace
  → Registering Workspace
  → Opening
  → Completed / Failed / Cancelled
~~~

行为：

- 所有 Git、网络、磁盘操作在后台执行。
- GPUI 前台只接收状态事件并更新界面。
- 展示当前阶段、已接收对象和传输量。
- 能解析时展示百分比；无法解析时使用不确定进度。
- 支持 Cancel。
- 首期全应用同一时间只运行一个克隆任务。
- 再次发起克隆时聚焦现有进度。
- 取消后只清理本任务新建的目标目录。
- 如果无法安全确认目录归本任务所有，保留文件并提供 Reveal in Finder，不得冒险删除。
- App 退出或来源窗口关闭时，应明确询问是否停止克隆。
- Clone 子进程必须在取消、失败和 App 退出时完成 kill 和 wait，避免僵尸进程。

Git 自带 --progress，可强制向标准错误输出接收对象、解析 delta 等进度。[Git Clone 文档](https://git-scm.com/docs/git-clone.html)

#### 5.1.8 完成与项目管理

克隆成功后：

1. 验证目标是有效 Git Worktree。
2. 安全检查远端是否携带 Norma Workspace 状态。
3. 写入远端元数据。
4. 以 WorkspaceSource::Clone 注册 Recent。
5. 初始化本地 Norma 项目存储。
6. 打开 Workspace。
7. Recent 行显示 GitHub 来源、owner/repo 和本地路径。

如果 Git 克隆成功但 Workspace 初始化失败：

- 不重新克隆。
- 不删除本地仓库。
- 保留 Clone Sheet 的成功阶段信息。
- 提供 Retry Open、Reveal in Finder 和 Open as Folder。
- 错误文案明确区分“仓库已克隆”和“Norma Workspace 打开失败”。

### 5.2 错误与边界场景

| 场景 | 处理 |
|---|---|
| Git 未安装 | 阻止开始，提供安装说明和重新检测 |
| Git 版本过旧 | 显示检测到的版本和最低支持版本 |
| URL 无效或非 GitHub | 输入框下方即时校验 |
| 仓库不存在或无权限 | 不猜测私有仓库是否存在；提示检查地址或权限 |
| OAuth 过期或撤销 | 保留仓库和路径选择，重新授权后继续 |
| GitHub App 组织审批中 | 展示等待管理员批准状态 |
| 组织需要 SAML SSO | 显示组织授权提示，不清除用户输入 |
| SSH Key 缺失 | 提供 HTTPS 和 GitHub SSH 设置入口 |
| Host Key 校验失败 | 不自动接受未知指纹，提供安全排障说明 |
| 目标目录已存在 | 不覆盖，选择其他路径或打开已有项目 |
| 父目录不可写 | 原地提示并打开目录选择器 |
| 磁盘空间不足 | 保留错误详情，可安全清理时提供清理 |
| 网络断开、DNS、TLS、代理失败 | 保留表单并允许 Retry；继承 Git 代理配置 |
| 默认分支不存在 | 显示远端配置错误，不创建伪成功 Workspace |
| Git LFS 未安装 | 克隆可完成，但明确提示 LFS 文件可能仍是指针 |
| 仓库包含 Submodule | 首期不自动初始化，完成后提示 |
| 克隆完成但打开失败 | 保留仓库，单独重试 Workspace 初始化 |
| 已在另一个窗口打开 | 聚焦已有窗口，不重复注册 |
| 用户取消 | 终止并 wait 子进程，安全清理本任务新目录 |
| 清理失败 | 保留目标目录并 Reveal in Finder |
| App 在克隆中退出 | 询问停止；确认后终止、wait 并执行安全清理 |

GitHub 的“仓库不存在”可能同时表示地址错误或用户没有权限；组织 SAML SSO 也会影响 HTTPS 和 SSH 认证，错误映射不能武断。[GitHub 克隆错误文档](https://docs.github.com/en/repositories/creating-and-managing-repositories/troubleshooting-cloning-errors)

### 5.3 后续版本

- GitHub Enterprise Server
- 多 GitHub 账号
- 指定 Branch、Tag 或 Commit
- Shallow、Partial、Sparse Clone
- 自动初始化 Submodule
- Git LFS 安装管理
- 克隆任务后台中心和跨窗口查看
- App 重启后继续或恢复任务
- Fork 和 Create Repository
- Push、Pull、Commit、Branch 等完整 Git 能力

## 6. 用户交互流程

### 6.1 URL 流程

~~~text
点击 Clone Repository
  → 打开克隆 Sheet，聚焦 URL 输入框
  → 粘贴 URL
  → 实时解析 owner/repo 与协议
  → 选择 HTTPS/SSH
  → 确认本地路径
  → 点击 Clone
  → 路径与 Git 环境预检
  → 按需认证
  → 后台克隆并显示进度
  → 安全检查
  → 注册为 WorkspaceSource::Clone
  → 自动打开 Workspace
~~~

### 6.2 GitHub 账号流程

~~~text
点击 Clone Repository
  → 切换 GitHub 标签
  → 未登录：Sign in with GitHub
  → 系统浏览器完成授权
  → 返回 Norma OS
  → 加载有权访问的仓库
  → 搜索并选择仓库
  → 选择 HTTPS/SSH
  → 确认本地路径
  → Clone
  → 进度、安全检查、注册、打开
~~~

### 6.3 从不同入口打开

Launchpad：

~~~text
Clone 成功 → 当前 Launchpad 窗口进入 Workspace
~~~

已有 Workspace：

~~~text
File → Clone Repository…
  → Clone 成功
  → 新建项目窗口
  → 当前 Workspace 保持不变
~~~

### 6.4 失败恢复

~~~text
克隆失败
  → 保留 URL、仓库选择、协议和本地路径
  → 显示归类后的错误
  → 提供与错误直接相关的修复动作
  → Retry 从安全状态重新开始
~~~

不支持对半成品目录直接再次执行 Clone。Retry 前必须先安全清理本任务创建的目录，或要求用户选择新的目标路径。

## 7. 用户体验与 Apple 设计原则

本设计参考项目内 apple-design 和 emil-design-eng 原则：

- 使用附着当前窗口的 modal sheet，背景轻度压暗。
- 常用路径先展示，高级选项暂不暴露。
- URL 输入、路径选择和错误反馈原地完成，不使用连续弹窗。
- 点击或键盘按下立即出现视觉反馈。
- 进度切换只做轻微淡入或内容替换，不做庆祝动画。
- Escape 在未克隆时关闭；克隆中触发停止确认。
- 完整支持键盘 Tab、Return、Escape 和清晰焦点环。
- 减少动态效果开启时，仅使用短淡入淡出。
- 明暗主题、增加对比度和增加文字尺寸下保持可读。
- 控件和错误文案使用直接、具体的标签。
- 用户必须始终知道当前在哪里、正在做什么以及如何退出。

动效目的仅限：

- 表达 Sheet 与来源窗口的空间关系
- 提供按下和状态变化反馈
- 避免进度、错误、完成状态突然跳变

不为高频键盘操作增加动画，不让动效延迟输入响应。

## 8. 文本原型

### 8.1 Launchpad

~~~text
┌──────────────────────────────────────────────────────────┐
│ NORMA / WORKSPACE                                        │
│                                                          │
│ Pick up where your workspace is still alive.             │
│                                                          │
│  [ □  Open Project ]   [ ↓  Clone Repository ]           │
│                                                          │
│ Recent projects                             View all (4)  │
│ ┌──────────────────────────────────────────────────────┐ │
│ │ NO  norma-ai-os                         GitHub  ›     │ │
│ │     alan/norma-ai-os · ~/Developer/norma-ai-os       │ │
│ ├──────────────────────────────────────────────────────┤ │
│ │ AP  another-project                              ›   │ │
│ │     ~/Developer/another-project                      │ │
│ └──────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────┘
~~~

### 8.2 URL 克隆 Sheet

~~~text
┌──────────────────── Clone Repository ─────────────────────┐
│                                              [ × ]         │
│  [ Repository URL ]   [ GitHub ]                          │
│                                                           │
│  Repository URL                                          │
│  ┌─────────────────────────────────────────────────────┐  │
│  │ https://github.com/owner/repository.git             │  │
│  └─────────────────────────────────────────────────────┘  │
│  ✓ GitHub repository · owner/repository                   │
│                                                           │
│  Clone with       (●) HTTPS    ( ) SSH                    │
│                                                           │
│  Local destination                                        │
│  ┌──────────────────────────────────────────┐ [Choose…]   │
│  │ ~/Developer/repository                   │             │
│  └──────────────────────────────────────────┘             │
│                                                           │
│  This project will open in this window.                   │
│                                                           │
│                        [ Cancel ]  [ Clone Repository ]    │
└───────────────────────────────────────────────────────────┘
~~~

### 8.3 GitHub 未登录

~~~text
┌──────────────────── Clone Repository ─────────────────────┐
│  [ Repository URL ]   [ GitHub ]                          │
│                                                           │
│                     GitHub                                │
│                                                           │
│       Sign in to search repositories you can access.      │
│                                                           │
│                  [ Sign in with GitHub ]                  │
│                                                           │
│       Public repositories can still be cloned by URL.     │
│                                                           │
│                                             [ Cancel ]     │
└───────────────────────────────────────────────────────────┘
~~~

### 8.4 GitHub 仓库列表

~~~text
┌──────────────────── Clone Repository ─────────────────────┐
│  [ Repository URL ]   [ GitHub ]                          │
│                                                           │
│  ● alan                                     [Disconnect]  │
│  ┌─────────────────────────────────────────────────────┐  │
│  │ Search repositories…                               │  │
│  └─────────────────────────────────────────────────────┘  │
│                                                           │
│  ○ alan/norma-ai-os              Private · Updated today  │
│    Norma OS spatial workspace                             │
│  ● acme/internal-tools           Private · Organization   │
│  ○ alan/demo                     Public · Fork             │
│                                                           │
│  Clone with       (●) HTTPS    ( ) SSH                    │
│  Local path       ~/Developer/internal-tools [Choose…]    │
│                                                           │
│                        [ Cancel ]  [ Clone Repository ]    │
└───────────────────────────────────────────────────────────┘
~~~

### 8.5 克隆进度

~~~text
┌──────────────────── Cloning Repository ───────────────────┐
│                                                           │
│  acme/internal-tools                                      │
│  Receiving objects…                            62%         │
│  ████████████████████────────────                         │
│  184 MB of 296 MB                                         │
│                                                           │
│  ~/Developer/internal-tools                               │
│                                                           │
│                                              [ Cancel ]    │
└───────────────────────────────────────────────────────────┘
~~~

### 8.6 失败状态

~~~text
┌──────────────────── Clone Repository ─────────────────────┐
│                                                           │
│  SSH authentication failed                                │
│  GitHub did not accept any configured SSH key.            │
│                                                           │
│  Your repository and destination selections were kept.    │
│                                                           │
│  [Try HTTPS]  [Open SSH Setup Guide]  [Retry]             │
└───────────────────────────────────────────────────────────┘
~~~

### 8.7 克隆成功但 Workspace 打开失败

~~~text
┌──────────────────── Repository Cloned ────────────────────┐
│                                                           │
│  ✓ acme/internal-tools was cloned successfully.           │
│                                                           │
│  Norma OS could not initialize the workspace database.    │
│  The repository has not been deleted.                     │
│                                                           │
│  [Reveal in Finder]  [Open as Folder]  [Retry Open]       │
└───────────────────────────────────────────────────────────┘
~~~

## 9. 技术实现概要

### 9.1 Git 执行方式

首期推荐调用系统 Git，而不是直接引入 libgit2 或 gix：

- 继承用户已有 Git 配置、Credential Helper、SSH Agent、代理和证书。
- 与终端中的行为一致，排障成本较低。
- 更自然地兼容 Git LFS、SSH Config 和企业网络。
- 避免首期引入另一套凭据和 Git 配置语义。
- VS Code 同样依赖本机 Git。[VS Code Source Control 文档](https://code.visualstudio.com/docs/sourcecontrol/overview/)

执行要求：

- 不通过 shell 拼接命令。
- 使用独立参数并在远端前加入 --，防止 URL 被解释成选项。
- 子进程必须支持取消、kill 和 wait。
- stdout 和 stderr 在后台读取。
- 输出转换成 typed progress 和 typed error 事件。
- 原始 stderr 可进入脱敏诊断，但不能包含 Token、完整私有 URL 或凭据。
- 使用 --progress 获取非终端环境中的克隆进度。
- 继承用户的 Git 和 SSH 配置，但不主动修改全局配置。

### 9.2 模块边界

~~~text
app
 ├─ Clone Sheet / Progress / Navigation
 ├─ Keychain 与系统浏览器组合
 └─ 后台任务生命周期
        ↓
git capability（零 GPUI / 零 Storage）
 ├─ URL 解析与规范化
 ├─ Git 环境探测
 ├─ CloneRequest / CloneJob / CloneEvent
 ├─ Git 子进程、进度、取消
 └─ GitHub API 协议与错误映射
        ↓
storage
 ├─ GitHub 账号非敏感元数据
 ├─ opaque Keychain credential reference
 └─ workspace remote metadata
~~~

如果未来 GitHub 能力会扩展到 Issues、PR 等，可再拆独立 github crate；仅为 Clone 单点能力不建议现在过度拆分。

建议职责：

- workspace crate：远端来源和 Workspace 元数据类型。
- 新 Git 能力模块：URL、Git 探测、Clone Runner、进度和取消。
- app：GPUI、系统浏览器、Keychain、窗口和异步任务组合。
- storage：账号非敏感元数据、凭据引用、Workspace Remote 映射。
- ui：只在确有复用价值时新增通用组件，优先使用 gpui-component 的 Dialog、Input、Tabs、Button 和 Progress。

### 9.3 数据模型

建议新增以下领域对象。

~~~text
GitRemote
- provider
- repository_id
- owner
- name
- web_url
- https_url
- ssh_url
- default_branch
- visibility

CloneRequest
- remote
- transport
- destination_parent
- directory_name
- open_behavior

CloneJob
- id
- state
- progress
- created_target_path
- started_at
- failure

GitHubAccount
- login
- avatar_url
- credential_reference
- token_expiry
- granted_installations
~~~

Workspace Remote 元数据单独存入 App SQLite：

~~~text
canonical_path
provider
repository_id
owner
name
normalized_remote_url
transport
cloned_at
~~~

不要把远端信息硬塞进现有 Recent 序列化结构：

- Recent 负责“最近打开过什么”。
- Workspace Remote 负责“项目从哪里克隆”。
- WorkspaceSource::Clone 负责标记进入 Recent 的来源。

### 9.4 GitHub 授权

生产方案推荐使用 GitHub App：

- Repository Metadata：Read
- Repository Contents：Read
- 用户可选择允许 Norma 访问哪些仓库
- 使用短期 User Access Token
- Token 和 Refresh Token 仅存 macOS Keychain
- SQLite 只保存不可反查的凭据引用

GitHub 推荐优先考虑 GitHub App，因为权限更细、用户可控制仓库范围，并支持短期 Token；HTTP Git 访问需要 Contents 权限。

- [GitHub App 用户授权说明](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-a-user-access-token-for-a-github-app)
- [GitHub App 权限说明](https://docs.github.com/en/apps/creating-github-apps/registering-a-github-app/choosing-permissions-for-a-github-app)

推荐流程：

~~~text
Norma → 系统浏览器 → GitHub 授权
      → Norma Auth Broker 换取 Token
      → 一次性回调桌面 App
      → Token 写入 Keychain
~~~

建议使用轻量 Auth Broker，因为桌面应用无法真正保密 GitHub Client Secret。Broker 只负责：

- 保存 GitHub App Client Secret
- 验证 OAuth state 和 PKCE
- 与 GitHub 交换授权码
- 通过短时、单次、绑定当前授权会话的方式把结果交回桌面 App
- 不长期保存用户 Token
- 记录不包含仓库名、Token 或授权码的安全审计事件

如果一期明确不建设服务端，可临时使用 OAuth Device Flow：

- 不需要 Client Secret
- 适合快速完成原型
- 需要用户在浏览器输入设备码
- 存在更明显的远程钓鱼和应用冒充风险
- OAuth App 的 repo scope 较宽

GitHub 官方更建议有浏览器的原生客户端采用授权码 + PKCE；Device Flow 应作为过渡而不是最终目标。[OAuth 最佳实践](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/best-practices-for-creating-an-oauth-app)

### 9.5 凭据桥接

克隆时不得把 Token 写入：

- Clone URL
- 命令行参数
- Git Config
- SQLite
- 普通日志
- Debug 输出

应通过一次性的 AskPass 或 Credential 桥接向 Git 子进程提供：

- Git 请求凭据时才从 Keychain 读取
- Token 只进入受控进程输入
- 单次使用后清除内存副本
- 不把 Token 拼入 origin URL
- Clone 完成后的 origin 保持无凭据 HTTPS URL

SSH 口令同样通过受控 AskPass 处理，不记录用户输入。

### 9.6 进度和错误

Git Runner 将进程输出映射为稳定事件：

~~~text
CloneEvent::Connecting
CloneEvent::Receiving { received, total, bytes }
CloneEvent::Resolving { resolved, total }
CloneEvent::CheckingOut
CloneEvent::Completed
CloneEvent::Failed(CloneFailure)
~~~

错误类型不直接等同于 stderr 字符串：

~~~text
InvalidRemote
GitUnavailable
DestinationExists
DestinationNotWritable
AuthenticationRequired
PermissionDenied
RepositoryNotFoundOrDenied
SshKeyMissing
HostKeyVerificationFailed
NetworkUnavailable
TlsFailure
ProxyFailure
DiskFull
Cancelled
CloneSucceededWorkspaceFailed
Unknown
~~~

UI 使用 typed error 决定主操作，原始脱敏错误只放在 Details。

### 9.7 Workspace 接入

当前 enter_workspace_for_path 硬编码 WorkspaceSource::Open。实现时应让调用方传入来源：

~~~text
Open Project  → WorkspaceSource::Open
Clone success → WorkspaceSource::Clone
~~~

克隆成功后的顺序必须是：

1. Git 仓库验证。
2. 不受信任 Workspace 状态检查。
3. 本地排除规则处理。
4. Workspace Remote 元数据写入。
5. ProjectStore 初始化。
6. Recent 以 Clone 来源记录。
7. Runtime 和窗口切换。

如果第 4 步后失败，重试必须幂等，不产生重复 Recent 或 Remote 记录。

## 10. P0 安全阻断项：不信任远端 Norma 状态

当前项目数据库位于：

~~~text
<workspace>/.multisoul/project.sqlite
~~~

对应路径逻辑见 [database_paths.rs](../crates/storage/src/database_paths.rs#L72)。

这意味着恶意仓库可以提交伪造的 .multisoul/project.sqlite。如果克隆后直接调用现有 Workspace 恢复流程，可能加载攻击者控制的节点、Agent 定义或会话信息。

首期必须满足：

1. 新克隆仓库默认视为不受信任。
2. 打开前检查远端是否跟踪 .multisoul/**。
3. 如果存在，不得直接恢复其中的 Terminal、Agent 或 Session。
4. 提供 Open Without Repository Workspace State，使用全新的本地 Norma 状态。
5. 不自动运行安装脚本、开发命令、Agent 或仓库声明的任务。
6. 对正常仓库，将 .multisoul/ 幂等加入 .git/info/exclude，不修改仓库的 .gitignore。
7. 只有用户明确选择信任后，才允许导入远端 Norma 状态。
8. 建议首期完全不提供信任入口，只支持安全打开。

安全检查建议：

~~~text
git ls-files --error-unmatch -- .multisoul
~~~

如果远端跟踪该目录：

- 不删除远端文件。
- 不修改仓库历史。
- 不加载该数据库。
- 使用隔离的本地 Workspace Store，或阻止进入现有恢复路径，直到安全模式具备实现。

这是上线阻断项，不应作为后续优化。

## 11. 目录与清理安全

开始 Clone 前记录：

- canonical destination parent
- target 是否存在
- CloneJob ID
- 任务创建目标目录的时间点
- 进程 ID

首期要求目标目录不存在，原因是：

- 可以明确判断目录是否由当前任务创建。
- Cancel 和失败清理不会误删用户文件。
- 避免把远端内容混入已有目录。

清理前再次验证：

- 目标仍在原 canonical parent 下。
- 目标名与 CloneRequest 完全一致。
- 目标在任务开始前不存在。
- 目标没有被替换成指向其他位置的符号链接。

任何验证失败都不得递归删除，只能保留并 Reveal in Finder。

## 12. 性能、可靠性与隐私

### 12.1 性能

- Git 子进程、网络、目录扫描、凭据读取都不在 GPUI 前台执行。
- 进度事件节流到足以流畅反馈的频率，避免每行 stderr 都触发重绘。
- 仓库列表分页加载，搜索优先在已加载结果中即时执行。
- GitHub API 刷新使用请求代次 ID，丢弃过期异步结果。
- Clone Sheet 关闭或账号切换后，旧请求不得覆盖新状态。

### 12.2 可靠性

- 一个 CloneJob 具有唯一 ID。
- Cancel、失败、完成都是终态，不接受迟到进度。
- Workspace 注册步骤幂等。
- Recent 和 Remote 元数据写入失败不伪报整体成功。
- 克隆已成功但 Workspace 打开失败时不得删除仓库。
- App 退出必须结束子进程并 wait。

### 12.3 隐私

普通日志不得包含：

- GitHub Token、Refresh Token、授权码
- Authorization Header
- 带凭据的 URL
- 私有仓库描述或完整仓库列表
- SSH 私钥路径以外的密钥内容

建议日志字段：

~~~text
job_id
transport
public_or_private（若已知）
stage
duration_ms
failure_category
git_version
~~~

遥测如未来启用，应默认不记录 owner、repo、完整路径和 URL。

## 13. 验收标准

- 公开 HTTPS 仓库无需登录即可克隆并打开。
- 有本地 SSH Key 的用户可通过 SSH 克隆。
- 用户可登录 GitHub、搜索并选择获授权的私有或组织仓库。
- URL、协议和目标路径错误均在开始前原地提示。
- 克隆期间窗口保持响应。
- 进度持续更新且用户可取消。
- 取消不会删除用户原有目录。
- 认证失败后可切换 HTTPS 或 SSH，不丢失仓库和路径选择。
- 成功后 Recent 记录来源为 Clone。
- 成功后 Workspace Remote 元数据可查询。
- Launchpad 发起后在当前窗口打开。
- Workspace 内发起后在新窗口打开。
- 克隆成功但 Workspace 初始化失败时，本地仓库不会被删除。
- Token 不出现在 SQLite、远端 URL、命令行、日志或 Debug 输出中。
- 远端跟踪的 .multisoul 不会被自动加载或执行。
- 正常 Git 仓库的 .gitignore 不会被 Norma 修改。
- Norma 的 .multisoul 被加入本地 .git/info/exclude。
- 所有交互可通过键盘完成。
- 支持系统减少动态效果、增加对比度和明暗主题。

## 14. 自动化验证映射

后续实现必须把验收标准映射到以下证据。

### 14.1 L1：领域测试

- HTTPS、SSH、网页 URL 解析与规范化
- 非 GitHub、file://、换行、NUL 和参数注入拒绝
- 文件夹名和 canonical parent 校验
- CloneJob 状态机
- 迟到事件拒绝
- Git 进度解析
- stderr 到 typed error 映射
- 相同远端检测
- Cancel 清理所有权判断
- symlink 替换后拒绝删除
- .multisoul 跟踪检测
- 日志和 Debug 脱敏

### 14.2 L2：Headless GPUI E2E

使用 Fake Git、Fake GitHub API、Fake Keychain：

- Launchpad 点击 Clone Repository
- URL 输入、协议切换和路径选择
- 未登录 → 授权成功 → 仓库列表
- 搜索并选择私有组织仓库
- Clone → 进度 → 成功打开
- HTTPS 失败 → 切换 SSH → 成功
- Cancel → 安全清理
- Clone 成功但 Workspace 初始化失败
- 远端 .multisoul 存在时安全阻断
- Recent 记录为 Clone，未选择项目不进入 Recent
- 账号断开不泄露或残留 Token

### 14.3 L3：真实 App

页面、布局、样式和视觉状态必须通过 Computer Use 操作真实 App：

- Launchpad 新入口
- Clone Sheet 键盘焦点
- Repository URL 和 GitHub 标签切换
- 进度、错误、成功状态
- 明暗主题
- 减少动态效果
- Launchpad 当前窗口打开
- 已有 Workspace 时新窗口打开

### 14.4 Ignored L3

- 真实公开仓库 HTTPS Clone
- 真实公开仓库 SSH Clone
- 真实 GitHub 授权和私有仓库 Clone
- SSO 或组织审批场景按可用环境单独验证

真实凭据测试不得进入默认 CI，凭据来自 Keychain 或未纳入版本控制的安全环境。

## 15. 分阶段建议

### Phase 1：URL Clone Vertical Slice

- 启用 Launchpad 和 File 菜单入口
- URL 解析与 HTTPS/SSH
- 系统 Git 探测
- 路径选择
- 后台 Clone、进度、取消
- 安全目录清理
- .multisoul 安全门
- WorkspaceSource::Clone 接入
- Recent 和新窗口行为

### Phase 2：GitHub 账号目录

- GitHub App 注册
- Auth Broker
- 系统浏览器授权
- Keychain Token
- 仓库列表、搜索、分页、组织仓库
- 私有 HTTPS Clone
- 账号断开和 Token 刷新

### Phase 3：增强能力

- GitHub Enterprise
- 多账号
- Branch、Shallow、Submodule、LFS
- 后台任务中心
- 更丰富的 Workspace Remote 管理

## 16. 关键产品与技术决策

本设计默认：

1. 首期只支持 GitHub.com，不支持 GitHub Enterprise。
2. URL Clone 无需 GitHub 登录。
3. 首期克隆默认分支和完整历史。
4. 首期不自动初始化 Submodule。
5. 首期调用系统 Git。
6. 生产授权目标为 GitHub App + 轻量 Auth Broker。
7. Device Flow 只作为无服务端过渡方案。
8. 新克隆仓库默认不信任远端 Norma Workspace 状态。
9. 首期不提供导入远端 .multisoul 的信任入口。
10. 从已有 Workspace 发起时默认在新窗口打开。

这些决策在进入实现前应完成评审；其中凭据安全、目录清理和远端 .multisoul 信任边界属于上线阻断项。
