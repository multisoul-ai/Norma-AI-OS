# Goal：桌面统一场景、GPU 合成与连续画布重构

**项目区域布局修订（2026-10-02，P1–P3 已实施、待真实 App 验收）**：按[项目区域紧凑布局实施计划](2026-10-02-desktop-project-canvas-compact-layout.md)执行：两行规则仅作用于 Workspace 内的项目区域，1 个项目占左上 1/4、2 个占上半区、3／4 个补齐 2×2，5 个起扩列并保持一屏容纳。Agent 卡片的排序、排版、布局菜单和 PTY 生命周期不变；按项目数量计算，与内部 Agent 数量无关。此前误写的 Agent 内部布局及 PTY 改造计划已撤销。同步项目总览、真实缩略图和镜头锚点；只影响桌面端，其余 Goal 状态及验收分工保持原样。

状态：ACTIVE。2026-10-02 已将 `origin/dev@55ca6101` 合并为 `549eca74`，并恢复 Browser 原生视图场景接线、镜头起点、同级返回及项目直达目录改动。最终代码全仓测试 1735 passed / 31 ignored；恢复后全量 App 测试 883 passed / 7 ignored，GPUI 原生场景测试 1 passed；默认／Chromium 检查及 Release 构建通过。Computer Use 连接既有隔离 App 仍超时，未取得本轮真实窗口验收证据。跨工作空间直接转向、工作空间展开途中选择项目、Browser 真机交互及 S1–S4 性能仍待完成，G0–G5 均未通过。详见[本轮整合记录](2026-10-02-desktop-dev-merge.md)。

**dev 整合决策（2026-10-02，用户确认）**：采用 dev 的紧凑 Command Dock，保留当前工作空间的项目编号和当前窗口导航；项目命令建议与键盘输入由对应项目持有。Agent 卡片材质采用 dev 的半透明渐变，移除多层壁纸采样，保留壁纸、圆角与描边。命令 Dock 用于项目指令，不新增跨工作空间派工入口。完成后普通推送并创建目标为 `dev` 的 PR；**不得合并 PR**。本修订覆盖下文关于保留旧 Dock/采样实现的历史描述。

**验收分工修订（2026-10-01，最新用户指令）**：从 Agent 剩余任务中移除“独立原型还原验收”。原型的布局、材质和动效还原由用户亲自验收，不再安排 UI 验收 subagent，也不由 Agent 代判还原通过。23／24 号参考继续作为实施依据；Agent 负责功能开发、Computer Use 真实 App 功能／交互回归、性能测量及问题修复。用户验收反馈另行记录，未收到结论时注明“待用户验收”，其余开发与测试继续推进。本修订覆盖下文历史记录和旧 Goal 提示词中的独立 UI 审查要求。

**提交与推送授权（2026-10-02，最新用户指令）**：用户要求“完成后提交代码，并且push”。完成本次重构实现与规定工程验证后，提交本 Goal 相关代码、测试和文档，并普通推送至 `origin/codex/desktop-multi-project-workspaces`；提交、推送无需再次请求确认。本授权覆盖旧 Goal 提示词中的“不要自动提交、推送”。提交前核对变更范围并通过仓库 hooks，推送后核对远端提交；不以提前提交替代剩余开发或验收，原型验收仍由用户负责。

**用户修订（2026-10-01，外观仍待实施与验收）**：工作空间与项目总览窗口以 [24 号用户参考图](../design-docs/2026-09-29-desktop-workbench-concepts/24-overview-glass-windows-reference.png)作为最新外观基准；创建 Agent 直接使用所属项目区域已选定的目录，不再弹出 Git worktree 选择或确认。Agent 默认目录已完成代码修复、16 项桌面单测及三种入口的真实 App/cwd 检查，详见 G1 observations；其他外观和统一场景要求仍未完成。

**交互修订（2026-09-30）**：按用户指令移除桌面顶部整条导航横栏及其占位；保留原 Canvas Dock。新增六个可配置导航快捷键，定义见 `docs/references/keyboard-shortcuts.md`。采用 Apple 内容优先原则，将导航交给直接操作与键盘；切换顺序稳定，焦点留在当前窗口。

**测试方式修订（2026-09-30）**：按用户最新指令停用 `crates/e2e`，不再新增、维护或执行旧套件，历史结果不计入验收。改用域层/GPUI 单元测试、真实 PTY 集成测试和 Computer Use 操作隔离真实 App。现行验收包含 G0–G5、USC-01–14（USC-14 为 2026-10-01 新增的创建命名要求）；原型还原和性能门槛继续有效。

