# Norma OS 桌面伙伴 · 高保真交互原型

入口：`index.html`。角色图片已内嵌，直接双击即可离线使用，没有外部字体、脚本或网络依赖。

```bash
python3 -m http.server 8794 --bind 127.0.0.1 --directory docs/prototypes/norma-desktop-companion
```

打开 <http://127.0.0.1:8794/>。底部切换三个方向，也可按 `1` / `2` / `3`、方向键，`R` 重放当前方向。输入时这些快捷键不生效。选中的方向保存在 URL 的 `?v=` 参数里，示例数据只存在内存中。

## 比较的设计方向

| 方向 | 主要取舍 | 适合场景 |
| --- | --- | --- |
| 身旁小窗 | 独立桌宠 + 紧凑工作窗口，占用较少；任务与对话需切页 | 平时安静陪伴，需要时打开 |
| 工作胶囊 | 常驻计数条 + 左任务右对话，信息持续可见；展开更宽 | 同时跟进多项工作 |
| 伙伴侧栏 | 高窗口与更鲜明的伙伴身份，长列表舒服；占据固定一侧 | 长时间对话和连续审批 |

三个方向共用同一份演示状态。切换布局不会清空对话、重置计数或重复派发。

## 可实际操作的流程

- 点桌宠开窗，收起后继续保留任务；拖动桌宠或窗口标题区域安放位置。
- 显示当前 Master Agent，切换 Norma / Simon / Alan。此处只模拟切换，不改变建议原本的派发目标。
- 发送文本消息，Norma 根据当前任务与审批数量回复进展；保留用户原文与对话历史。
- 查看运行任务、Agent、最近执行记录；模拟完成任务后，运行计数和对话回执同步更新。
- 查看幕僚建议的理由、目标和完整指令；批准后先禁用重复操作，成功才移除建议并加入运行列表。
- 忽略建议、派发失败后重试、处理完后的空态，以及重置/专注/完成四类演示场景。
- 摸摸、休息、叫醒、右键菜单；休息不改变后台任务的演示状态。
- 中英文、深浅主题、键盘焦点、减少动态效果、窄窗口以及顶边浮层避让。

这是浏览器里的设计原型，透明置顶、真实跨桌面、原生点击穿透、真实 LLM 对话、麦克风和任务执行尚未接入。示例操作不会调用真实 Agent 或修改项目。

## 品牌与角色生成

品牌来源：`docs/brand/norma-os-brand-guidelines.md`、`docs/brand/generation-brief.md` 以及 `docs/brand/assets/source-references/norma-character-reference.png`。用户明确要求 Q 萌桌宠；采用用户所附蓝发 Q 版图的比例与线条风格，保留 Norma 的固定身份：冷浅紫长马尾、白色 X 发夹、黑色蝴蝶结、蓝紫眼睛、海军蓝剪裁服装与蓝紫纹理。没有采用参考角色的女仆帽、女仆服或发色。

采用内置 imagegen 生成 `assets/norma-chibi.png`，并将同一图片内嵌到 HTML，生成提示词保存在 `assets/norma-chibi-prompt.txt`。它是独立的桌宠衍生资产，不替换正式品牌源图。此前的黑发 3D 探索未被采用，未加入此原型。

## 设计技能如何落地

- `prototype`：隔离于正式 Rust 代码，三个方向分别比较窗口布局、信息密度和常驻入口；底部选择器遵循指定样式。
- `apple-design`：信息先于装饰、对话/运行/审批分层、直接拖动、明确焦点和操作反馈；无需遮罩打断手头工作。
- `emil-design-eng`：高频切页和键盘操作即时更新，删除窗口重复入场动画；角色触摸反馈仅用 180ms 的 transform/opacity，减少动态效果时禁用位移。
- `frontend-design`：暖白、石墨、银灰为基础，品牌紫用于操作，Acid 用于待审批，Mint 用于正常状态。角色负责陪伴，工作内容保持清晰。
- `review-animations`：独立对抗式检查覆盖焦点丢失、重复动画、浮层越界、失败目标和主按钮遮挡；发现后修复并加入定向验收。

## dsh-pet 技术参考与 Norma 接入边界

参考仓库：<https://github.com/PC2005-cloud/dsh-pet>，阅读基准 commit `0b8a6ad98ad2bc2bee986cd027936a5eeb3accc8`。

| 已查看的实现 | 借鉴点 | 在 Norma 中的落点 |
| --- | --- | --- |
| `runtime/electron-helper/main/windows.js` | 无边框透明置顶窗口、跨工作空间、按交互区域调整鼠标穿透 | 后续由 GPUI/macOS 窗口适配承接；本轮只呈现浏览器内效果，不引入 Electron |
| `runtime/electron-helper/pointer-target.js` | 桌宠与气泡命中、菜单打开时保持交互 | 桌宠、窗口、菜单分别拥有命中区域，透明空白不应该吞掉桌面点击 |
| `runtime/electron-helper/sprite/playback.js` | 前后台双播放器、预加载完成后再切换、加载失败回退 | 后续角色素材预加载与替换；当前是单张原创 PNG 加触摸反馈 |
| `src/shared/work-status.ts`、`src/host/work-status.ts` | thinking / working / result / waiting / success / error，等待和错误优先 | 后续从现有任务与审批快照派生展示状态，不由桌宠产生另一套执行状态 |
| `src/client/pet/animation/next-animation.ts` | 按工作状态选动画，结束/异常时有明确回退 | 后续将 Norma 的 Thinking / Waiting / Complete / Error 映射到获批角色素材 |

上表的外部路径均相对 dsh-pet 仓库的 `dsh-pet/`。只借鉴技术组织，没有复制其实现代码、角色或动画。该项目 README 对素材标注非商业限制；代码 LICENSE 是 MIT，不能把二者视为同一许可。

正式接入优先复用：

- 对话与任务协调：`crates/agent/src/master.rs` 的 `NormaSession`。
- 建议与派发：`crates/app/src/app/brain_host/proposals.rs` 和 `crates/app/src/app/brain_host/proposal_actions.rs`。
- 快照与焦点：从现有 Runtime/工作空间获得状态；桌宠不拥有独立任务执行器。

视频/动态素材不能直接照搬 Web 播放路径到 GPUI。若采用透明视频，须单独验证 macOS alpha 解码和 Metal/IOSurface 低拷贝上屏，遵守仓库性能护栏；普通 420f 视频路径不代表已经支持 alpha。本轮不宣称这些原生能力已完成。

## 验收

```bash
node docs/prototypes/tests/norma-desktop-companion.cjs
```

使用现有 Playwright 与独立无头 Chrome，浏览器交互检查覆盖三个方向的对话、任务详情、批准/忽略、重复点击、失败重试、Master 切换、双语、减少动态效果、顶边浮层与窄窗口定位。截图写入 `evidence/`。没有修改 Rust/GPUI 生产代码，因此不以浏览器验收冒充真实 `.app` 验收。

2026-10-31 验证结果：上述浏览器验收 PASS，`git diff --check` PASS，原创 PNG 为 1368×1150 且包含 alpha。`python3 scripts/check-i18n.py` 未通过：`crates/app/src/settings/voice/view/gpt_live.rs:70` 既有 `GPT-Live` 硬编码；已核对 HEAD 中同样存在，本次未改该文件。

展示：本地 HTTP 返回 200，已通过系统浏览器打开。尝试 `norma present browser` 时，宿主返回 `unauthorized`（Provider 编排授权无效或已撤销），因此没有声称已经安装进 Norma 内容抽屉。
