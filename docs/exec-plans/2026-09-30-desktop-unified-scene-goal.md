# Goal：桌面统一场景、GPU 合成与连续画布重构

状态：ACTIVE。用户处理后，同一隔离 App（PID 69255）窗口与输入已恢复，启动阻塞不再成立；尚无冷启动根因修复结论。G0 已有八节点历史基线、两路输出与三轮 CPU 占用记录；真实有界诊断收到 7,268 组 CPU／GPU／呈现回调，其中 2,144 条 presentedTime 为零，不能据此判呈现达标。当前多一个空工作空间，总览仍复现 session gone，正式固定负载与连续动画证据待补。G0–G5 均未通过。更新：2026-10-01。

**用户修订（2026-10-01，外观仍待实施与验收）**：工作空间与项目总览窗口以 [24 号用户参考图](../design-docs/2026-09-29-desktop-workbench-concepts/24-overview-glass-windows-reference.png)作为最新外观基准；创建 Agent 直接使用所属项目区域已选定的目录，不再弹出 Git worktree 选择或确认。Agent 默认目录已完成代码修复、16 项桌面单测及三种入口的真实 App/cwd 检查，详见 G1 observations；其他外观和统一场景要求仍未完成。

**交互修订（2026-09-30）**：按用户指令移除桌面顶部整条导航横栏及其占位；保留原 Canvas Dock。新增六个可配置导航快捷键，定义见 `docs/references/keyboard-shortcuts.md`。采用 Apple 内容优先原则，将导航交给直接操作与键盘；切换顺序稳定，焦点留在当前窗口。

**测试方式修订（2026-09-30）**：按用户最新指令停用 `crates/e2e`，不再新增、维护或执行旧套件，历史结果不计入验收。改用域层/GPUI 单元测试、真实 PTY 集成测试和 Computer Use 操作隔离真实 App。G0–G5、USC-01–13、原型还原和性能门槛保持不变。

