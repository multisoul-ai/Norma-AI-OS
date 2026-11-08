# Norma 灵动岛 2.0 实施计划

- 日期：2026-11-07
- 负责人：Henry
- 原型：`docs/prototypes/island-2/island-2.html`（权威），参考图 `docs/prototypes/island-2/reference/*.png`
- 基线：现有岛实现（`crates/app/src/island/`、`crates/runtime/src/island/`）及 `docs/exec-plans/2026-11-05-norma-dynamic-island.md`

## 1. 目标

把现有灵动岛升级到原型的样子，并用真实 App 的 desktop-e2e 证明“一比一还原”。不重做已完成的状态机、审批/问答、跨工作空间链路，只改表现层、几何和排序规则。

## 2. 与现有实现的差异（要做的事）

| # | 原型要求 | 现状 | 落点 |
| --- | --- | --- | --- |
| D1 | 四种头像轮廓视觉大小一致（圆/蛋/六边形/三角形按面积归一：缩放 1 / .98 / 1.06 / 1.10，圆与蛋不得有描边撑大） | 各轮廓大小不一 | `crates/runtime/src/island/bloub/profiles.rs`、`bloub.rs`、`app/island/bloub_view.rs` |
| D2 | 收起槽位分流：左槽“运行中”最多 3 个头像 + `+N`，右槽“待处理”最多 2 个头像（橙色脉冲环）+ 总数徽章；同一 Agent 不同时出现在两侧 | 左槽用首字母圆点，右槽只有数字 | 新增纯函数 `runtime::island::roster`；`app/island/view.rs` 收起行 |
| D3 | 收起宽度自适应：运行数 + 待处理数 ≥ 4 时 330 → 416pt，否则 330pt | 固定 330pt | `runtime/island/geometry.rs`（含单测） |
| D4 | 头像情绪映射：运行=Working、等你=Waiting、完成=Responded、排队=Idle 且 55% 不透明度 | 部分形态固定 Mood | `app/island/style.rs::avatar_mood` 及各调用点 |
| D5 | 完成通知：Bloub(30pt, Responded) + “X 完成了 …” + “查看成果”，底部 3 秒倒计时线 | 有通知，无倒计时线与头像 | `app/island/notify.rs`、`view.rs` |
| D6 | 审批卡下方“待处理队列”：待处理 ≥ 2 时显示可点头像，点击切换当前卡，处理后自动轮到下一个 | 无 | `app/island/pending.rs`、`navigation.rs` |
| D7 | 展开「运行中」栏顶部筛选（全部/运行中/等你/完成）+ 全体头像一览；行内头像 30pt | 仅分组列表 | `app/island/running.rs` |
| D8 | 展开头部 52pt（34 + 18），其余形态宽度 460 / 540 / 660 不变 | 已满足 | 回归即可 |
| D9 | 所有新增文案中英 i18n | — | `crates/ui/locales/`，`ui::i18n::text/format` |

## 3. 设计规格（还原依据，以原型 CSS 为准）

- 颜色：`island #000`、`fg #f4f6f8`、`run #5fcbe0`、`attn #ffa94d`、`ok #5ad38a`、`deny #ff7b7b`、`staff #a99bff`；provider 色沿用 `style.rs::avatar_mood`。
- 收起 34pt 高，刘海居中，槽位 padding 12pt；展开圆角 32，完成通知圆角 24。
- 头像尺寸：收起 20pt（间距 -2pt 重叠）、完成通知 30pt、审批卡 40pt、队列 22pt、列表 30pt、一览 20pt。
- 眼睛：工作=专注双胶囊，等你=一大一小，完成=笑眼，失败=八字；眨眼周期 4.4s。
- 动作：工作=呼吸 0.9s；等你=左右摇 ±8° 1.1s；完成=弹跳 1.1s。均复用 Bloub 引擎，每窗口合并重绘，不得因头像数量增加刷新率（沿用 30/15 fps 规则）。
- 动效须尊重系统“减少动态效果”。

## 4. 约束（来自 CLAUDE.md，必须遵守）

1. 在隔离 worktree 开发，不改主目录 `dev`；**只提 PR，不合入 dev**，等用户亲测。
2. TDD：D1–D3、D6、D7 的排序/槽位/宽度规则先写纯函数测试（红 → 绿），位于 `runtime`，不依赖 GPUI。
3. 禁止 `#[allow]`；单个 Rust 文件 ≤ 500 行（`view.rs` 现 483 行，新增逻辑必须拆出新文件，如 `roster_view.rs`、`queue_view.rs`）。
4. 前台不做 IO/LLM；不得在 render 路径上扫描磁盘或访问 SQLite。
5. 用户可见文案走 i18n，运行 `python3 scripts/check-i18n.py` 与 `python3 scripts/test-i18n.py`。
6. 交互控件需有 hover / pressed / focus / selected / disabled 状态。
7. 提交通过 pre-commit，不得 `--no-verify`。Cargo 命令用 `CARGO_INCREMENTAL=0`，先查磁盘空间。
8. 代码 review 使用独立 subagent 做对抗式审查并核验发现。

## 5. 里程碑