执行 Issue：[GitHub #73](https://github.com/multisoul-ai/Norma-AI-OS/issues/73)，标签 `ready-for-agent`，含此前发布的计划副本；2026-10-01 修订以本地文档为准，本次未同步远端 Issue。

- 实施 worktree：`/Users/alan/Documents/mybook/norma-ai-os-desktop-workspaces`。
- 分支：`codex/desktop-multi-project-workspaces`；dev 合并提交：`549eca7480f2acd8988e7fff5763cef6fc6a613b`；Browser 场景与导航恢复改动纳入本轮整合提交。旧 e2e 与本地模型软链接保持本地未提交状态，原始 stash 与备份分支保留。
- 产品规格：[统一场景 SPEC](../product-specs/2026-09-30-SPEC-desktop-unified-scene.md)；归属规则：[桌面工作空间 SPEC](../product-specs/2026-09-29-SPEC-desktop-multi-project-workspaces.md)。
- 动效与空间关系基准：[23 号 HTML 原型](../design-docs/2026-09-29-desktop-workbench-concepts/23-workspace-canvas-motion-prototype.html)；窗口材质与外观基准：[24 号用户参考图](../design-docs/2026-09-29-desktop-workbench-concepts/24-overview-glass-windows-reference.png)。外观冲突按最新用户参考执行，连续动画要求仍然有效。
- 被替代的计划：[旧画布重构计划归档](archive/2026-09-30-desktop-workspace-canvas-rebuild.md)。它的未通过项继续追踪，不继承任何“已完成”结论。

## 1. Goal 与完成边界

在上述 worktree 重构 macOS 桌面工作台，使总览、工作空间、项目展开和真实卡片共享稳定场景及坐标；按原型实现可中断的连续镜头与局部重排，并完成真实 App 的视觉、输入、会话和性能端到端验收。

允许较大范围调整 `app`、`canvas` 及仓库维护的 GPUI 补丁。业务事实源、工作空间／项目／卡片归属、Runtime／PTY 生命周期、现有卡片绘制能力继续复用。只影响桌面端，不改变 iOS 或手机协议。

归档文件不是施工入口。此计划是后续 Goal 的执行与实时状态入口；每完成阶段更新状态、证据路径、剩余风险。检查项必须有证据后才能勾选。

## 2. 当前证据与问题优先级

| 优先级 | 已核实位置 | 当前事实与处理方向 |
| --- | --- | --- |
| P0 | `crates/app/src/workspace/desktop/project_world.rs` | 动画逐帧改变真实项目视图的 left/top/width/height；改为稳定内容布局与呈现变换分离 |
| P0 | `vendor/gpui-0.2.2/src/view.rs` 的 ViewCacheKey／prepaint | 缓存要求 bounds、content mask、text style 一致；移动容器会导致复用失效。验证局部坐标缓存与变换重放 |
| P0 | `crates/app/src/workspace/desktop/preview_canvas.rs`、`view.rs` | 缩略、总览与展开经过不同显示路径；统一真实内容渲染入口及稳定身份 |
| P0 | `crates/app/src/workspace/desktop/scene.rs` 的 defer／finish navigation | 跨层导航存在先收回再执行的队列；替换为当前画面直接转向新目标 |
| P1 | `workspace/desktop/{project_area,workspace_card,launchpad}.rs` | 总览返回时外壳缺失已局部修复；仍须按 24 号参考统一两级窗口材质、内容比例与选中反馈，不能以终态边框恢复代替完整还原 |
| P1 | `workspace/desktop/agent.rs`、`dialogs_other.rs`、`app/agent_actions.rs` | 已删除 worktree chooser；各入口直接使用目标区域目录，保留后台校验与异步身份保护。单测及菜单／快捷键／Dock 的真实 cwd 检查通过；完整 G4 回归待继续 |
| P1 | `vendor/gpui-0.2.2/src/scene.rs`、`window.rs`、`platform/mac/metal_renderer.rs` | 已有绘制重放与 Metal backend，但任意子树缩放／离屏纹理不是现成公共能力；先验证再决定补丁范围 |
| P1 | `crates/app/src/frame_diagnostics.rs`、`motion/diagnostics.rs` | 已有可选诊断；扩展必要的帧、缓存和布局计数，避免新建诊断系统 |

原型实际是 `world` 的 translate3d + scale，加项目和卡片内部 CSS 几何过渡；不能把“统一场景”误解为永远不改变局部布局。性能收益尚未实测，上表不构成掉帧根因的完整 profile。

### 接手基线（历史结果，不是新架构验收）

- 桌面专项：30 passed、7 ignored，日志 `/tmp/norma-scene-regression6.log`。
- 完整串行 E2E：252 passed、3 failed、60 ignored，日志 `/tmp/norma-scene-full-e2e.log`。
- 首个失败为 `e2e_02_workspace_open_never_reuses_latest_session_for_multiple_unbound_agent_cards`：2 秒内没有收到 fake Claude 输出；后两项因共享锁中毒失败。单独运行曾通过，不能据此称完整回归通过。
- Release 编译已成功，日志 `/tmp/norma-scene-release.log`；隔离 App 中二进制尚未更新到该次构建，无对应最新真实 UI 验收。
- 开始新 Goal 时复制可用日志到持久证据目录；若临时文件已消失，注明缺失并重建基线，不伪造结果。

## 3. 设计决定与施工范围

### 场景结构

```text
固定屏幕层：壁纸／Canvas Dock／菜单
场景视口：镜头平移 + 等比缩放 + 裁剪
  桌面工作空间（稳定 ID）
    项目区域（工作空间 ID + 项目身份）
      真实卡片（区域身份 + 节点 ID）
```

这是一份显示结构，不另建业务会话注册表。允许视口外剔除、丢弃绘制缓存；稳定身份不要求所有卡片一直执行 render。

### 边界与复用

| 职责 | 现有入口／允许修改范围 |
| --- | --- |
| 几何、逻辑布局与变换 | `crates/canvas/src/`；先复用现有 CanvasRect、视口和命中能力 |
| 工作空间导航与呈现 | `crates/app/src/workspace/desktop/`；统一 camera、scene、launchpad、project world 和 preview 的职责 |
| 卡片内容与终端绘制 | `crates/app/src/cards/`、`crates/app/src/canvas/view.rs`；复用现有 chrome、终端缓存和卡片能力 |
| GPU／绘制缓存／平台输入 | `vendor/gpui-0.2.2/`；在 `NORMA_PATCH.md` 记录新增补丁及测试 |
| 固定 UI／壁纸 | `crates/app/src/app/chrome.rs`、`chrome/dock.rs`、`video_wallpaper.rs`；保留当前入口与低拷贝路径 |
| 用户流程验证 | 隔离真实 App＋Computer Use；底层算法与协议由受影响 crate 单元/集成测试覆盖 |
| 性能观测 | `crates/app/src/frame_diagnostics.rs`、`motion/diagnostics.rs`、现有性能采样脚本 |

若发现代码已变化，先核对事实并更新计划中的位置。保留他人未提交改动；不重置 worktree、不恢复旧版本覆盖新功能。

### 必须先解决的技术关卡

1. GPUI 内容按局部坐标布局，合成变换更新时可以复用内容；裁剪、圆角、阴影、glyph、image 和低拷贝 surface 均正确。
2. 画面变换与命中逆变换共享同一来源，包含事件坐标、滚动、IME、菜单和可访问性边界。
3. 单纯镜头移动不改变 PTY 行列；真实布局改变时，网格 resize 与内容显示有一致提交点，无末帧闪跳。
4. 先验证 Browser 原生视图的过渡与交互，再扩展全工作台；不得把 Browser 留到最后才发现无法合成。
5. 绘制指令复用先行；仅在 profile 显示仍需降低合成成本时添加有限的 GPU 纹理缓存。确认缓存透明内容不会固定旧壁纸。

## 4. 动效与交互合同

- 镜头临界阻尼：角频率 `15`，阻尼系数 `30 = 2 × 15`；平移与缩放使用同一个时间源，转向保留当前位置和速度。
- 原型卡片重排：`330ms cubic-bezier(0.2, 0.8, 0.2, 1)`；项目几何：`350ms ease-out`；非目标区域透明度：`180–200ms ease-out`。使用合成变换与适当裁剪重现，不逐帧推动整棵布局树。
- 以起止布局进行视觉插值／变换；比例变化下文字不得被拉伸到不可读，终点不能换一套卡片。实现后比较实际轨迹和转向行为，不能只对比终点截图。
- 从实际可见区域出发，包括搜索、滚动、不同窗口尺寸与冷启动；缺失布局时先建立正确布局，不用居中猜测框冒充来源。
- 工作空间、项目直接切换，以及项目直接返回目录，均可中途改变目标；不等待旧动画收尾，不排队播放过期目标。
- 点击或切换工作空间落在项目总览，保留总览滚动位置，不自动展开内存或磁盘记录的历史项目。只有显式项目点击／导航才展开；工作空间动画落定不追加项目动画（2026-10-01 用户反馈修订）。
- 展开后去掉外层边框、重复标题和说明；保留真实卡片 chrome、半透明材质、壁纸及原有 Dock；不恢复额外 Norma 输入栏。
- 工作空间与项目总览窗口统一采用 24 号参考的轻薄深色半透明底板、细描边、较小圆角和克制的选中描边，自动网格排列；空窗口保留同样外壳。内容是真实 Agent／Terminal／Browser 画布整体缩小，标题和状态简洁，不用多行说明挤压内容。
- 展开时外壳随镜头淡出，返回时边框、底板与内容连续恢复；不得出现仅剩卡片浮在壁纸上、末帧突然补框、背景压住内容或按钮未随缩略比例变化。共用用户当前壁纸，参考图不指定地球壁纸；不引入视频人物、外部桌面或演示统一输入栏。
- 场景动画不额外触发 Agent 启停；窗口不可见时暂停无意义绘制，恢复后正确收敛。
- 尊重减少动态效果，直接切换并保留选中／焦点反馈。普通模式的动画是明确需求，不能为追求帧率删除。

实现参考 `apple-design`、`emil-design-eng`、`improve-animations`；原型还原由用户验收，不另设 Agent 动效还原审查。上述原型参数优先于技能中的通用默认值。写 GPUI 前加载对应 `gpui-*` 技能，并以当前钉死版本的代码为准。

### 项目缩略内容与原始画布一致（2026-10-01，代码已实现，验收记录见文末）

- 项目总览与工作空间目录中的项目内容，复用展开态的真实项目 session／Canvas 绘制树，在相同逻辑尺寸上整体等比缩小。保留原标题、类型与状态徽章、按钮、字体、圆角、阴影、毛玻璃和运行特效；删除独立 preview card chrome、最小字号补偿与额外 Message Agent 输入框。
- 项目外壳负责项目名、导航和菜单。按2026-10-02点击反馈修订：总览中的项目标题、预览留白及真实卡片缩略图单击均进入该项目的 Agent 画布，并沿用放大动画；预览不直接接收终端输入、拖拽或关闭卡片，展开后才操作真实内容。项目更多菜单独立响应；目录中的二级预览仍只触发工作空间导航。未就绪内容明确显示加载／未打开状态，不用空正文的仿制卡片冒充实时内容。
- 缩放不修改 document viewport、节点布局或 PTY 尺寸；完整画布以统一比例适配标题下方区域。展开／返回使用同一内容矩形与原有弹簧，终点不重新 fit 节点、不换皮。缩略内容与展开内容共享当前项目主题和壁纸来源。
- 实施：先用真实 Window::draw 回归证明旧预览缺少真实标题栏／按钮；复用已有 gpui::scene 和项目 session，合并目录、总览、展开的内容入口；删除旧预览与 viewport 二次缩放；验证正反向尺寸、真实会话和输入归属。运行 App 单测、check、Release、性能／壁纸护栏后，用 Computer Use 对比两级缩略和展开。
- 设计依据：apple-design 的一致性与内容优先；emil-design-eng 的空间连续性；review-animations 检查可中断转向与减少动态效果。Browser 原生层现有合成限制仍单列 G1，不把 Terminal 通过等同于全部卡片通过。

### 项目快捷切换动效修正（2026-10-01，代码已实现，真实 App 待验收）

- 23 号原型的 `data-quick-project → openProject → aim` 直接修改同一个 `.world` 镜头，不经过总览。镜头用临界阻尼弹簧（ω=15，刚度225、阻尼30），中途改目标保留位置与速度；同级项目在相同全屏尺度横向／按网格方向移动。
- 原型同时用 180ms ease-out 切换项目透明度、350ms ease-out 改变局部尺寸（未展开700、展开870），卡片局部重排330ms。复用生产稳定项目网格作为空间锚点；真实 session 以固定局部布局参与平移、等比缩放和淡入淡出，壁纸与 Dock 不跟随移动，不用截图或简化卡片替换内容。
- 此条覆盖此前将高频直达理解成无动画的实现决定。编号、项目菜单、快捷键共用入口；直达表示没有「返回总览→重新展开」两段动画，不表示瞬间替换。重复当前目标不重启；连续 A→B→C→A 保留可见层和镜头速度；减少动态效果立即落定。
- 加载中保留当前项目；加载成功后开始直接动画；取消、失败、过期结果不得串目标。退出项目、返回总览仍从当前几何继续，旧会话继续运行且只让当前目标接收输入。
- 实施顺序：①修改 Dock 聚焦回归并取得失败证据；②复用 DesktopCamera 和已有 GPUI scene 呈现变换，补连续转向／透明度缓存检查；③通过受影响测试、check、Release；④Computer Use 操作隔离 App，检查连续画面与目标输入。原生 Browser、IME、总览统一场景及帧性能以实际证据记录，不能由普通 Terminal 终态截图推断通过。
- 设计依据：apple-design 的空间连续性与内容优先；emil-design-eng 的可中断响应；improve-animations 对触发入口／轨迹／终态的局部审计；review-animations 检查减少动态效果和无空白跳帧。原型已由用户明确指定，通用「高频减少动画」建议不能覆盖它。

### 工作空间卡片内置标题（2026-10-01，代码已实现，真实 App 待验收）

- 工作空间名称放入卡片顶部的紧凑标题栏，与缩略画布共用同一半透明底板、圆角和边框；左侧显示名称，右侧保留「⋯」菜单入口。名称不得悬浮在卡片外，也不在卡片上／下方重复显示。
- 标题栏单行、垂直居中，名称优先分配宽度，过长时末尾省略并可查看完整名称；项目数量与运行状态降为次要信息，空间不足时先省略次要信息。空空间、加载中、有项目的卡片使用相同结构。
- 真实画布缩略内容排列在标题栏下方，按内容区等比缩放并裁剪，不让标题遮住 Agent 卡片。保持紧凑网格与可见工作空间数量，移除只为外置标题预留的卡片外间距；不通过增高卡片或增加独立说明区解决布局。
- 名称和卡片主体点击均进入该工作空间的项目总览；「⋯」只打开菜单并阻止触发进入。沿用 hover、pressed、keyboard focus、selected 状态，明暗壁纸下名称可读。
- 进入工作空间时，标题栏与卡片外壳一起淡出；展开后不残留标题栏。返回目录时与外壳一起恢复，期间不得漂移到边框外。工作空间目录与快捷切换视图共用这一组件。
- 实施定位：`workspace/desktop/workspace_card.rs` 已移除外置标题的 `-54px` 偏移和 `60px` 上边距，`launchpad.rs` 同步删除配套镜头偏移。标题栏为 40px，名称单行省略，悬停可查看全名和状态；短名称且宽度足够时显示项目数量，否则优先保留名称／菜单。`scene.rs::workspace_content_rect` 供静态缩略图和 `view.rs` 的工作空间转场共用，等比适配标题下方内容区，并随外壳淡出归零内边距，展开终点不留标题空白。遵循 apple-design 的清晰归属、内容优先与直接操作；实际明暗壁纸和连续动画仍须 Computer Use 验收。

### 创建时命名工作空间（2026-10-01，代码已实现，真实 App 待验收）

- 点击「新建工作空间」后显示紧凑命名对话框：标题「创建工作空间」、单个「工作空间名称」输入框及「取消／创建」按钮。输入框默认留空并自动聚焦，示例占位「例如：官网改版」；用户填写后才创建，不再直接生成 `Workspace N` 或 `New Workspace`。
- 复用存储已有规则：去掉首尾空白后为 1–80 个 Unicode 字符，支持中文；空白或超长时显示字段提示且禁止提交，不静默截断。允许同名，继续以独立 ID 区分，不合并已有空间。
- Enter 提交有效名称，中文输入法组词期间 Enter 只确认候选词；Escape／取消关闭，不创建记录、不切换当前画布。提交中防止重复创建；失败保留名称并允许重试，成功后进入新空间的空项目总览。
- 首页、工作空间切换界面的新建入口，以及打开／克隆项目后的目标选择器「新建工作空间」，共用命名流程。后者命名成功后继续原项目添加流程，取消命名返回目标选择器并保留待添加项目，不隐式创建默认名称空间。
- 名称在工作空间目录缩略图、切换入口列表和既有名称展示处保持一致，可被搜索，重启后恢复；长名称按现有布局省略并可查看完整名称，不重新引入展开画布的重复标题栏。创建后继续支持已有改名。
- 实施复用 `DesktopWorkspace.name`、`DesktopWorkspaceStore::create/rename` 及现有 Input／对话框组件，不新增存储结构；按 apple-design 的简洁层级与即时反馈处理焦点、校验、提交与取消。

### Agent 创建的默认目录（2026-10-01）

- 项目区域是创建目标时，Agent 的工作目录 `cwd` 直接取该区域已选定并保存的实际目录 `area.root`；该目录本身是 Git worktree 时直接使用，普通非 Git 项目同样适用。用户所说“项目作为工作空间”在这里指 Agent 的执行目录，不改变桌面工作空间 → 项目区域 → 卡片的归属。
- 不列出 worktree 供二次选择，不要求确认，也不自动创建 worktree、切换分支或改用仓库主目录。同一目录允许多 Agent、跨工作空间独立卡片同时运行。
- 入口尚未确定目标项目时，只选择／添加项目区域；不猜测其他项目或继承其他工作空间的活跃目录。重复添加同项目仍定位既有区域，不隐式改写其目录或既有卡片的 `cwd`。
- 复用现有目录与项目身份校验，在后台执行。异步请求捕获原始 workspace／area／目录身份；导航后不得投递到新选中的项目，目标已删除或失效时不得启动，失败显示可重试状态。既有卡片的执行目录不随新默认规则改变。

## 5. 阶段实施计划

**执行顺序修订（2026-10-01）**：按用户最新 Goal 指令，允许先完成各阶段的功能开发，再集中进行真实 App 验收。阶段顺序保留为开发分解，未完成的 G0 基线、G1 真实输入／Browser／视频／性能关卡和 G5 验收仍须补齐；代码可提前推进，不因开发完成而把验收状态标为通过。

| 阶段 | 状态 | 依赖 | 必须交付及退出条件 |
| --- | --- | --- | --- |
| G0 基线与验收固定 | IN_PROGRESS | 无 | 隔离 App、当前性能与画面基线、验收映射、已知失败复现 |
| G1 最小真实场景验证 | TODO | G0 | 真实卡片统一变换、输入、PTY、壁纸、Browser 验证；可行性与帧时间证据 |
| G2 统一布局与导航 | TODO | G1 通过 | 同源布局，直接转向，取消旧分段交接；按 24 号参考统一两级窗口外观，工作空间标题收进卡片 |
| G3 绘制与资源调度 | TODO | G2 | 可见性剔除、精确失效；按需纹理预算与回收；压力证据 |
| G4 完整交互与回归 | TODO | G3 | 工作空间创建时命名；Agent 默认项目目录创建；所有卡片、IME、快捷键、持久化／删除／背景会话回归 |
| G5 功能／性能验收与清理 | TODO | G4 | Release、所需 crate 测试、真实 App 功能回归、性能对比，移除旧链路；原型还原另由用户验收 |

### G0：冻结可复现基线

1. 核对当前工作树与未提交差异；保存二进制 SHA-256、HEAD、diff 摘要、macOS／机器／显示器／窗口尺寸。不能只用 Git SHA 标识脏工作树构建。
2. 在独立数据目录准备两个工作空间、各两个项目、各两张真实卡片，共 `2 × 2 × 2 = 8` 张；其中两张持续输出。一组让不同空间的同一项目使用同一 worktree。
3. 用 Computer Use 录制目录 → 工作空间 → 项目 → 反向 → 直接切换的基线；记录壁纸、Dock、标题、卡片几何、输入与会话情况。
4. 对连续转向的纯状态机补可控时间的单元测试，以真实 App 连续画面证明导航行为。按 USC-01 至 USC-14 映射证据，不再使用旧 E2E 操作入口。
5. 保留历史恢复失败记录，在隔离真实 App／真实 PTY 路径验证会话恢复；若产品确有缺陷，在 G4 修复，不继续维护已停用 harness。

退出：验收映射、当前数据、失败证据和后续测量方法齐全。此阶段不宣称优化成功。

### G1：先完成一个真实纵向切片

1. 在现有 GPUI 渲染路径验证局部坐标下的绘制复用与呈现变换；只支持本任务需要的平移、正数等比缩放、透明度与裁剪，不泛化旋转／3D 引擎。
2. 在工作台里接入两个项目的真实 Terminal／Agent 内容，共用现有绘制器；以现有 runtime 和 PTY 验证持续输出、输入、选择与聚焦。
3. 对变换覆盖的文字、圆角、阴影、图片和视频 surface 补最小测试；证明输入命中与显示一致。可短暂保留旧实现作为比较基线，但不形成长期双路径。
4. 加入一个真实 Browser 卡片验证原生视图的层级、裁剪、转场及落定交互；如需缓存，验证来源与会话连续性。
5. 验证项目展开的局部重排及终端 resize 提交点；纯镜头动画期间 resize 次数必须为零。
6. 使用同机器同负载比较 Release 基线与候选；输出布局次数、内容构建次数、CPU／GPU 帧耗时、实际呈现掉帧及输入证据。

退出：这个切片完整可用且达到第 7 节帧目标；展示两种相反的操作和中途转向。若做不到，先定位并改进渲染方案，禁止继续批量迁移来掩盖基础缺陷。不要擅自改为全工作台 WebView 或删减真实卡片能力。

### G2：统一场景、布局与可中断导航

1. 复用已有身份构建显示结构：工作空间 → 项目区域 → 卡片；建立逻辑布局、当前视觉变换、导航目标的清晰职责。
2. 统一目录缩略图、工作空间总览、项目展开的卡片入口；目录与快捷切换入口使用相同场景和布局规则。
3. 将镜头变化从全局布局失效中分离；响应式列数基于稳定可用尺寸，不随镜头缩放跳断点。
4. 替换 `pending_navigation` 对动画的串行安排；新目标从当前视觉状态继续。异步加载用现有身份／请求有效性机制防止迟到覆盖。
5. 保留搜索、滚动、冷恢复、空画布与添加项目布局；按 24 号参考统一项目和工作空间窗口外壳、真实缩略内容及空状态。展开填满窗口安全边距和 Dock 之间并消除外壳，返回时外壳与内容连续恢复；覆盖空／非空、选中／未选中与亮／暗壁纸，交互反馈与文字对比度可辨。
6. 重构工作空间卡片内置标题：名称／菜单锚点与预览共用外壳，删除外置标题的负偏移和额外占位，预览适配标题下方内容区；覆盖首页目录、快捷切换、空／加载中／非空和长名称。标题与主体点击进入同一空间，菜单不误触发导航。
7. 通过底层单元测试验证标题边界位于卡片内、预览不与标题重叠、网格不为标题多留外间距；Computer Use 检查长中文名称、窄窗口、明暗壁纸、名称／菜单点击和展开／返回连续画面，覆盖 USC-07／08。输入、运行实例和归属的验收标准不得降低，不维护已停用的旧套件。

退出：USC-01、02、06、07 的窗口外观部分及 08 的底层测试与真实切片通过；同时对照 23 号动效与 24 号窗口外观，没有通过替换卡片内容或中心猜测起点来完成转场。

### G3：只绘制需要变化的内容

1. 优先复用已有终端文字与图像缓存；用内容版本、真实布局和显示条件控制失效，镜头运动只更新呈现所需数据。
2. 在重型卡片内容构建前裁剪；隐藏工作空间保持 runtime 消费输出但停止无意义绘制，重新可见时正确更新。
3. profile 若证明绘制指令重放已满足门槛，不新增纹理层；若仍不足，仅对测得的复杂子树加入 GPU 离屏缓存。
4. 如果加入纹理，记录字节预算、分辨率策略、LRU／等价淘汰、活跃输入优先级和失效条件；内存压力下丢弃缓存后仍可完整重建。
5. 保持实时输出、光标、输入和状态更新；禁止冻结所有缩略图、停止后台 Agent、CPU 读回或降低壁纸质量来通过门槛。

退出：USC-07、11、13 的压力和资源证据通过；无纹理实现时在计划中记录省略依据，不将该条件项误标为缺功能。

### G4：输入、原生视图与生命周期收口

1. 覆盖总览就地聚焦、展开输入、IME、粘贴、选区、滚动、链接、菜单、项目内拖拽／resize；不得跨项目或工作空间移动。
2. 保留原 Canvas Dock、Settings 快捷键录制／恢复和键盘焦点；工作空间切换在当前窗口发生。
3. 验证同项目跨工作空间、同 worktree 独立会话、后台输出、删除取消／确认、重启恢复及退出回收。
4. 回归 Browser、预览和其他当前已支持卡片类型；原生视图与 GPU 场景的显示／命中必须同步。
5. 修复 G0 复现的会话恢复失败，保留失败到通过证据；历史 harness 共享环境问题仅作记录；验收依据是替代测试与真实 App，不改变生产恢复语义或仅延长超时。
6. 覆盖减少动态效果、不同显示缩放与窗口尺寸；共享会话路径变动时运行已有手机后台回归。
7. 收口 `begin_desktop_agent_creation_for_area` 及 Dock、快捷键、项目菜单、自定义 Agent 定义等调用路径；去除仅服务 worktree 选择的 `agent_creation` 状态与弹层，以项目区域目录直接进入现有创建流程。保留后台校验、加载反馈和错误处理，不只在某个按钮上跳过弹窗。
8. 先补聚焦失败测试，再验证默认 `cwd` 与异步目标归属：普通目录、Git 主目录、存在多个 worktree 的已选目录、同目录多 Agent、创建中导航／移除。真实 App 至少覆盖 Codex、Claude 与上述入口，检查实际进程／会话启动目录及无选择弹窗；不可只断言 UI 文案。

9. 完成工作空间创建时命名：收口 `controller.rs::create_desktop_workspace`、`choose_desktop_project_destination(None, …)` 及其首页／目标选择器调用方，复用命名输入、已有 1–80 字符校验和后台持久化；先补失败测试，再实现统一的提交／取消行为。
10. 覆盖中文名称、首尾空格、空白、80／81 字符边界、同名独立 ID、重复提交、取消、保存失败保留输入、搜索／改名／重启恢复；Computer Use 验证输入法 Enter 不误提交，以及直接新建和添加项目时新建两条完整路径。

退出：USC-03、04、05、09、10、12、14 通过，替代测试及真实 App 无未解释失败。

### G5：功能／性能验收、删除旧链路并交付

1. 删除废弃预览绘制、过渡叠层、仅服务旧交接的导航队列和临时验证开关；保留有用的可选诊断。
2. 完成第 8 节规定检查并构建最终 Release；更新隔离 App 二进制、签名和构建身份，不能验收旧产物。
3. 用 Computer Use 完成第 6 节的真实 App 功能与交互回归，记录连续切换、输入、会话、壁纸／Dock 状态及性能数据；保留实际行为验收，不安排原型还原审查。
4. 提供可运行 App 和工程验证报告供用户试用；原型布局、材质和动效还原由用户验收。用户提出问题后修复并复验受影响功能，不启动 UI 验收 subagent。
5. 功能测试发现跳变、卡片替身、壁纸／Dock 回归或输入错位时继续修复；工程测试通过与用户的原型验收结论分别记录，不互相替代。
6. 按2026-10-02用户授权，完成实现与规定工程验证后提交本 Goal 相关变更，运行仓库提交验证，再普通推送 `origin/codex/desktop-multi-project-workspaces`；记录commit SHA、推送结果与远端HEAD。保留既有无关变更；禁止跳过hooks或主动强制推送。

退出：所有完成定义成立，用户可运行的产物及报告齐全；才能将新 Goal 标记 complete。

## 6. 验收入口与证据目录

业务流程通过隔离真实 App 的操作、连续画面及会话回执验证；域层与 GPUI 单元测试验证变换、裁剪、缓存、路由和生命周期，真实 PTY 集成测试验证进程与输入输出。不得把旧 harness 搬到别处继续运行，也不得只证明两套内部坐标互相一致。

| SPEC | 复用的主要先例 | 补充真实证据 |
| --- | --- | --- |
| USC-01、02、06、08 | 镜头／布局状态机单元测试 | 进入、返回、跨项目／空间、中途转向、搜索、滚动、resize 连续帧 |
| USC-03、04、05 | runtime 输入路由单元测试＋terminal 真实 PTY 集成测试＋Agent 创建入口聚焦测试 | 默认项目目录创建、无 worktree 弹窗、实际 cwd、同会话输入回执、中文 IME／选择／粘贴 |
| USC-07、09、10 | 桌面 Dock／壁纸／项目菜单与既有 Browser、快捷键测试 | 对照 24 号两级窗口材质与空／选中状态，工作空间标题在卡片内、长名／窄窗口与菜单点击，静态／视频壁纸、原生视图、键盘、减少动态效果 |
| USC-11、13 | 现有帧诊断与 CPU benchmark 入口 | 帧时间、呈现掉帧、CPU／GPU、RSS、缓存字节与淘汰 |
| USC-12 | runtime／storage 生命周期测试＋真实 App 恢复 | 删除／退出后目标 PTY 回收，其他空间继续输出 |
| USC-14／DWS-01 | app 创建入口聚焦测试＋storage 名称校验／持久化测试 | 各入口命名、中文 IME、校验、取消、重复提交、错误恢复、名称展示／搜索／改名与重启 |

执行时在 `docs/design-docs/2026-09-30-desktop-unified-scene/` 建立结果索引；原始采样／录像放 `artifacts/desktop-unified-scene/` 并在索引中给出具体路径。上述目录是待生成产物，不代表现在已有验收。

至少覆盖 `1600 × 1000` 逻辑像素、当前默认窗口、缩小窗口及本机可用的 Retina／显示刷新率。记录逻辑尺寸和 backing scale，不能混用截图像素与窗口坐标。

## 7. 性能协议与通过门槛

以下是本计划制定的验收目标，不是已测结果；固定硬件与输出负载后执行，不允许事后改小负载来声称通过。

### 场景与采样

| 场景 | 数据构造 | 目的 |
| --- | --- | --- |
| S1 基础 | 2 空间 × 2 项目 × 2 卡片 = 8 张；2 个真实 PTY 持续输出 | G1 纵向切片与全部输入能力 |
| S2 密集 | 6 空间 × 4 项目 × 4 卡片 = 96 张；4 个真实 PTY 持续输出，其余真实卡片空闲 | 总览、跨空间、剔除、缓存与资源上限 |
| S3 多入口 | 12 个工作空间，至少 6 个含真实卡片；搜索、滚动后进入 | 保持工作空间数量与可达性，不按原型固定数量封顶 |
| S4 资源／恢复 | 重复进入返回、显示尺寸变化、缓存淘汰、窗口隐藏恢复和冷启动 | 泄漏、迟到加载、终点跳变和空白帧 |

- 持续输出使用可控本地 PTY producer，不消耗真实模型额度；每路每秒 60 行、每行约 80 字节，即约 `60 × 80 = 4,800 B/s`，4 路约 19,200 B/s；30Hz 每批 2 行。保留真实 PTY 与真实卡片渲染。
- 每个性能场景预热 10 秒后采样 60 秒，重复 3 次。正常开关壁纸分别采样，至少一次开启视频壁纸。CPU 采样关闭逐帧日志，详细帧诊断另跑一轮以避免观测干扰。
- 单独测冷启动；预热结果不能覆盖首次打开的空白／卡顿。设置一轮固定镜头下只有 A 输出的对照，确认未变化 B 不重建内容。
- 操作由 Computer Use 完成；复用 CPU sampler 时只采用已准备 App 的 `--pid`／`--no-prepare` 模式，不调用现有脚本中的 AppleScript UI 自动操作。新增真实场景 runner 只负责隔离启动、采样和产物记录。

### 门槛

1. 参考目标为 60Hz，单帧预算 `B = 1000 / 60 ≈ 16.67ms`。在 S1、S2 的预热动画区间，应用 CPU 提交耗时和 GPU 执行耗时各自 P95 ≤ B；两者分别报告，不能把 CPU 时间当作端到端呈现时间。
2. 实际显示错过目标呈现时机的比例 ≤ 1%，且无归因于应用的 >50ms 长停顿（50ms = 3 × 16.67ms）。用呈现时间／平台 trace 验证，requestAnimationFrame 式回调间隔或 headless 模拟时间不能代替。
3. 具备 120Hz 显示设备时增加 `1000 / 120 ≈ 8.33ms` 对照报告；未测或未达标不得声称 120Hz。60Hz 是本轮硬门槛，不暗中降低帧率掩盖失败。
4. 纯镜头移动不逐帧重排未变化的卡片内容，也不发送 PTY resize。通过局部诊断计数证明；局部展开真正改变内容布局时另列重排次数及原因。
5. 若启用纹理缓存，G1 报告必须固定预算及分辨率推导：单纹理字节约为实际像素宽 × 高 × 每像素字节，额外 mip／缓冲另计。G3／G5 缓存峰值不得超过该预算，资源淘汰后不能持续增长；未实现纹理则报告绘制缓存实际占用。
6. 同一固定场景执行 10 轮往返预热，再执行 100 轮往返，比较 GPU 资源数、缓存字节和 RSS 趋势；不得保留已删除节点或每次切换增加一个永久资源。RSS 仅辅助判断，不能将 allocator 留存直接当泄漏。
7. 真实 PTY 的本地输入回显不能等动画结束才出现；连续转向过程中仍可路由已有合法焦点输入。记录 input-to-visible P50／P95，并与 G0 相同命令、机器对照，不将远端模型响应时间计入 UI 延迟。

采样缺失、缓存预算尚未固定、无法识别实际呈现掉帧或机器不具备所声称刷新率时，对应项只能标“未验证”；禁止用“看起来很流畅”填充性能结论。

## 8. 验证命令与工程约束

所有命令在指定 worktree 执行；实现每个非平凡行为前先加聚焦失败测试。测试注释写明中文场景、数据推导、执行步骤及预期，关键正负断言带说明。

```bash
cargo test -p canvas -p runtime -p terminal -p storage
# GPUI 补丁的纯底层测试；独立 manifest 使用 vendor 自带 lockfile。
cargo test --manifest-path vendor/gpui-0.2.2/Cargo.toml --locked --lib scene_transform::tests -- --nocapture
cargo check -p app
cargo build -p app --release
cargo fmt --all -- --check
git diff --check
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
bash scripts/check-agent-guide-lines.sh
bash scripts/check-storage-boundaries.sh
bash scripts/check-performance-guardrails.sh
python3 scripts/check-video-wallpaper-performance.py
```

- 修改 canvas／runtime／terminal／storage／ui 时跑对应 crate 测试；本计划预计触及跨 crate 行为，最终跑 `cargo test --workspace`。真实 PTY 用相关 ignored 用例定向执行，不把全部忽略项一概视为已通过。
- 新增／修改 GPUI 补丁要运行适用的聚焦测试及 app 回归；在 G1 固定实际可执行命令，不假定上游全部测试可在当前 macOS 环境直接运行。
- Rust 新代码与新增文件遵守 ≤500 行、禁止新增 `#[allow]`；不扩大既有 vendor 大文件的职责，必要的新职责拆分到小模块，不借本次任务全量重写上游代码。
- 视频低拷贝、前台无阻塞、输入热路径不写 SQLite 和手机关窗生命周期按 AGENTS／CLAUDE 执行。
- 真 App 只操作隔离验证版，不替换用户常驻安装、不停止其他 Norma 实例；不自动提交、推送或发布安装包。

## 9. 完成定义

- [ ] G0–G5 有实际产物、命令结果与构建身份；条件项写明省略依据。
- [ ] USC-01 至 USC-14 全部有对应证据，输入／会话／归属正负断言通过。
- [ ] 用户负责：原型布局、材质和动效还原的验收结论已记录；未收到结论时标记待用户验收，Agent 不代判通过。
- [ ] 性能目标有 Release 真 App 测量，缓存／内存有界，无 CPU 逐帧截图上传。
- [ ] 替代测试与真实 App 的输入／恢复／生命周期回归通过；历史 E2E 结果不计入验收。
- [ ] Agent 负责：真实卡片、壁纸、Canvas Dock、IME 与 Browser 的功能／交互回归通过；半透明样式与原型动效的还原效果交由用户验收。
- [ ] 工作空间名称与菜单入口位于卡片内部，标题不遮挡预览、不增加外部占位，长名和空／加载状态一致；展开无外围框或残留标题栏，返回外壳与真实内容连续恢复。Agent 验证实际显示状态及交互；是否符合 24 号参考的外观效果由用户确认。
- [ ] 所有 Agent 创建入口默认使用目标项目区域目录，无 worktree 选择或确认；真实 cwd、同目录并行、异步导航／删除的归属与失败路径均验收。
- [ ] 所有新建工作空间入口支持创建时命名；合法名称持久化并一致显示，取消不创建，失败保留输入，USC-14／DWS-01 的边界与真实 App 验收通过。
- [ ] 废弃渲染／导航链路已移除，iOS 与业务身份／存储没有意外变更。
- [ ] 按用户授权提交本次变更并普通push当前功能分支，提交验证通过且远端HEAD已核对；记录commit SHA与推送结果。
- [ ] 最终结果报告注明可运行 App 与证据路径；只有此时将新 Goal 标记 complete。

## 10. 可直接创建 Goal 的提示词

```text
在 /Users/alan/Documents/mybook/norma-ai-os-desktop-workspaces 工作树、codex/desktop-multi-project-workspaces 分支中实施桌面统一场景重构。

唯一实施计划：docs/exec-plans/2026-09-30-desktop-unified-scene-goal.md。
规格：docs/product-specs/2026-09-30-SPEC-desktop-unified-scene.md。
工作空间／项目／卡片与 worktree 归属规则继续遵守 docs/product-specs/2026-09-29-SPEC-desktop-multi-project-workspaces.md。
动效与空间关系基准：docs/design-docs/2026-09-29-desktop-workbench-concepts/23-workspace-canvas-motion-prototype.html。
窗口材质与外观基准：docs/design-docs/2026-09-29-desktop-workbench-concepts/24-overview-glass-windows-reference.png（2026-10-01 用户修订）。项目、工作空间两级都采用轻薄半透明窗口、细边框、较小圆角、真实画布缩略内容与克制的选中反馈，空窗口一致；返回总览时外壳必须随内容连续恢复。工作空间名称必须置于卡片内部的紧凑顶部标题栏，和真实预览共用外壳；移除外置名称和预留间距，长名省略、菜单不误触发进入，展开时随外壳淡出、返回同步恢复。保留用户当前壁纸。
旧计划已归档到 docs/exec-plans/archive/2026-09-30-desktop-workspace-canvas-rebuild.md，仅作历史证据，不能作为施工入口或已完成证明。

目标：保留 Rust／GPUI，将工作空间、项目和真实卡片重构为统一场景、独立布局与 GPU 合成。允许较大范围重构 app、canvas 和现有 GPUI 本地补丁，复用数据结构、卡片内容绘制、Agent 与 PTY。只改桌面端，不改变 iOS 或手机协议。保留当前未提交改动。

依计划 G0–G5 分解工作；按用户许可，可先完成各阶段功能开发，再集中进行真实 App 验收。基线、真实卡片、输入、PTY、Browser、视频壁纸及性能关卡仍须补齐，开发完成不等于验收通过。纯镜头变化不触发全量内容重排或 PTY resize；直接切换和反向操作从当前视觉状态与速度继续。真实卡片统一渲染，复杂内容只在测量证明需要时加入有界 GPU 纹理缓存，不得使用假预览或每帧 CPU 截图读回。

保留原型的镜头弹簧和局部展开重排、壁纸、半透明样式、原 Canvas Dock；展开填满窗口安全边距与 Dock 之间，无外围边框和重复标题，不恢复额外 Norma 输入栏。工作空间切换发生在当前窗口；同项目跨空间独立卡片，同 worktree 允许并行，卡片不能跨项目或空间移动，后台 Agent 持续运行。

创建工作空间必须先由用户输入名称，复用已有 1–80 个 Unicode 字符及首尾空白校验，禁止直接以默认名称创建。所有新建入口共用命名流程，覆盖中文输入法、Enter／Escape、重复提交、取消无副作用、失败保留输入，以及名称展示、搜索、改名和重启恢复；项目目标选择器中新建时保留待添加项目。按 USC-14／DWS-01 验收。

创建 Agent 时直接使用所属项目区域已选定的实际目录作为 cwd，不再弹 Git worktree 选择或确认；不自动创建／切换 worktree 或分支。普通目录与已选 worktree 一视同仁。入口没有目标项目时只选择／添加项目区域。覆盖 Dock、快捷键、项目菜单与自定义 Agent 入口，检查真实 cwd，并防止异步创建因导航或删除而投递到错误项目。桌面对象归属和既有卡片 cwd 不变。

按 AGENTS.md 先 TDD。用户已停用旧 crates/e2e，不得新增、维护、运行或以其结果验收。使用域层/GPUI 单元测试、真实 PTY 集成测试和隔离真实 App 覆盖 USC-01 至 USC-14；验证真实会话恢复，修复产品缺陷。按计划运行范围测试、最终跨 crate 测试、护栏和 Release 构建。

必须使用 Computer Use 操作隔离的真实 App，验证连续快速切换、中途反向、输入法、Browser、背景输出、资源回收及实际显示状态。原型布局、材质和动效还原由用户亲自验收，不安排独立 UI 验收 subagent，不由 Agent 代判还原通过；交付可运行 App，记录并修复用户反馈。使用固定负载采集真实 CPU／GPU、呈现掉帧、缓存／内存和输入延迟，未测不能宣称高性能。

持续更新该实施计划的阶段状态和证据路径。只有全部完成定义成立才能将 Goal 标记 complete；构建或 headless 通过不代表真实验收通过。用户于2026-10-02授权完成实现及规定工程验证后提交本Goal相关变更，并普通push到origin/codex/desktop-multi-project-workspaces，无需再次确认；保留既有无关修改，提交hooks不得跳过，推送后核对远端HEAD。不要替换用户常驻App。
```

## 11. 实时执行记录

| 日期 | 阶段 | 实际工作与证据 | 未完成事项 |
| --- | --- | --- | --- |
| 2026-09-30 | 计划 | 核对原型、GPUI 缓存条件、现有 E2E／诊断、历史失败；生成规格及计划，旧计划归档；发布 GitHub #73（ready-for-agent）；文档链接与格式检查通过 | G0–G5 尚未启动；新方案未有性能或 UI 通过结论 |
| 2026-09-30 | G0 | 已保存 `artifacts/desktop-unified-scene/g0/baseline.patch`、`git-status.txt`、`build.json` 和历史日志；隔离 `Norma Scene Baseline.app` 使用独立数据目录与 bundle ID。项目直接返回目录的聚焦 E2E 已产生有效失败：一次镜头结束仍为 1920×1081，而目录目标为 460×259，见 `direct-navigation-red.log`。会话恢复模块复现 1 passed / 3 failed，见 `session-restore-baseline.log` | 基线真实卡片、连续画面及性能采样未完成；新生产渲染方案尚未实现。先前 CUA 鼠标失败后，重新按完整 App 路径连接，已成功点击进入工作空间；不能据先前工具错误断言 App 死锁 |
| 2026-09-30 | G0 | 真实 UI 已创建独立 project-a 与一张 Terminal，保存空空间、终端启动及恢复后缩略图；fixture A/B 改为独立 Git 仓库，隔离 App 使用专属 ZDOTDIR，原 App 退出的 shell 后代已回收。会话恢复单用例也曾失败，临时探针捕获 CLI 启动阶段延迟；探针已清理，原测试最新一组 4 passed / 0 failed。具体操作与证据见 `artifacts/desktop-unified-scene/g0/observations.md` | CUA 鼠标再次 noWindowsAvailable、原生管道曾失败，尚无 8 卡片连续基线与性能报告；测试仍有历史间歇失败，不能标记修复。G0 尚未通过，不启动整体迁移 |
| 2026-09-30 | G0／G1 局部能力预研 | 已新增 GPUI 固定局部布局的 `scene` 元素；缓存图元、命中与鼠标坐标使用同源可更新变换，嵌套父裁剪与 deferred 绘制保留场景。先复现外容器缩小但内容不缩小，以及 deferred 内容跑到窗口原点，再实现修复。3 个新聚焦场景及既有缓存边界、终端创建、终端回显共 6 项通过；证据见 `artifacts/desktop-unified-scene/g1/observations.md` | 尚未接入生产工作台。G0 8 卡片与性能基线未齐；CUA 鼠标复查仍报 noWindowsAvailable。IME 平台坐标、透明度、Browser、真实卡片切片和性能均待验证，G1 未通过，不开始整体迁移 |
| 2026-09-30 | 测试策略修订 | 用户明确停用当前 E2E；关闭旧 crate 测试入口和依赖，原测试源码及结果仅留历史记录；同步指南、规格与本计划，后续采用底层测试、真实 PTY 和真实 App | 所有业务与性能要求继续有效；既有 E2E 绿灯不计入完成证据 |
| 2026-09-30 | 替代验证 | 已实际跑通 GPUI 独立 manifest 的 2 项纯单测；terminal 51 项库测试及 1 项真实 PTY 进程回收测试通过。Cargo metadata 确认旧 e2e 无测试 target／依赖。用户再次明确端到端使用 Computer Use；日志见 G1 observations | CUA 截图可读，鼠标仍 noWindowsAvailable；未通过真实 UI 或 G0/G1 关卡 |
| 2026-10-01 | 用户追加布局修复 | 内嵌项目移除重复 chrome 边距，使用实际局部视区；项目区域及缩略图复用响应式画布网格。修复前两项几何测试失败，修复后通过；desktop 14 项与 master_layout 11 项通过；独立只读审查发现并关闭了密集布局负缩放问题，代码护栏通过。详见 `artifacts/desktop-unified-scene/g1/layout-validation.md` | Release 最终版本构建、签名通过，隔离产物与身份见 `g1/layout-build.json`；Computer Use 拒绝连接隔离 App（not approved），未通过真实 UI 验收；G0–G5 状态不变 |
| 2026-10-01 | G1 输入适配预研 | 平台 IME 候选框、选区与文字命中共用场景变换，缓存输入跟随父镜头；新增两次明确失败后修复，GPUI scene_ 4 项通过；详见 `g1/scene-ime-red.log`、`scene-ime-clip-red.log`、`scene-ime-green.log` | 真实中文 IME 与 Browser 仍待 Computer Use，透明度和生产切片未完成；App 输入回归3项、check、Release和隔离包签名通过（`g1/scene-ime-build.json`）；G0/G1 未通过 |
| 2026-10-01 | 总览外壳修复与工具恢复 | 项目／工作空间转场保留外壳，透明度随镜头恢复；修正背景层序、缩略按钮与文字比例。desktop 15 项、check、Release、代码／性能／视频护栏通过；red 为新增 helper 缺失的编译失败，非运行期断言。Computer Use 已恢复，真实 App 完成目录→空间→项目→两级返回；构建 `g1/overview-chrome-build.json`，截图与详细记录见 `g1/observations.md`。独立 `layout_acceptance` 审查本轮边框修复无需打回 | 首帧接近终态，未覆盖完整动画；首次进入出现 `session gone` 后恢复，根因未解决。只有两张 Terminal，8 卡片与性能证据未齐。旧链路局部修复不等于统一场景完成 |
| 2026-10-01 | 最新窗口外观与 Agent 创建规则 | 保存 24 号用户原图；在本计划 G2／G4、完成条件、Goal 提示词及两份 SPEC 中记录轻薄半透明窗口、真实缩略内容、默认区域 cwd、移除 worktree 选择／确认、真实 cwd 与异步归属验收 | 此次为文档修订；最新外观与 Agent 默认目录行为待实施、待验收。G0 仍进行中，G1–G5 未通过，Goal 保持 active |
| 2026-10-01 | G0 八卡片与冷启动检查 | Computer Use 创建至两个非空空间各两项目、各两张真实 Terminal；保存 `g0/s1-fixture-inventory.json`、`s1-fixture-build.json` 及 B 的 pwd 回显。有限时长 PTY producer 自检通过。重启后的两次窗口连接超时；主线程采样停在启动期文件 open，详见 `g0/observations.md` | 仍有额外空空间，8 张卡片中重启前仅 7 live。冷启动具体阻塞文件未定位；未启动持续输出、未采性能，不满足 G0 退出条件。最新窗口外观与 Agent 默认 cwd 仍待实施 |
| 2026-10-01 | G0 冷启动边界定位 | 临时日志将阻塞收敛到隔离 app.sqlite 的 Connection::open；数据库完整性检查 ok。系统日志记录完整磁盘访问预检拒绝，尚未确定最终授权项。诊断及清理 Release 均通过，探针已移除，原隔离 App 二进制与签名已恢复；证据见 g0/boot-diagnostic-build.json 和 observations.md | Finder 前台启动仍超时，Computer Use 安全策略禁止读取系统授权弹层；需用户确认屏幕上的系统提示。冷启动未修复，性能未采样，G0–G5 状态不变 |
| 2026-10-01 | G0 第三轮阻塞复核 | PID 99238 与原二进制 SHA256 再次核实；同一进程仍存活超过 12 分钟，Computer Use getApp 16.8 秒后返回 timeoutReached。3 秒 sample 在至少 45 秒后仍无输出，已停止采样器，未重启 App。证据：g0/boot-blocked-audit.json | 相同冷启动阻塞已连续三轮；需用户确认是否有系统文件访问／开发者工具提示及原文。无法完成 G0 真实基线，不能越过关卡整体迁移。Goal 标记 BLOCKED；未修改权限，未绕过受保护系统界面，未将预检拒绝视为最终根因 |
| 2026-10-01 | 恢复执行与 G0 采样 | 原 PID 已退出后通过 Computer Use 启动同一隔离 App（16449），核实两空间各 4 live；两路真实 producer 各完成 900 秒与 54,000 行。CPU 采样脚本两项兼容错误先失败后修复，8 项测试通过；三轮 60 秒 CPU 中位数 4.0%、3.9%、3.9%。结果索引见 `docs/design-docs/2026-09-30-desktop-unified-scene/README.md` | CPU 占用不证明帧时长达标；Metal trace 因磁盘不足退出，8.9GiB 不完整产物已清理，Hitches 探针也未保存。仅清理 worktree 内已核实的重复诊断二进制备份和一份可重建增量缓存，恢复写入；GPU／呈现及完整动画未验证。冷恢复的 session gone 仍待修复。Goal active，未越过 G0 |
| 2026-10-01 | G0 有界帧诊断 | GPUI 增加 `GPUI_FRAME_TIMINGS=1`：复用现有日志，最多 300 秒／36,000 次提交，分别记录 CPU draw＋submit、Metal 完成后的 GPU 时间戳、drawable 实际 presentedTime、尺寸与 backing scale；按 PID／提交 ID 关联，不新增截图或 GPU 等待。时间戳单测先因缺少实现编译失败，随后 1 项通过，scene 回归 4 项通过；check、Release、签名、行数／allow／视频护栏通过。证据 `g0/frame-timing-*.log`、`frame-timing-review-build.json` | 新 bundle（67976）与既有隔离 bundle（69255）均在启动期文件 open 等待，尚无帧记录；前者已停止，后者保留现场。最新 review App 为诊断构建，原基线二进制保存为 `g0/pre-timing-baseline-norma-os.bin`。仅清理本 worktree 三份可重建缓存，剩余约 2.8GiB；完整动画、GPU／呈现、S1 正式采样仍未通过。相同启动阻塞在恢复后本轮重新出现，按新一轮计数，Goal 保持 active |
| 2026-10-01 | G0 启动第二轮复核 | 核实同一 PID 69255、相同二进制身份；Computer Use 16.6789 秒后仍 timeoutReached，未重启。证据 `g0/frame-timing-startup-audit-2.json`，本轮分类为已验证等待 | 需用户确认有无系统文件访问／开发者工具提示及原文；最终权限原因尚未证实。恢复后相同阻塞连续两轮，Goal active；G0–G5 不变 |
| 2026-10-01 | G0 启动第三轮阻塞审计 | 同一 PID 69255 仍存活，SHA256 不变；Computer Use 16.6765 秒后再次 timeoutReached。证据 `g0/frame-timing-startup-audit-3.json`；未重启、改权限或读取受保护系统界面 | 相同启动阻塞在恢复后连续三轮，无法继续必要 G0 真实基线；需要用户确认屏幕上的系统提示或明确没有提示。Goal 标记 BLOCKED；完整目标与 G0–G5 完成条件保持不变 |
| 2026-10-01 | G0 用户处理后恢复 | 同一 PID 69255 通过 Computer Use 完成目录→项目展开→项目总览；真实 Terminal 的 pwd 回显 fixture/project-b。有界日志 7,268 组，三类回调 ID 完整配对；GPU 均 Completed，2,144 条呈现零值保留为无效。2560×1600 像素、scale=2，对应 1280×800 逻辑尺寸。见 `g0/performance/resumed-bounded-frame-summary.json` 与 `g0/frame-timing-resumed-input.png` | Goal active；旧启动阻塞解除，原因未经证实。探针无固定负载及独立连续动画区间，不能判性能通过；目录新增空 Workspace 3，project-a 总览仍 session gone、展开后恢复，见 `g0/frame-timing-resumed-project-overview.png`。G0 未通过，后续按现有关卡推进 |

### 2026-10-01：Agent 默认目录修复与工作空间入口反馈

- Agent 创建：项目菜单 New Codex、展开项目 Cmd+2 创建 Claude、Dock 创建 Codex 均直接生成真实卡片，无 App worktree 选择器。`agent-default-root-processes.json` 核验 Codex cwd 为 plain-project，Claude/Codex cwd 为 chosen-worktree；git-main 仍仅有原 main/selected 两个 worktree。CLI 自身信任／更新界面保留，未代替用户确认或提交任务。
- `agent-default-root-regression.log`：16 项通过；check、Release 及代码护栏通过。可运行隔离产物 `artifacts/desktop-unified-scene/g1/Norma Agent Review.app`，构建身份见 `agent-default-root-build.json`。仅此项交互修复，不代表 G4 或整体 Goal 完成。
- 工作空间入口修复：删除异步加载结束时恢复历史展开项目的逻辑，避免工作空间动画落定后又启动项目动画。新增两个实际入口回归先失败后通过，覆盖磁盘历史及内存历史；desktop 共 18 项通过，check、Release、diff／行数／allow 护栏通过，日志为 `g1/workspace-entry-*.log`。
- Computer Use 验收 `Norma Workspace Entry Review.app`：磁盘已有历史项目时首次进入停在两项目总览；显式点击 chosen-worktree 才展开；返回目录再进空间仍停在总览，两次分别在约 36 秒和 78 秒后复查保持总览。截图、动作时间和构建身份见 `g1/workspace-entry-*.png`、`workspace-entry-ui-steps.json`、`workspace-entry-build.json`。
- 本轮确认自动跳项目已修复；截图不覆盖完整动画，不能证明所有瞬时闪烁已消除。冷加载总览仍出现 `session gone`，项目展开才恢复，该既有问题继续保留在 Goal 待办。未通过统一场景／性能关卡，G0–G5 状态不变。


### 2026-10-01：返回动画入口修复

- Dock Workspace／⌘⇧K 原来直接覆盖全屏切换器，绕过镜头；现已复用目录画布与原型弹簧。Back to Canvas、Escape、再次切换可动画返回原层级，中途反向保留当前几何。常规工作空间选择仍不自动展开项目。
- 新增三个实际控制器回归先失败后通过；desktop 21 项、check、Release、签名与代码护栏通过。证据与源码身份见 `artifacts/desktop-unified-scene/g1/return-motion-build.json` 和 `observations.md`。
- 修复后 Computer Use 启动新隔离包及更新的原隔离包均 timeoutReached；进程采样停在文件 open，真实动画尚未验收。卡片换层样式、统一场景和性能仍待处理。G0–G5 未通过，Goal 保持 active；不以本次局部修复标记整体完成。


### 2026-10-01：Dock 内切换当前空间项目

- 用户新增底部 `01 02 … ▦` 入口：编号切换当前空间对应项目，网格返回项目总览；选中高亮、项目名提示，多项目横向滚动。复用原导航与镜头，项目总览和展开画布均可使用。
- 已实现；实际 Dock 点击回归先失败后通过，desktop 22 项、check、Release 和代码护栏通过。最新隔离包身份见 `artifacts/desktop-unified-scene/g1/project-dock-build.json`，证据与待办见同目录 observations.md。
- 新版隔离 App 启动时 Computer Use 连续三次超时，主线程采样停在文件 open；真实编号切换／视觉状态验收待完成。旧版截图不能算新版证据；G0–G5 仍未通过，整体 Goal 不因本次局部功能完成。


### 2026-10-01：本轮桌面优化提交检查

- 用户要求完成此次优化后提交一次代码。提交范围为本分支累计的桌面导航、项目布局、默认 Agent 目录、底部项目编号及相关测试／规格／场景预研；不将此检查点视为 G0–G5 全部完成。
- 完整 App 库测试首次 831 passed / 1 failed / 7 ignored：旧 Dock 源码结构测试把项目 Dock 与新增总览 Dock 的图标重复计数。修正函数截取边界后 832 passed / 7 ignored；没有放宽按钮数量断言。Runtime/Storage 311 passed / 3 ignored；GPUI scene 4 项及帧时间算术 1 项、CPU 脚本 8 项通过。证据为 `g1/project-dock-commit-*.log`。
- 全仓串行回归 1639 passed / 29 ignored（144 suites），最新 Release 构建通过。此前并行构建期间音频调度用例超时，完整精确用例复查与全仓串行复跑均通过；保留失败日志，不将其归因为已确认的产品故障。
- 同一隔离 App（PID 90671）随后恢复连接，未重启进程或修改系统权限；启动等待根因仍未知。Computer Use 验证 01→02、重复点击 02、返回项目总览、总览进入 01、进入空工作空间及重新进入原空间；编号高亮、tooltip 和空空间无编号均正确。证据：`g1/project-dock-ui-steps.json`、`project-dock-final.png`、`project-dock-overview.png` 及 `project-dock-commit-validation.json`。
- 本轮确认的是项目切换核心流程与终态样式；大于四项目的滚动、键盘焦点、完整连续动画和帧性能尚未真机验收。空空间一次 ⌘⇧K 未出现可见变化，Dock Workspace 入口正常，记入后续快捷键复核；G0–G5 状态不变。


### 2026-10-01：项目快捷切换直接进入 Agent 画布

- 用户修正交互：底部编号与项目快捷键切换同级项目时直接展示目标 Agent 画布，取消“收回旧项目到总览 → 展开目标项目”的两段路径；只有显式总览入口执行返回镜头。
- 修复统一的 `expand_desktop_project_area`：解绑旧项目窗口并保留会话，立即绑定目标项目；中途再次选择按最新目标执行。目标尚未加载时保留当前画布，加载成功后直接切换，取消／失败清除相应等待导航。
- 当时误将直达等同于取消同级动画；该决定已由本文件「项目快捷切换动效修正」覆盖，历史测试仅证明导航目标正确，不能证明原型动效合格。
- 聚焦回归先复现两项明确失败（点击编号仍为 None、未加载目标覆盖当前画布），修复后 desktop 22 项通过，涵盖 Dock、快捷键、返回中途切换、减少动态效果、异步加载和取消。check 与静态护栏通过；真实 App 与 Release 证据见 `g1/project-dock-direct-*`，此项不改变整体 G0–G5 状态。
- Release 与隔离包 strict 签名通过。新版 PID 54815 经 Computer Use 完整路径、bundle ID、Finder 前台打开均超时；主线程 sample 停在启动文件 open，根因未明，未完成新版真机复验。代码／测试已完成，视觉验收保持待办，未追加提交。


### 2026-10-01：追加工作空间创建时命名计划

- 用户要求创建时填写工作空间名称；已纳入交互合同、G4 任务、完成定义、Goal 提示词及 USC-14／DWS-01 验收。检索确认当前普通新建使用 `Workspace N`，项目目标选择器使用 `New Workspace`，两条路径均需调整。
- 本轮仅更新计划和关联 SPEC，实施状态为 TODO，未修改生产代码或宣称通过功能验收；保持桌面范围及既有名称字段／独立 ID。


### 2026-10-01：追加工作空间卡片名称内置计划

- 用户反馈名称显示在卡片外，已将内置紧凑标题栏、预览内容区、长名、菜单命中及转场要求纳入 G2、USC-07／08、完成定义和 Goal 提示词，并同步两份 SPEC。
- 当前源码证据为 `workspace_card.rs` 的负顶部偏移与外部上边距；本轮只调整计划，状态 TODO，未修改组件或宣称视觉问题已修复。


### 2026-10-01：工作空间名称内置实现

- 已完成 G2 第 6 项的代码修改：名称／菜单归入同一半透明外壳，长名称省略并显示完整 tooltip，数量／运行状态降为次级信息；去除外置标题占位，保持原卡片高度及 16px 网格间距。目录与快捷切换继续复用同一组件。
- 静态预览与展开内容使用同一内容矩形计算；为标题保留 40px 加 8px 留白，预览等比缩放。标题随外壳淡出并裁剪，内容区连续恢复到完整画布，避免展开终点突然移除标题占位。未更改弹簧参数、壁纸或运行会话。
- G2 第 7 项底层验证：新增真实组件布局／点击测试先复现“名称不得出现在卡片顶边外”，修复后验证长名称、菜单边界、标题与内容不重叠、网格密度、菜单不导航／标题导航。另验证转场起点／中途／终点内容几何；desktop 回归 24 项通过。日志 `g1/workspace-title-red-full.log`、`workspace-title-green.log`、`workspace-title-regression.log`。
- Computer Use 连接既有隔离 App 再次 timeoutReached，同一 PID 54815 仍在运行；尚未获得本次构建的可见窗口，不用旧截图或 headless 结果替代真实验收。G2、G5 及整个 Goal 均未通过；创建工作空间时命名仍按 G4 待实施。

- 本次 check、Release 均通过（31 项既有 warning），行数／禁止 allow／性能／视频护栏和 diff 检查通过；构建与源码 SHA256 见 `artifacts/desktop-unified-scene/g1/workspace-title-build.json`。Release 尚未替换正在启动等待的隔离 App，没有提交或推送。


### 2026-10-01：创建命名与总览会话恢复实现

- G4 创建命名已落地：普通新建、切换界面与打开／克隆项目的目标选择器统一使用 `creation.rs`，复用 Input 与原生对话框。空名称自动聚焦，使用存储已有 trim 后 1–80 Unicode 字符校验；提交中禁用，失败保留输入，取消保留待添加项目。成功保存后进入具名空空间或继续原项目添加，无 schema 改动。
- 单行 Enter 的冒泡由表单消费，中文组词检查 marked text，避免候选确认误创建；对话框子视图只持有 host 弱引用，以避开父视图渲染借用。已有公共状态仍为唯一业务状态，未新增状态框架。
- 总览 `session gone` 的底层根因之一已复现并修复：带 desktop_host 的项目在总览也恢复保存节点，窗口 attach/detach 不终止该恢复；仍严格检查项目目录身份，普通后台项目保持原暂停逻辑。停止项目后迟到恢复不能启动第三张未恢复卡片；kill_all 保留节点历史，测试验证运行终端数而非错误要求 live 容器清空。
- 证据：创建前错误写入默认名和总览不恢复各有真实失败日志 `g1/workspace-naming-red-full.log`、`overview-restore-red-full.log`。最终命名聚焦 2 项、桌面回归 27 项、恢复聚焦 2 项、storage 112 项通过；check／Release 通过（31 项既有 warning）。TestPtyBackend 验证不等同于真实 PTY 或输入法验收。
- Computer Use 重新连接同一 PID 54815 成功，旧构建已正常退出；隔离 App 更新为本次 Release 并 strict 验签。首轮启动连接超时，待检查新版可见窗口。构建身份见 `g1/workspace-naming-build.json`；不以旧截图证明本次修复，G0–G5 均未通过。统一真实场景生产迁移、连续动画与性能仍未完成。


### 2026-10-01：真实 App 命名与标题检查，补修长名省略号

- 同一新构建 PID 16801 在启动等待后可连接。Computer Use 确认三个原有空间的标题／菜单均在边框内；点击新建出现默认聚焦的空输入，空名称 Enter 不创建，Escape 取消后仍保留原三项。
- 输入带首尾空白的中文长名称并 Enter，进入空空间，Dock 保留。SQLite 只读核对显示恰好新增一项，持久名称已 trim 且完整，证据 `workspace-naming-persisted.json`。返回目录后名称与卡片共用外壳；菜单只显示 Rename／Delete，不进入画布；点击名称进入相应空空间，终态无外框与重复标题。截图与动作见 `workspace-naming-ui-steps.json`、`workspace-title-directory.png`、`workspace-title-menu.png`、`workspace-title-expanded.png`。
- 真机发现长名称被硬裁切，末尾没有省略号（`workspace-title-long-before-ellipsis.png`）。根因在 GPUI 0.2.2 的 TextLayout：nowrap 重测直接复用上一次宽度，并且裁剪会改变后续测量复用的样式 runs。现有本地补丁增加 truncate_width 缓存匹配，每次截断基于原 runs；未在各个标题中手动截取字符。
- 新聚焦测试用同一个 TextLayout 执行宽→窄→宽：先准确失败“窄列必须重新测量并绘制省略号”，修复后通过并验证宽度恢复时原文和样式不会丢失。GPUI 库 75 passed／1 ignored，App 库 837 passed／7 ignored，check、Release 和行数／allow／性能／视频护栏通过。
- 最终隔离包已更新并 strict 验签，构建身份 `workspace-title-final-build.json`。首轮启动连接再次 timeoutReached，等待最终省略号复验；前一构建的功能证据不能替代该项。中文 IME 候选、窄窗口、全部明暗背景、完整连续动画与性能仍未验收；G0–G5 保持未通过。

### 2026-10-01：恢复同级项目直达动画（代码通过，真机验收待完成）

- 根因已复现：`expand_desktop_project_area` 在已有项目之间切换时把 transition 清空。Dock 真实入口测试首先失败「同级项目切换必须保留直接平移镜头」，证据 `g1/project-switch-motion-red.log`。
- 复用 DesktopCamera 的225/30弹簧；`project_switch.rs` 根据稳定项目网格建立锚点，保留共同镜头、速度与尚可见项目。真实 session 通过已有 `gpui::scene` 呈现平移／等比缩放；180ms ease-out透明度、350ms局部尺寸过渡，宽度比700/870来自23号原型。连续目标不重置位置或透明度；重复当前项目不重播；减少动态效果直接落定。源项目在缩放途中被打断时保留实际可见 viewport。
- 生产同级切换不再逐帧通知所有卡片重绘，复用固定布局及绘制缓存。GPUI scene 增加呈现透明度，对文字、边框、阴影、图片、路径和420f surface同样生效；surface只增加Metal alpha，没有CPU读回。透明度缓存测试先因缺少API失败，补齐后验证重放不累乘旧透明度。
- 渲染回归实际构建项目视图＋Dock，确认同级切换层存在、项目总览不存在。测试最初混用 `VisualTestContext::draw` 与缓存窗口帧导致缓存索引越界，已改用正常 `Window::draw` 帧生命周期，未用运行时兜底隐藏问题。三项目转向测试覆盖A→B→C→A的几何／速度／透明度连续及落定；Dock覆盖快捷键、加载／取消、减少动态效果和显式返回。
- 验证：桌面聚焦28 passed；App库838 passed／7 ignored；GPUI库76 passed／1 ignored；check、Release、行数／allow／storage／performance／video护栏与diff检查通过。既有Browser源代码护栏只放宽对分号的字面匹配，Settings显示门控断言保留。旧crates/e2e未运行。
- 局限必须保留：本次只完成同级真实Agent／Terminal场景切片；目录／工作空间／项目总览还在旧缩略链路。原生Browser为避免覆盖移动中的GPUI内容，在同级转场期间隐藏，终点恢复；真实浏览器连续呈现／裁剪仍是G1必做，不可把该临时方案视为完整还原。中途返回总览仍需真机检查接缝；帧性能未通过。
- 真机：旧省略号构建成功连接并补取 `workspace-title-final-ellipsis.png`，长中文标题可见省略号。随后正常退出旧隔离App，更新Release并strict验签，身份见 `g1/project-switch-build.json`。新版PID20353多次Computer Use连接超时，Finder前台打开也未取得窗口；只读sample／lldb定位 `AppStore::initialize → migrate → rusqlite::open_with_flags → open`，调试器已detach，未改权限或重启用户常驻App。因尚未进入页面，没有新版切换动画或输入验收证据；G0–G5均未通过，Goal保持active。


### 2026-10-01：真实项目画布等比缩略

- 总览、目录与展开态统一调用同一个真实项目 session 的 `project_canvas_scene`；保留原卡片标题、徽章、关闭按钮、字体、圆角、半透明材质和运行内容，删除独立缩略 chrome、节点 fit 与额外输入框。项目外壳只承载项目名称、导航和菜单。
- 固定真实画布逻辑尺寸，缩略与动画只修改呈现变换；展开／返回不再改 document viewport、重排节点或 resize PTY。同级切换继续使用已有弹簧和连续目标切换。目录恢复 host 焦点并遮挡缩小卡片的点击，避免关闭真实卡片或向隐藏终端输入。
- 多项目验收复现目录中部分内容消失：多个 scene 共用标识导致镜头状态复用。现以真实 session ID 区分；聚焦测试先因第一个项目的标题落在第二项目区域而失败，修复后通过。
- 自动检查：App 库 834 passed／7 ignored，check、Release、行数、禁止 allow、性能／视频壁纸护栏和 diff 检查通过。新增测试覆盖真实标题与按钮等比、动画起点与中途、返回位置、PTY／session 不变、缓存帧后的真实 shell 独立输出行、目录焦点与相邻项目隔离。失败／通过日志与构建身份保存在 `artifacts/desktop-unified-scene/g1/live-preview-*`。
- 真实 App 验收和限制详见 `artifacts/desktop-unified-scene/g1/observations.md`。冷目录尚未打开的空间明确显示加载入口；原生 Browser 转场合成、中文 IME、完整连续动画及帧性能仍为 G1／G5 待办，不能由本次 Terminal 外观和单元测试推断通过。整体 G0–G5 状态不变。
- Computer Use 已复核最终构建的目录 → 总览 → 展开 → 返回 → 目录，真实缩略内容、原有材质和布局保持一致，项目之间的镜头隔离正确；证据 `g1/live-preview-*-final.png`。真实键盘输入仍未取得回执（搜索框也出现同样现象），根因未确证，单列后续输入验收／排查项；未宣称完整 E2E 通过。

### 2026-10-01：切换动效诊断日志

- 在 `camera.rs` 的工作空间／项目展开与返回、`project_switch.rs` 的同级直切、`camera_frames.rs` 的共享帧回调与收敛出口接入 `desktop_motion`。复用现有 tracing 和异步磁盘 writer，默认 INFO 开启；每段只输出 start/end 两条，帧回调仅更新固定大小的内存计数。没有新增依赖、逐帧日志、同步文件读写或内容扫描。
- start 包含 `pid`、`transition_id`、`previous_transition_id`、`kind`、源工作空间／项目 ID、目标 `subject`、当前 `source` 与 `requested_target` 几何，以及源状态的项目区域／已加载会话数量。`kind` 为 `workspace_expand`、`workspace_overview`、`project_expand`、`project_overview`、`project_switch`。连续改目标先以 `retargeted` 结束旧段，新段关联旧 ID；弹簧位置、速度和呈现逻辑保持原样。
- end 包含 `outcome`、`elapsed_ms`、`callbacks`、`first_callback_ms`、`max_callback_gap_ms`、`long_callback_gaps`、`clamped_callback_gaps`。`completed` 表示镜头及局部过渡收敛；另明确记录 `reduced_motion`、`headless_disabled`、`forced_finish`、`workspace_created`。减少动态效果为零回调，首回调延迟为 None；重复结束不会多记日志。
- `elapsed_ms` 从动效入口计至收敛／中断，包含首回调前准备时间，不包含入口之前的项目加载或结束之后的窗口绑定。`first_callback_ms` 是入口至第一次回调的延迟；gap 为实际送入动画积分器、尚未被其截断的 delta。长间隔阈值为 2/60 秒≈33.33ms；clamped 表示 delta 超过现有积分上限100ms。它们均不是 GPU 耗时、呈现帧数、掉帧率或完整点击延迟；刷新率不同仍使用这个固定排查阈值。
- 检索：`rg 'desktop_motion' ~/.norma/logs/norma-os.*.log`。只保留动效日志可在启动前设 `RUST_LOG=warn,desktop_motion=info`；排除动效日志使用 `RUST_LOG=info,desktop_motion=off`。需要真实绘制／GPU／呈现证据时用 `GPUI_FRAME_TIMINGS=1 RUST_LOG=info` 启动，再按相同 PID 和 start/end 时间范围对照已有有界帧诊断；本次没有新增逐帧关联 ID，也不声称已完成性能验收。需重新构建并启动后生效。
- 回归用16/34/150ms输入验证未截断统计、改目标分段、零回调与TRACE级也不逐帧刷日志；聚焦1项及桌面回归28项通过，`cargo check -p app`及Release构建通过（32项既有warning），行数／禁止allow／性能护栏和diff检查通过。证据为 `g1/motion-log-red.log`、`motion-log-green.log`、`motion-log-desktop-tests.log`、`motion-log-check.log`、`motion-log-release.log`。本次仅增加诊断，不修改视觉，不替换用户正在运行的 App；G0–G5状态不变。


### 2026-10-01：添加项目与创建工作空间表单优化

- 沿用 Apple Design 的清晰层级与内容优先原则：统一图标、标题、说明、输入与底部操作的间距；表单面板使用不透明主题底色，避免画布文字穿透影响可读性。Emil 交互原则落实为原生组件的即时 hover／按下／焦点反馈，未新增装饰动画。
- 添加项目复用已有 Dialog／List／Input／Button。文件夹与克隆入口并列；最近项目改为名称＋路径两行、完整路径提示、搜索与虚拟滚动，读取全部已保存历史，不再截断前6条。当前历史容量仍为50；没有新增分页或索引服务。已在当前空间的路径标记 Added，鼠标和回车确认均防重复。
- 创建工作空间保留空白默认名称、自动聚焦、trim后1–80 Unicode字符校验、保存失败保留输入及重试；新增字符数与清晰的创建按钮。小窗口下按可用尺寸调整宽度与列表高度，取消按钮保持可见。
- 修复窗口级弹窗被每个真实项目缩略图重复绘制：仅宿主绘制 Root 的 dialog layer。准确失败证据为 `g1/creation-dialog-red.log`；回归用正常窗口绘制周期验证嵌入项目不含命名框、宿主仍显示命名框。搜索覆盖50条、末条、同名不同路径与无匹配。
- 当前自动验证：桌面回归31项、App check、Release、行数／禁止allow／diff检查通过；原仓库全量fmt检查仍有本轮之外的既有格式差异，本轮修改文件单独格式化。旧e2e未使用。原生组件交互回归通过：800×600窗口、50条记录、末条搜索、Enter添加、禁用项防重复及Escape关闭。证据 `g1/project-picker-interaction.log`。
- 隔离 App 已更新并 strict 验签，使用复制的数据 `g1/project-forms-data` 与50条测试历史，未改用户正式数据库。Computer Use 四次连接超时；只读sample／lldb定位启动 `AppStore::initialize → migrate → rusqlite::open_with_flags → open`，已detach。当前尚无新版表单截图，真实输入／滚动／视觉不能标记通过；G0–G5状态保持不变。

- 本轮结束时已用 SIGTERM 结束仅本轮启动的无窗口隔离进程 PID38528，避免留后台挂起；已更新的验收包和测试数据保留。真实表单截图与视觉验收仍待启动阻塞解除。


### 2026-10-01：桌面创建与导航快捷键补齐

- 新增 NewWorkspace（⌘N）与 AddProject（⇧⌘A），复用现有命名表单与项目选择器。通过 `shortcuts.rs` 注册，并在 Settings → Keyboard Shortcuts 支持录制、持久化、冲突校验、单项恢复及恢复全部默认。
- 保留并验证 ⌥⌘←/→ 在当前空间循环切换项目、⌥⌘↑返回项目列表、⌃⌘↑返回工作空间列表；继续复用统一镜头与同级直达动画，后台会话不重建。创建/添加从子项目焦点延迟转交窗口宿主，避免子项目拥有第二份表单或重入借用。
- 模态表单/Settings未关闭时不响应桌面导航，避免输入和目标空间被改走；目录尚未选择空间时添加快捷键无操作。Apple Design 的平台熟悉性落实为⌘N创建、一致的方向键分层、沿用现有焦点和取消行为；GPUI Actions/Testing技能用于复用现有注册/分发及真实按键事件测试，没有新增快捷键框架。
- 新测试先复现“创建工作空间缺少默认快捷键”和“目录⌘N未打开命名表单”，证据 `g1/shortcuts-create-red.log`、`shortcuts-routing-red.log`。新按键测试覆盖项目焦点→宿主表单、取消不保存、模态导航拦截、项目双向循环及两级返回动画；桌面33项、快捷键注册/设置11项、App check与行数/禁止allow/diff检查通过。子项目Settings转交前保护也由失败测试复现并修复（`g1/shortcuts-modal-red.log`），设置保护同时在子项目和宿主检查。Release及隔离包strict签名通过（32项既有warning），构建身份见 `g1/shortcuts-build.json`。本轮只限桌面，不修改iOS、不运行旧e2e、不提交或推送。

- 真实App验收未通过：Computer Use直接启动及重试均timeoutReached，PID75430的sample显示主线程仍停在文件open，未到窗口。通过Terminal启动的备用方式被Computer Use安全策略拒绝，未绕过该限制。保留 `g1/shortcuts-startup.sample.txt`，结束仅本轮无窗口隔离进程；用户常驻App未替换，G0–G5状态不变。现有按键路由测试不能替代真实macOS键盘验收。


### 2026-10-01：工作空间顺序保持固定

- 移除按最近打开时间倒序排列：存储统一按 created_at_unix_ms ASC、id ASC 读取，新建空间在内存列表末尾追加。选择、返回、搜索清空和重启不再将当前空间移到首位；最近打开时间仍记录，前后切换快捷键与列表采用相同创建顺序。Apple Design 的空间稳定性落实为保留条目位置，不引入额外排序设置。
- TDD 分别复现数据库最近访问排序与新建 insert(0) 两处问题，证据 `g1/workspace-order-red.log`、`workspace-order-create-red.log`。回归覆盖打开 Beta 后 Alpha 仍在首位、搜索恢复、Gamma 追加末尾与数据库重载一致；桌面34项、storage112项通过。App check、Release、行数／禁止allow／存储边界／diff检查通过（32项既有warning）；未使用旧e2e。
- 更新隔离验收包并通过 strict 验签。Computer Use 使用绝对路径启动及 bundle ID 重试均 timeoutReached，未取得新版窗口或截图，真实交互验收仍未通过。本轮仅结束自行启动的隔离进程 PID90907，未替换用户常驻App；不绕过先前Terminal安全拒绝。构建身份见 `g1/workspace-order-build.json`；整体G0–G5状态不变。


### 2026-10-01：Browser 原生场景接线与试用包准备

- GPUI 在整帧结束时把最新变换、裁剪、透明度与固定逻辑尺寸交给原生视图；缓存重放保留注册，离场通知隐藏。Wry／CEF 共用 AppKit clipping host，缩放容器、不逐帧重排网页；桌面项目离开展开态保留网页实例，释放焦点时只处理属于自身网页的 first responder。
- 删除转场／后台项目隐藏 Browser 的临时 gate，接入真实 Canvas 的 scene 呈现路径。生命周期旧行为的失败证据为 `g1/native-browser-lifecycle-red.log`；App 单测 842 passed／7 ignored，记录 `g1/native-browser-app-tests.log`；GPUI 场景回归 6 passed，记录 `g1/native-scene-regression.log`。默认和 chromium-browser 编译、性能及视频壁纸护栏已通过。原生缩放、裁剪、输入、菜单遮挡和 Metal 图层时序尚无真实 App 证据，G1／G4 不勾选。
- 独立源码复核仍发现三项阻断：跨层导航等待 pending_navigation 中间动画；同级切换途中返回总览可能同时呈现同一项目两次；缺失来源布局时使用居中猜测框。对应真实动画验收待补，不能用旧构建的静态截图判通过。
- 按用户“先打一个包”要求暂缓继续扩展，使用 `scripts/release-local.sh` 生成 Developer ID 签名、未公证的本地试用包。日志 `g1/local-preview-package.log`；完成后记录准确产物与摘要。本轮不自动提交、推送或安装。

- 试用包已生成：`build/Norma OS_0.1.71_aarch64.dmg`，195,635,574 bytes；App 为 `build/Norma OS.app`。版本 0.1.71，manifest 无版本变更；Developer ID 签名与 strict 验签、DMG 布局、hdiutil 校验通过，未公证。SHA-256：`04625603b4da018f93370d5a28c8bda500b2a194b9c05aa982b0e146671690d9`；完整构建身份见 `g1/local-preview-package.json`。首次打包因旧镜像挂载冲突失败，正常卸载旧镜像后重跑成功；未强制卸载。焦点护栏最终聚焦回归通过，记录 `g1/native-browser-focus-final.log`。本包包含未提交 Browser 改动，真实 UI／性能验收仍未完成。


### 2026-10-01：原型验收移交用户

- 用户取消剩余任务中的第 5 项独立原型还原验收，改为本人试用并反馈；移除未来 UI 验收 subagent 的安排。同步更新阶段表、G5、完成定义、可复用 Goal 提示词和统一场景规格的验收分工。此前源码审查记录保留为已发现缺陷的证据，不作为新的验收关卡。
- Agent 剩余工作为连续导航、Browser 场景整合、输入／会话回归和 S1–S4 性能验收；继续负责 Computer Use 功能测试、构建和工程报告。原型仍为实现依据，用户反馈后修复；本次仅调整文档，不新增产品代码，不重跑构建或测试，Goal 保持 ACTIVE。


### 2026-10-01：镜头起点与总览共用布局

- 上轮为计划分工修订的实际进展；本轮继续功能开发，不恢复独立原型审查。移除 `centered_source_rect` 的 32%／42% 居中猜测框。目录卡片和镜头来源共用 `layout::workspace_layout`；项目镜头从现有响应式项目网格计算，展开期间调整窗口尺寸后，返回使用新网格而非旧 prepaint 缓存。
- 目录保存实际滚动句柄，展开／返回采用同一滚动偏移。通过快捷导航选择搜索外的工作空间时先清空过滤，建立包含目标的真实目录布局；点击搜索结果内的目标保留过滤。不存在的项目／空间不伪造来源。布局计算仅在渲染布局与导航入口进行，不重排真实卡片或逐帧 resize PTY。
- 聚焦失败证据 `g1/camera-origins-red.log`：冷布局起点实际使用 (435.2,232,409.6,336)，与真正绘制的 Beta 矩形 (450.5,118,379,236.5) 不符；搜索外目标未恢复布局。修复后测试比较实际 Window::draw，允许 1/backing-scale 的设备像素取整；另覆盖窄窗返回第二行、长目录滚动往返。桌面 37 项和 App 845 passed／7 ignored 通过，见 `g1/camera-origins-desktop-regression.log`、`camera-origins-app-tests.log`；App check 0 errors／32 项既有 warning，行数、禁止 allow、性能／视频壁纸护栏和 diff 检查通过。
- Release 构建及隔离 App 的 ad-hoc strict 验签通过，构建身份为 `g1/camera-origins-build.json`。首次 Computer Use 连接超时；接续轮核实 PID 10083 仍存活后重连同一实例成功，未因观察超时重启。完成目录→Workspace 1 总览→快捷键展开 plain-project→返回项目总览→返回目录，四张 `g1/camera-origins-*.png` 保存最终功能状态。卡片实际内容在目录仍可见；这些静态画面不证明完整运动轨迹、搜索／滚动／缩窗、输入或性能通过。原型由用户验收，G0–G5 状态不变。

### 2026-10-01：同级切换途中返回项目总览

- 聚焦测试走真实导航入口和 Window::draw：A→B 推进32ms后返回，复现总览再次绘制A，以及动画末帧A透明度仍为0，证据 `g1/project-return-duplicate-red.log`、`project-return-red.log`。追加未加载C场景也复现占位丢失，证据 `project-return-placeholder-red.log`。
- 项目镜头锚点统一取真实总览内容矩形，移除把700/870局部缩放重复用于空间间距的计算；该比例继续用于非选中项目的局部尺寸。返回时保留当前镜头／透明度／尺寸，把全部项目转向总览可见态；反向再展开仍可重新选中。总览只绘制外壳，由同级场景绘制每个真实画布一次，未加载项目保留占位。
- 设计依据为 apple-design 的当前呈现值与空间连续性、emil-design-eng 的可中断变换；保留用户指定的225/30弹簧、180ms淡入、350ms局部缩放和快捷键动画，不采用技能中删除高频动效的通用建议。代码级 review-animations 检查保留减少动态效果及原始会话，无新增依赖／CPU截图路径；原型效果仍交用户验收。
- 新回归比较返回瞬间真实标题、末帧与落定后的网格位置；具体断言A/B不重复、C占位存在、A/B末帧可见，且原viewport／80×24 PTY不变。最终桌面38项通过，App check、Release、行数／禁止allow／性能／视频壁纸护栏与diff检查通过；check／Release保留31项既有warning。日志前缀 `g1/project-return-`；构建身份 `project-return-build.json`，隔离App完成ad-hoc strict验签。未提交、推送或更新用户常驻App／试用DMG。
- Computer Use 启动本次Release（PID40451），完成工作空间进入、项目展开及两次快速切换→返回总览；真实卡片与Dock终态正常。`project-return-motion.log`确认同级动画分别在22.26ms／50.83ms被返回打断，随后在933.44ms／932.26ms落定。两次首张截图均约1.1s才返回，`project-return-frames.json`与`project-return-motion.json`保留实际请求／返回时间，不能据此判断前半段动画还原。日志出现最高51.06ms的显示回调间隔，尚无应用归因及正式CPU/GPU/呈现采样，不判性能通过。
- 下一步仍是删除跨层pending_navigation的分段等待，保留当前位置与速度直接转向；工作空间和项目之间的坐标转换必须覆盖真实绘制，不能仅删除等待条件造成跳帧。Browser原生交互、窗口尺寸变化中的同级镜头、输入／生命周期及S1–S4性能仍待验证。G0–G5状态不变，Goal保持ACTIVE；原型由用户验收。

### 2026-10-02：项目画布直接往返目录与中途改目标

- 项目返回目录不再先播放项目收起、再播放工作空间收起。将真实项目总览内容矩形投影到工作空间目录的内容区，现有弹簧直接以该嵌套矩形为目标；保持当前镜头位置、尺寸和速度。反向直接恢复原项目；返回途中选择当前工作空间直接改为项目总览，避免重载和隐式展开。
- 目录与项目渲染使用同一投影来源；运动中的空间不另画一份真实卡片，真实项目层裁剪到所属空间内容区。中途改选同级项目保留目录坐标和已有镜头，不丢弃外围场景。不增加截图、纹理缓存、依赖或另一套动画时钟。
- TDD：`g1/direct-directory-red.log` 记录原来的两段导航断言失败；`direct-directory-workspace-red.log` 记录中途选择当前空间仍排队；`direct-directory-sibling-red.log` 记录新目录投影在同级改目标时被丢失。均已修复。新增 Root／真实 Window::draw 测试检查真实终端标题首帧、中途、末帧位置（容差为一个设备像素），反向速度与惯性、无重复缩略／导航队列、同级改目标和原节点／viewport／80×24 PTY 保留。
- 最新 `cargo test -p app --lib desktop_ -- --test-threads=1` 为 39 passed；`cargo check -p app` 通过（31项既有warning）。文件行数、禁止allow、性能护栏、视频壁纸护栏及diff检查通过。最新命令日志为 `g1/direct-directory-desktop.log` 和 `direct-directory-check.log`；旧e2e未运行，历史全量测试不计为本轮结果。
- Release构建通过（31项既有warning）。通过Computer Use正常退出旧隔离PID40451后，确认进程已消失，再更新既有 `Norma Workspace Entry Review.app` 并通过ad-hoc strict验签；未替换用户常驻App或之前交付的DMG。构建身份见 `g1/direct-directory-build.json`，App主程序SHA256为 `58a32a652e222af75581d1d768a656379854c1bba89413314b52a152d82bd19e`。
- 新隔离PID74757启动后，两次Computer Use连接分别在9.624秒、16.726秒超时；保留同一存活进程，未因观察超时重启。`g1/direct-directory-startup.sample.txt` 中主线程794个样本停在启动通知回调的 `open → __open`，本次未定位具体文件或最终权限原因。没有新版真实窗口、输入或动画证据，不用旧构建截图或本轮headless测试替代。下一轮先检查该PID是否恢复，同时可继续尚未完成的开发；本轮有实际代码／测试进展，Goal保持ACTIVE。
- 余项：不同工作空间之间的导航，以及工作空间镜头运动中直接选择项目仍有旧排队逻辑；后续继续移除并验证坐标和速度转换。Browser、窗口改变期间的同级镜头、输入／生命周期、S1–S4性能仍未验收；本轮局部修复不改变G0–G5关卡状态。原型还原继续由用户验收。
