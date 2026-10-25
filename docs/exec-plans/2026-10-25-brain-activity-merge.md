# 幕僚活动合并自主处理,补触发信息,并持久化 7 天

状态:T1 到 T7 已实施,真实 App 验收见文末 §8。视觉与交互以原型为准:[`docs/prototypes/norma-brain/brain-activity.html`](../prototypes/norma-brain/brain-activity.html)。背景设计见 [`2026-10-23-norma-brain-acp.md`](../design-docs/2026-10-23-norma-brain-acp.md)。

## 1. 目标与非目标

目标:
- 左抽屉移除“Norma 自主处理”栏,所有自主动作并入幕僚活动,与对话轮次按时间混排。
- 每行显示绝对时刻、相对时间、触发源;轮次显示用时;动作与产生它的轮次互相跳转。
- 幕僚活动持久化,保留最近 7 天,重启后仍在;提示文案不再写“重启后清空”。
- 抽屉里留一个“幕僚活动”入口,带上次打开后新增的动作数。

非目标:
- 跨设备同步、手机端展示、导出。
- 旧版内存记录的迁移(没有持久化过,无从迁移)。
- 全文搜索(原型只有项目与触发源筛选)。

## 2. 现状核对(已读代码)

| 需要 | 现状 | 结论 |
| --- | --- | --- |
| 轮次数据 | `brain::acp::trace::AcpTrace{at, lane, turn, item}`;`TraceBuffer`(`brain_host/trace.rs`)内存,2000 条或 2MB 封顶;`BrainStatus::Trace` 在 `brain_host.rs:316` 入缓冲 | 持久化从这里旁路写入 |
| 动作数据 | `AutoLog`(`brain_host/log.rs`)内存,`MAX_LOG = 20`;写入点 `ops.rs:167`、`ops.rs:250`;`settings.rs:215` 清除 | 并入同一持久层,去掉 20 条上限 |
| 概览读取 | `brain_overview().actions`,渲染在 `drawers/overview_brain.rs`,分组入口在 `overview_list.rs` | 删除该分组与渲染,动作改由幕僚活动读 |
| 面板 | `drawers/brain_trace.rs`(40 张卡封顶,`ago()` 超 60 秒一律“稍早”);数据由 `brain_trace_cards` 聚合各项目会话 | 改为统一时间线;时间格式换成绝对加相对 |
| 触发源 | `AcpTrace` 没有这个字段;`Lane` 只有 `Main` / `Approval`;触发来自 `BrainEvent`(如 `AgentFinished`)经 `router` 变成 `Action` | 需要在 brain 层补,见 §4 T1 |
| 存储 | `storage` 里有 app 级 `app.sqlite`(`open_app_connection`、`migrate_app_db`)和项目级库;`mobile_activity_store.rs` 是 app 库里按 `new(path)` 每次开连接的写法 | 幕僚活动聚合所有项目,放 app 库 |
| 边界 | `storage` 不依赖 `app`/`runtime`/`ui`;`brain` 不依赖 `storage`;`scripts/check-storage-boundaries.sh` 禁止热路径直接碰 SQLite | brain 只产出数据,`app` 经后台线程调用 `storage` |

## 3. 设计决策

1. **存哪里**:app 库 `app.sqlite`。幕僚活动是桌面级视图,跨项目聚合,不属于某个项目库。
2. **两张表**,不把动作塞进轮次表:
   - `brain_turns(launch_id, area_id, lane, turn, started_at_ms, trigger_kind, trigger_who, finished_at_ms, stop_reason)`,主键 `(launch_id, area_id, lane, turn)`。
   - `brain_trace_items(launch_id, area_id, lane, turn, seq, at_ms, kind, title, text)`,外键指向轮次;展开时才按轮次读正文。
   - `brain_actions(id, launch_id, area_id, at_ms, kind, agent, node_id, summary, reason, ask_key, via_lane, via_turn)`。