| 里程碑 | 内容 | 完成标准 |
| --- | --- | --- |
| M1 | D1 头像轮廓归一 + D4 情绪映射 | 轮廓面积比单测（四种轮廓两两比例 0.9–1.1）；排队态不透明度 |
| M2 | D2/D3 槽位分流与自适应宽度 | `roster` 纯函数测试：8 个 Agent（4 运行/3 待处理/1 排队）输出左 3+`+1`、右 2+总数 3；宽度 330/416 边界；收起形态真实 App 看图 |
| M3 | D5 完成通知 + D6 审批队列 | 队列切换、自动轮到下一个、3 秒倒计时（reduce motion 下无动画）测试；E2E 看图 |
| M4 | D7 展开「运行中」栏筛选与一览 | 筛选计数与列表一致测试；E2E 看图 |
| M5 | 全形态还原验收 | 第 6 节全部通过，证据与剩余缺口写入进度文档 |

每个里程碑单独 PR（可叠加分支，PR 描述注明依赖），不合入。

## 6. 一比一还原验收（E2E）

新增场景 `island-redesign`，放在 `test/desktop-e2e/Scenarios/island/IslandRedesign.swift`，在 `IslandModule.swift` 登记 `scenarioNames` 与 `visualScenarioNames`；看图清单放 `test/desktop-e2e/checks/island/island-redesign.checks.md`；夹具放 `Fixtures/island/`。必须在**真实 App** 上跑：`scripts/desktop-e2e.sh island-redesign --visual`，逐张读取原图，OCR 不可用属 ENV，不得退回手动或 Computer Use。

夹具：8 个 Agent（Claude/Codex/Cursor/OpenCode/Kimi 混合，覆盖四种轮廓），3 个工作空间，4 运行、2 等你（权限、问答）、1 完成待查看成果、1 排队；另有 2 个和 5 个 Agent 的变体。

每个形态必须同时满足下面三类证据，缺一不可：

1. **几何数值（硬指标，±1pt）**：从真实窗口读取岛 frame，收起 330/416、完成通知 460、审批 540、展开 660；头部高度 34 / 52；各头像 frame 尺寸与间距符合第 3 节。
2. **结构断言**：左右槽头像数量与 `+N`、徽章数字、队列头像数、筛选计数、标签页计数与夹具一致；同一 Agent 不同时出现在左右槽。
3. **逐张对照看图**：把真实截图与 `docs/prototypes/island-2/reference/<形态>.png` 并排，按 checks 清单逐项核对（布局、层级、颜色、圆角、头像轮廓与大小一致性、表情、文字、按钮状态）。不得以“图片存在”或内部状态代替结论。

| 形态 | 参考图 | 夹具 | 额外检查 |
| --- | --- | --- | --- |
| 收起 2 个 | `compact-n2.png` | 2 | 宽度 330 |
| 收起 5 个 | `compact-n5.png` | 5 | 宽度 416，3 运行 + 2 待处理 |
| 收起 8 个 | `compact-n8.png` | 8 | 左 3+`+1`，右 2 + 总数 3 |
| 完成通知 | `activity.png` | 8 | 3 秒后自动收回，倒计时线 |
| 审批提醒 | `alert.png` | 8 | 队列点击切换 |
| 展开·需要你处理 | `expanded-pending.png` | 8 | 三张卡片 |
| 展开·运行中 | `expanded-running.png` | 8 | 筛选切换后行数正确 |
| 展开·幕僚 | `expanded-staff.png` | 8 | 沿用既有行为 |
| 幕僚汇报中 | `speaking.png` | 8 | 声波 |

另需：中英文各跑一遍；浅/深外观；系统“减少动态效果”下头像与倒计时无动画；hover / pressed / focus / selected 的瞬时反馈用真实 App 捕获，不用几何单测冒充。

## 7. 通用验证命令

```bash
CARGO_INCREMENTAL=0 cargo test -p runtime -p canvas -p terminal
CARGO_INCREMENTAL=0 cargo test -p app island
CARGO_INCREMENTAL=0 cargo check -p app
CARGO_INCREMENTAL=0 cargo clippy -p app -p runtime --all-targets -- -D warnings
bash scripts/check-max-file-lines.sh && bash scripts/check-no-allow.sh && bash scripts/check-performance-guardrails.sh
python3 scripts/check-i18n.py && python3 scripts/test-i18n.py
scripts/desktop-e2e.sh island-redesign --visual
```

## 8. 已知缺口与不做的事

- 原型头像是 SVG 近似，真实实现用 Bloub 引擎，表情细节允许有差异，但轮廓、大小、配色、动作类型必须一致。
- 刘海屏、外接屏、全屏 Space、透明区点击穿透需要对应硬件；没有就如实标“未验证”，不得标通过。
- 不改审批/问答/跨空间链路与幕僚逻辑；不新增设置项。

## 9. 交付

- PR 链接、最新 head 的 CI（含 rust checks）状态、E2E 报告目录、逐形态“标准 → 证据 → 结果”表，写入 `docs/exec-plans/2026-11-07-norma-dynamic-island-2-progress.md`。
- 每个里程碑向用户简报一次；未通过项保留，不得写“只剩测试”。
