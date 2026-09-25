# Cursor Mobile 设计参考（用户偏好：简洁）

来源：用户提供的 Cursor 手机端截图（2026-09-25）。  
用途：Norma iOS 伴侣「注意力优先指挥面」原型视觉与交互参照。  
偏好声明：**喜欢这种简洁设计**——大留白、细线图标、圆形按钮、列表少边框、底部一条指挥条。

截图目录：[`cursor-mobile/`](./cursor-mobile/)

| 文件 | 画面 | 可借鉴点 |
|------|------|----------|
| [`01-context-sheet.jpg`](./cursor-mobile/01-context-sheet.jpg) | Context 底页：缩略图条 + Mode/Add 分组列表 | 附加上下文用 sheet，不堵主屏；分组小标题全大写灰色 |
| [`02-live-activity-lockscreen.jpg`](./cursor-mobile/02-live-activity-lockscreen.jpg) | 锁屏 Live Activity：Working · 任务名 · +/−/Files · repo | **不打开 App 也能扫一眼**；状态一眼、指标极少 |
| [`03-inbox-composer.jpg`](./cursor-mobile/03-inbox-composer.jpg) | Inbox 上浮 Composer：项目/环境选择 +「Plan, ask, build...」 | 指挥入口是 **浮层/条**，不是占满首页的长聊天 |
| [`04-all-agents.jpg`](./cursor-mobile/04-all-agents.jpg) | All Agents：Working / Open PR 分组列表 + 底栏 | 舰队是 **分组列表二级页**，不是首页主舞台 |
| [`05-working-list.jpg`](./cursor-mobile/05-working-list.jpg) | Working 列表：极简单行 + Recents + 底栏 | 状态页大量留白；元数据一行灰字 |
| [`06-inbox-status-grid.jpg`](./cursor-mobile/06-inbox-status-grid.jpg) | Inbox：2×2 状态卡（Needs Attention / Working / …）+ Projects | **Needs Attention 有独立入口与计数**；Inbox 作仪表而非聊天墙 |

本 effort 自产草图（对照用）：

- [`../assets/norma-ios-home-q9-chief-redesign.png`](../assets/norma-ios-home-q9-chief-redesign.png)（若已拷贝）
- 仓库外：`/opt/cursor/artifacts/assets/norma-ios-home-q9-chief-redesign.png`

---

## 视觉语言（从截图抽出）

- **底色**：白 / 极浅灰；彩色只给功能图标（红/橙/蓝/紫少量）
- **字体**：粗标题 + 灰次要信息；无装饰性展示字体
- **形状**：大圆角卡片、圆形图标按钮、底栏 pill
- **分隔**：靠留白与细分割线，少描边盒子
- **密度**：一屏信息少；宁可下钻列表，也不堆仪表盘

---

## 信息架构对照 → 我们的 Q9=C

已锁定首页：**Norma 指挥条 + 闸门主舞台**。

| Cursor Mobile | Norma 伴侣（拟映射） | 注意 |
|---------------|----------------------|------|
| Inbox 状态格里的 **Needs Attention** | 首页主区「需要你」决策队列 | 计数为 0 时要平静，不要空热闹 |
| 底栏 / 上浮 Composer「Plan, ask, build...」 | 顶部或底部 **Norma 指挥条**（+ / 输入 / 麦） | 默认不胀成全屏会话 |
| All Agents / Working 列表 | 次级「舰队 / 在跑」下钻 | 对齐 Agent Chief：监视不是默认税 |
| Open PR 行上的 +/− · Files | 决策卡/摘要卡上的 **可扫指标**（可选） | 完成项进摘要，不抢主队列 |
| 锁屏 Live Activity | 仅 **interrupt 级**；**默认短下一步（A）**，权限/失败必带方案，普通提问可标题+极短线索（融 C）；禁止心跳刷屏 | 见票 [03](../issues/03-grilling-resume-cue-in-home-c.md) |
| Context sheet | 指挥条「+」展开：项目/附图/模式 | 保持分组列表，勿做成设置坟场 |

---

## 明确要学的交互

1. **一条指挥入口**：pill 输入条贯穿 Inbox / Agents / Working，用户心智稳定。  
2. **Needs Attention 一等公民**：和 Working / In Review 并列计数——我们把它升级为首页主列表。  
3. **锁屏极简状态**：Working + 标题 + 极少数字；适合「带着方案来」的短文案，不适合长日志。  
4. **舰队二级化**：All Agents 是下钻页，分组折叠（Working / Open PR）。  
5. **Context 用 sheet**：不抢注意力主路径。

## 明确不要照搬的

- 把 Inbox 做成多会话聊天时间线（与注意力闸门冲突）。  
- Live Activity / 推送为每一次「Working」心跳响铃（Chief：零信息 drop、完成进 digest）。  
- 首页 2×2 四卡若导致「点来点去监视」，则弱化 Working/In Review，只强化 Needs Attention + 摘要入口。

## 原型验收时对照本夹

做 [可点 HTML 原型](../issues/05-prototype-home-c-html.md) 时，评审清单：

- [ ] 留白与圆角接近 Cursor Mobile，而非密仪表盘  
- [ ] 指挥条形态接近 Composer/底栏，而非满屏聊天气泡墙  
- [ ] 需要你 / 摘要分区清晰；舰队不在首页占主面积  
- [ ] 若演示锁屏/动态岛，文案 ≤ 两行 + 可选一个方案短句  
