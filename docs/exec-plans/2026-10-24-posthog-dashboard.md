# PostHog 看板设计：Norma 使用概览

> 日期：2026-10-24　状态：待在 PostHog 中创建
> 前置：[接入方案](2026-10-24-posthog-analytics.md)、[事件字典](../references/analytics-events.md)
> 区域：US（`https://us.i.posthog.com`）；组织 `NormaOS-Desktop`；不区分 dev/prod，单项目。

## 目的与角色

| 角色 | 看什么 | 做什么决定 |
| --- | --- | --- |
| 运营 | 看板 1、2 | 拉新是否起量、新用户卡在哪一步、哪批用户留得住 |
| 产品 | 看板 2、3 | 优先适配哪个 Agent、哪些功能该推或砍、多 Agent 卖点是否成立 |
| 研发 | 看板 3、4 | 版本发布后成功率有无倒退、旧版本剩多少 |

## 指标口径（所有图统一）

- **用户** = PostHog `distinct_id`。授权前是匿名安装 ID，授权后是 License Key 的加盐哈希；没有账号体系，所以这里的“用户”约等于“一台 Mac / 一个授权”。
- **活跃** = 当天有任意事件（排除 `$identify`、`analytics_pipeline_check`）。不能只用 `app_session_started`，因为 App 常驻不重启时当天不会再触发。
- **激活** = 首次出现 `agent_task_completed` 且 `success = true`。理由：Norma 的价值是协调 Agent，只启动或只建 Terminal 不算用上。
- **测试数据过滤**：所有图排除 `distinct_id = norma-pipeline-probe`；之后若加入 `build_channel` 属性，再排除开发构建。
- 时区：项目时区，周起始为周一。

## 看板 1：增长与激活（每周）

| 图 | 类型 | 配置 |
| --- | --- | --- |
| 新增安装（日/周） | Trends，唯一用户 | 事件 `app_session_started`，只计用户首次出现（first time for user），按日/周 |
| 激活漏斗 | Funnel，有序，窗口 14 天 | `app_session_started` → `agent_card_created` → `agent_task_started` → `agent_task_completed`（success=true） |
| 激活率 | 公式 | 首次成功任务的用户数 ÷ 新增安装用户数，按首次出现周 |
| 激活用时 | Funnel 的 Time to convert | 首次启动 → 首次成功任务，看中位数与分布 |
| 版本激活率 | Funnel 按 `app_version` 拆分 | 同激活漏斗，breakdown = `app_version`（取 `app_session_started` 上的值） |

## 看板 2：活跃与留存（每周）

| 图 | 类型 | 配置 |
| --- | --- | --- |
| DAU / WAU / MAU | Trends，唯一用户 | 活跃定义见上，分别按日、周、月 |
| 粘性 DAU÷MAU | 公式 | DAU 的 30 天均值 ÷ MAU |
| 周留存 | Retention | 起始事件 `app_session_started`（首次），回访事件 = 任意活跃，周粒度，12 周 |
| 激活 vs 未激活的第 7 天留存 | Retention 按 cohort 拆分 | cohort A = 已完成成功任务的用户，cohort B = 其余；对比两条曲线，验证激活定义是否真和留存相关 |
| 人均每周活跃时长 | Trends，聚合 | 事件 `app_backgrounded`，`active_ms` 按用户求和，周粒度，取分布（P50/P90）；单位换算为分钟 |

## 看板 3：功能与 Agent 价值（每月/按需）

| 图 | 类型 | 配置 |
| --- | --- | --- |
| 各 Agent 使用人数 | Trends，唯一用户，柱状 | 事件 `agent_card_created`，breakdown = `agent` |
| 各 Agent 任务数 | Trends，总次数 | 事件 `agent_task_started`，breakdown = `agent` |
| 任务成功率 | 公式 | `agent_task_completed`(success=true) ÷ `agent_task_completed`，breakdown = `agent`，周趋势 |
| 任务耗时分布 | Trends，总次数，堆叠 | 事件 `agent_task_completed`，breakdown = `duration_bucket` |
| 多 Agent 用户占比 | Trends 公式 | 近 30 天创建过 ≥2 种 `agent` 的用户 ÷ 创建过任一 Agent 的用户 |
| 功能渗透率 | Trends，唯一用户 | 分别统计 `terminal_node_created`、`preview_card_opened`、`browser_card_created`、`idea_created`、`feature_used`(feature=voice/wallpaper) 的 30 天唯一用户 ÷ MAU |

## 看板 4：健康与信任（每月）

| 图 | 类型 | 配置 |
| --- | --- | --- |
| 关闭统计占比 | 公式 | `analytics_opted_out` 唯一用户 ÷ MAU；趋势上升说明隐私上有反感 |
| 版本分布 | Trends，唯一用户，堆叠 | 活跃用户按 `app_version` 拆分，看升级速度与旧版本残留 |
| 授权激活成败 | Trends，总次数 | `license_activated` 按 `success` 拆分（仅开授权门的构建有数据） |

## 订阅与告警

- 任务成功率周环比下降超过 10 个百分点 → 通知研发。
- DAU 周环比下降超过 20% → 通知产品与运营。
- 新版本发布第 3 天复查：看板 1 的版本激活率、看板 3 的该版本成功率。

## 现有数据的缺口

下列看板项在补齐事件前不准确或为空：

1. 没有 `app_first_launch`：新增安装靠“首次出现”推算，卸载重装会被算成新用户。
2. 没有获客渠道：看不出用户来自官网、GitHub 还是推荐；需官网下载链接带来源并在首启上报。
3. 没有错误信号：Agent 启动失败、PTY 异常、上次崩溃均未上报，成功率会偏乐观。建议新增 `agent_launch_failed`、`app_crashed_last_run`。
4. 任务只有开始和结束：看不出中途放弃，建议新增任务中断事件。
5. `onboarding_step_completed` 等事件未接入，激活漏斗前两步靠 `agent_card_created` 间接推。
6. 没有构建渠道属性：无法排除开发构建的数据；建议所有事件统一带 `build_channel`（dev/release）。
7. 发出的正式包默认不开授权门，所以没有“注册用户”概念，只有活跃安装；开授权门后，授权用户会在 PostHog 里表现为已识别用户。

## 创建约束（给执行者）

- 只新建看板与图，不修改项目设置，不删除任何已有内容。
- 看板名称：`Norma 使用概览`；四组图用 PostHog 看板里的文本卡片作分区标题。
- 每张图的描述里写明口径（如“活跃 = 任意事件”），避免日后误读。
- 无法在 PostHog 中表达的公式或口径，不要近似乱建：保留一个文本卡片说明并在结果里列出。