执行 Issue：[GitHub #73](https://github.com/multisoul-ai/Norma-AI-OS/issues/73)，标签 `ready-for-agent`，含此前发布的计划副本；2026-10-01 修订以本地文档为准，本次未同步远端 Issue。

- 实施 worktree：`/Users/alan/Documents/mybook/norma-ai-os-desktop-workspaces`。
- 分支：`codex/desktop-multi-project-workspaces`；计划核对时 HEAD：`b21b32a1`，存在大量未提交改动，HEAD 不代表全部现状。
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

参考 `apple-design`、`emil-design-eng`、`improve-animations`，最终使用 `review-animations`；上述原型参数优先于技能中的通用默认值。写 GPUI 前加载对应 `gpui-*` 技能，并以当前钉死版本的代码为准。

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
| G2 统一布局与导航 | TODO | G1 通过 | 同源布局，直接转向，取消旧分段交接；按 24 号参考统一两级窗口外观 |
| G3 绘制与资源调度 | TODO | G2 | 可见性剔除、精确失效；按需纹理预算与回收；压力证据 |
| G4 完整交互与回归 | TODO | G3 | Agent 默认项目目录创建；所有卡片、IME、快捷键、持久化／删除／背景会话回归 |
| G5 真实验收与清理 | TODO | G4 | Release、所需 crate 测试、性能对比、独立 UI 验收通过，移除旧链路 |

### G0：冻结可复现基线

1. 核对当前工作树与未提交差异；保存二进制 SHA-256、HEAD、diff 摘要、macOS／机器／显示器／窗口尺寸。不能只用 Git SHA 标识脏工作树构建。
2. 在独立数据目录准备两个工作空间、各两个项目、各两张真实卡片，共 `2 × 2 × 2 = 8` 张；其中两张持续输出。一组让不同空间的同一项目使用同一 worktree。
3. 用 Computer Use 录制目录 → 工作空间 → 项目 → 反向 → 直接切换的基线；记录壁纸、Dock、标题、卡片几何、输入与会话情况。
4. 对连续转向的纯状态机补可控时间的单元测试，以真实 App 连续画面证明导航行为。按 USC-01 至 USC-13 映射证据，不再使用旧 E2E 操作入口。
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
6. 通过底层单元测试和真实 App 连续画面验证用户可见连续性；输入、运行实例和归属的验收标准不得降低，不维护已停用的旧套件。

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

退出：USC-03、04、05、09、10、12 通过，替代测试及真实 App 无未解释失败。

### G5：真实验收、删除旧链路并交付

1. 删除废弃预览绘制、过渡叠层、仅服务旧交接的导航队列和临时验证开关；保留有用的可选诊断。
2. 完成第 8 节规定检查并构建最终 Release；更新隔离 App 二进制、签名和构建身份，不能验收旧产物。
3. 用 Computer Use 完成第 6 节每条可见场景，记录连续画面和性能数据；用浏览器对照原型时按用户要求优先 Jev。
4. 按此前用户要求启动一个只读 UI 验收 subagent，对照 23 号 HTML 的动效与 24 号参考的窗口外观独立审查；不要让多个 agent 同时操控真实 App。可以由主 agent 操作并提供连续证据，审查不足时安排串行复验。
5. 有跳变、卡片替身、壁纸／Dock 回归、输入错位或大部分布局未还原，必须打回；修复后复验受影响路径。

退出：所有完成定义成立，用户可运行的产物及报告齐全；才能将新 Goal 标记 complete。

## 6. 验收入口与证据目录

业务流程通过隔离真实 App 的操作、连续画面及会话回执验证；域层与 GPUI 单元测试验证变换、裁剪、缓存、路由和生命周期，真实 PTY 集成测试验证进程与输入输出。不得把旧 harness 搬到别处继续运行，也不得只证明两套内部坐标互相一致。

| SPEC | 复用的主要先例 | 补充真实证据 |
| --- | --- | --- |
| USC-01、02、06、08 | 镜头／布局状态机单元测试 | 进入、返回、跨项目／空间、中途转向、搜索、滚动、resize 连续帧 |
| USC-03、04、05 | runtime 输入路由单元测试＋terminal 真实 PTY 集成测试＋Agent 创建入口聚焦测试 | 默认项目目录创建、无 worktree 弹窗、实际 cwd、同会话输入回执、中文 IME／选择／粘贴 |
| USC-07、09、10 | 桌面 Dock／壁纸／项目菜单与既有 Browser、快捷键测试 | 对照 24 号两级窗口材质与空／选中状态，静态／视频壁纸、原生视图、键盘、减少动态效果 |
| USC-11、13 | 现有帧诊断与 CPU benchmark 入口 | 帧时间、呈现掉帧、CPU／GPU、RSS、缓存字节与淘汰 |
| USC-12 | runtime／storage 生命周期测试＋真实 App 恢复 | 删除／退出后目标 PTY 回收，其他空间继续输出 |

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
- [ ] USC-01 至 USC-13 全部有对应证据，输入／会话／归属正负断言通过。
- [ ] 对照原型的连续画面通过，独立 UI 审查未留下阻断问题。
- [ ] 性能目标有 Release 真 App 测量，缓存／内存有界，无 CPU 逐帧截图上传。
- [ ] 替代测试与真实 App 的输入／恢复／生命周期回归通过；历史 E2E 结果不计入验收。
- [ ] 原型动效、真实卡片、壁纸、半透明样式、Canvas Dock、IME 与 Browser 均验收。
- [ ] 工作空间与项目总览窗口符合 24 号参考；展开无外围框，返回全程外壳与真实内容连续恢复，空窗口风格一致。
- [ ] 所有 Agent 创建入口默认使用目标项目区域目录，无 worktree 选择或确认；真实 cwd、同目录并行、异步导航／删除的归属与失败路径均验收。
- [ ] 废弃渲染／导航链路已移除，iOS 与业务身份／存储没有意外变更。
- [ ] 最终结果报告注明可运行 App 与证据路径；只有此时将新 Goal 标记 complete。

## 10. 可直接创建 Goal 的提示词

```text
在 /Users/alan/Documents/mybook/norma-ai-os-desktop-workspaces 工作树、codex/desktop-multi-project-workspaces 分支中实施桌面统一场景重构。

唯一实施计划：docs/exec-plans/2026-09-30-desktop-unified-scene-goal.md。
规格：docs/product-specs/2026-09-30-SPEC-desktop-unified-scene.md。
工作空间／项目／卡片与 worktree 归属规则继续遵守 docs/product-specs/2026-09-29-SPEC-desktop-multi-project-workspaces.md。
动效与空间关系基准：docs/design-docs/2026-09-29-desktop-workbench-concepts/23-workspace-canvas-motion-prototype.html。
窗口材质与外观基准：docs/design-docs/2026-09-29-desktop-workbench-concepts/24-overview-glass-windows-reference.png（2026-10-01 用户修订）。项目、工作空间两级都采用轻薄半透明窗口、细边框、较小圆角、真实画布缩略内容与克制的选中反馈，空窗口一致；返回总览时外壳必须随内容连续恢复。保留用户当前壁纸。
旧计划已归档到 docs/exec-plans/archive/2026-09-30-desktop-workspace-canvas-rebuild.md，仅作历史证据，不能作为施工入口或已完成证明。

目标：保留 Rust／GPUI，将工作空间、项目和真实卡片重构为统一场景、独立布局与 GPU 合成。允许较大范围重构 app、canvas 和现有 GPUI 本地补丁，复用数据结构、卡片内容绘制、Agent 与 PTY。只改桌面端，不改变 iOS 或手机协议。保留当前未提交改动。

严格依计划 G0–G5 推进：先保存真实基线，再完成两项目真实卡片、输入、PTY、Browser、视频壁纸的纵向验证；通过性能和输入关卡后才整体迁移。纯镜头变化不触发全量内容重排或 PTY resize；直接切换和反向操作从当前视觉状态与速度继续。真实卡片统一渲染，复杂内容只在测量证明需要时加入有界 GPU 纹理缓存，不得使用假预览或每帧 CPU 截图读回。

保留原型的镜头弹簧和局部展开重排、壁纸、半透明样式、原 Canvas Dock；展开填满窗口安全边距与 Dock 之间，无外围边框和重复标题，不恢复额外 Norma 输入栏。工作空间切换发生在当前窗口；同项目跨空间独立卡片，同 worktree 允许并行，卡片不能跨项目或空间移动，后台 Agent 持续运行。

创建 Agent 时直接使用所属项目区域已选定的实际目录作为 cwd，不再弹 Git worktree 选择或确认；不自动创建／切换 worktree 或分支。普通目录与已选 worktree 一视同仁。入口没有目标项目时只选择／添加项目区域。覆盖 Dock、快捷键、项目菜单与自定义 Agent 入口，检查真实 cwd，并防止异步创建因导航或删除而投递到错误项目。桌面对象归属和既有卡片 cwd 不变。

按 AGENTS.md 先 TDD。用户已停用旧 crates/e2e，不得新增、维护、运行或以其结果验收。使用域层/GPUI 单元测试、真实 PTY 集成测试和隔离真实 App 覆盖 USC-01 至 USC-13；验证真实会话恢复，修复产品缺陷。按计划运行范围测试、最终跨 crate 测试、护栏和 Release 构建。

必须使用 Computer Use 操作隔离的真实 App，验证中间帧、连续快速切换、输入法、Browser、背景输出、资源回收；独立 UI 验收 subagent 只读审查且不能与主 agent 同时控制 App。大部分布局／动效未还原必须打回。使用固定负载采集真实 CPU／GPU、呈现掉帧、缓存／内存和输入延迟，未测不能宣称高性能。

持续更新该实施计划的阶段状态和证据路径。只有全部完成定义成立才能将 Goal 标记 complete；构建或 headless 通过不代表真实验收通过。不要自动提交、推送或替换用户常驻 App。
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