3. **`launch_id`**:`turn` 在每次启动后从头计数,重启后会与旧记录撞号。每次启动生成一个 `launch_id`(启动时刻毫秒),列表里的“第 N 轮”继续只显示 `turn`。
4. **7 天保留**:按 `at_ms` 删除超过 7 天的行。启动时清一次,之后运行中每 6 小时清一次;删除在后台线程做。另设体积兜底:`brain_trace_items` 总文本超过 32MB 时从最旧开始删,防止长提示词撑爆库。
5. **写入不碰前台**:前台只把条目发进一个有界通道;一个后台线程每 500ms 或攒满 50 条批量事务写入。通道满时丢最旧的非动作条目并计数,动作条目不丢。关闭 App 时尽力冲刷,不阻塞退出超过 1 秒。
6. **读取**:打开面板时在后台线程读最近一页(默认 100 条,不再是 40 张卡),前台只更新状态。滚动到底再读下一页。内存里只保留已打开页,不再整体常驻 `TraceBuffer`;当前运行的新条目同时进入内存尾部,保证实时刷新。
7. **隐私**:库在本机用户目录,与现有 app 库同权限。记录含提示词与命令,**不得进入 `analytics`**,不得经手机推送中继发送。设置页的“清除”按钮同时清空持久记录。
8. **触发源枚举**(写入时固定,不存自由文本):`AgentFinished`、`ApprovalRequest`、`UserDispatch`(你批准提议)、`UserMessage`(你的指令或插话)、`Scheduled`(空闲巡检)、`Startup`、`Unknown`。`trigger_who` 存相关 Agent 名。
9. **UI 原则**(Apple HIG + 项目约束):内容优先、点行直接展开、无装饰动效;跳转高亮是 0.9 秒的一次性描边,减少动态效果时改为静态描边;所有可点元素有 hover、按下、键盘焦点态;面板仍是只读加 Esc 关闭。

## 4. 实施切片(每片先写失败测试)

**T1 brain:触发源与用时(`crates/brain`)**
- 测试:`Action` 经 `router` 产生时携带 `Trigger`;各 `BrainEvent` 映射到对应枚举;空闲巡检为 `Scheduled`。
- 实现:新增 `Trigger{kind, who}`,随 `AcpTrace` 的轮次开始条目和轮次结束条目送出;轮次结束带 `stop_reason` 与起止时间。保持 brain 不依赖 `storage`。

**T2 storage:表、迁移与保留(`crates/storage`)**
- 测试:插入再按页读取;同一 `launch_id` 内轮次号重复被主键拒绝、不同 `launch_id` 可同号;`prune(now)` 只删 7 天前的行且同步删除其条目;体积兜底删最旧;重复迁移幂等。
- 实现:`brain_activity_store.rs`,在 `migrate_app_db` 里建表,沿用 `mobile_activity_store` 的每次开连接写法。单文件控制在 500 行内,必要时拆 `brain_activity_prune.rs`。
- 更新 `scripts/check-storage-boundaries.sh` 的热路径名单,确认新 Store 不在渲染与输入路径里被引用。

**T3 app:后台写入器(`crates/app/src/app/brain_host/`)**
- 测试:批量写入合并事务;通道满时丢弃计数但动作不丢;退出冲刷在限时内返回;`launch_id` 每次启动不同。
- 实现:`activity_sink.rs`。`drain_brain_status` 的 `Trace` 分支与 `ops.rs` 两处 `log.push` 改为同时发往写入器。动作写入时带上产生它的轮次(`via_lane`、`via_turn`);`AutoLog` 的 20 条上限与 `overview` 的 `actions` 字段删除。

**T4 app:读取与统一时间线(`brain_trace_state.rs`、新 `activity_feed.rs`)**
- 测试(纯函数):轮次与动作按 `at` 倒序合并;按天分组为今天、昨天、日期;筛选项目、触发源、类型、需要你;动作能找到所属轮次;分页边界不重复不遗漏;相对时间 `<1 分`“刚刚”,其后按分钟、小时、天。
- 实现:后台读页、前台只写状态;`ago()` 替换为带绝对时刻的格式化函数。

**T5 ui:面板重做(`drawers/brain_trace.rs` 拆分)**
- 原型对应:时间列加触发标签;动作行紫色竖条;“需要你”橙色竖条;“来自第 N 轮 ↗”跳转并展开高亮;“产生 N 个动作”徽标;类型 / 项目 / 触发源筛选。
- 动作行保留“为什么”“以后这类先问我”“停止该 Agent”,逻辑沿用 `overview_brain.rs` 现有的 `brain_ask_first`、`brain_stop_agent`,只搬渲染。
- 文案放 `crates/ui/locales/chrome.zh-CN.json`,删除“重启后清空”,`chrome.brain_trace_earlier` 作废。
- 文件超 500 行就拆:`brain_trace.rs`(外壳与筛选)、`brain_trace_row.rs`(行渲染)。

**T6 抽屉:移除分组并加入口**
- 测试:`desktop_overview_layout_tests.rs` 里分组计数不再包含自主处理;入口数字 = 上次打开后新增的动作数,打开后清零(只存内存即可,重启后以最近一次打开时间为界,该时间存 `SettingsStore`)。
- 实现:删 `OverviewGroup::Brain` 及 `overview_brain.rs` 中被搬走的部分;提议(proposals)的“批准”仍留在概览,它不属于自主动作。

**T7 设置与清理**
- “清除幕僚活动”(`brain_host/settings.rs`)改为同时清库。
- 设置页文案说明保留 7 天。

## 5. 验收标准与证据映射

| 标准 | 证据 |
| --- | --- |
| 抽屉没有“Norma 自主处理”栏,有带数字的入口 | `desktop_overview_layout_tests`;`scripts/desktop-e2e.sh` 对应模块场景 |
| 面板同时显示轮次与动作,按时间倒序 | T4 纯函数测试;真实 App 截图 |
| 每行有绝对时刻、相对时间、触发源;轮次有用时 | T1、T4 测试;真实 App 逐行核对 |
| 动作与轮次互相跳转 | T4 查找测试;真实 App 点击后看展开与高亮 |
| 重启后记录仍在 | T2 读写测试;真实 App:产生记录、退出、重开、面板仍有 |
| 超过 7 天的记录被清除 | T2 `prune` 测试(注入时间) |
| 前台路径无 SQLite | `check-performance-guardrails.sh`、`check-storage-boundaries.sh` |
| 浅色与深色都可读 | 真实 App 两种外观各一张截图 |
| 减少动态效果下跳转高亮为静态 | 单元测试(状态选择)加真实 App 检查 |

必跑检查:`cargo test -p brain -p storage`、`cargo test -p app`(受影响模块)、`cargo check -p app`、`bash scripts/check-max-file-lines.sh`、`bash scripts/check-no-allow.sh`、`bash scripts/check-storage-boundaries.sh`、`bash scripts/check-performance-guardrails.sh`、依赖边界 `cargo tree -p brain | grep -iE "gpui|runtime|storage|agent "` 无输出。

## 6. 风险与取舍

- **体积**:提示词全文持久化可能很大。已用 7 天加 32MB 双重上限;若实测仍偏大,下一步是对 `Prompt` 条目只存前 4000 字(面板现在也只显示 4000 字)。
- **写入丢失**:崩溃时最后不到 500ms 的批可能丢失,可接受,这是审计视图而不是账本。
- **多窗口**:幕僚活动由桌面外壳聚合各项目会话,写入器只在外壳持有一份,避免重复写入;测试覆盖“两个项目会话各产生轮次”。
- **触发源历史**:升级前没有持久化,不存在缺触发源的旧行;之后若新增触发类型,枚举加 `Unknown` 兜底。
- **工作量**:T1 到 T3 是数据层,T4 到 T6 是界面,建议按顺序合并,T1 到 T3 完成即可先让记录持久化。

## 7. 待补的文档

实施完成后同步更新:`docs/design-docs/2026-10-23-norma-brain-acp.md` 的 F、G 节、`docs/prototypes/norma-brain/README.md` 中“Norma 自主处理栏”的描述、`docs/references/crate-file-map.md`。

## 8. 真实 App 验收(desktop-e2e)

- `scripts/desktop-e2e.sh brain-activity`:预置 7 天内外的记录,验收打开面板、筛选(自主动作 / 需要你 / 全部)、展开正文、“来自第 N 轮”跳转(清筛选、展开、滚到行首)、未读数清零,以及退出重开后 7 天内的记录仍在、8 天前的已被启动清理删除。夹具 `Fixtures/brain/BrainActivityFixture.swift`,视觉清单 `checks/brain/BrainActivity.checks.md`。
- `brain-autonomy` 与 `brain-propose` 已改为在幕僚活动面板里验收派发记录;抽屉不再有“Norma 自主处理”栏,只有带未读数的“幕僚活动”入口。
- 面板与入口的截图已逐张核对:无重叠或错位;动作行与轮次行的时间列对齐(所有行都留 3px 左边框)。
