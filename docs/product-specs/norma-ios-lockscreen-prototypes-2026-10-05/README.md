# Norma iOS 锁屏实时活动：五种视觉原型

日期：2026-10-05。状态：待用户选型。

这五张图使用内置 imagegen，根据用户提供的锁屏截图编辑生成，用于比较信息层级与状态呈现。Agent 状态、完成项和测试数均为示例；图片不是现有 App 截图或真实 App 验收证据。未修改应用代码。

| 编号 | 方向 | 图片 | 主要价值 |
| --- | --- | --- | --- |
| A | 一眼总览 | [A-overview.png](A-overview.png) | 运行人数、参与者、完成项与需人处理数量 |
| B | Agent 状态清单 | [B-agent-list.png](B-agent-list.png) | 逐行看到谁在做什么、实际完成数和等待原因 |
| C | 任务阶段轨迹 | [C-milestones.png](C-milestones.png) | 方案、实现、测试和验收的阶段关系 |
| D | 待决策优先 | [D-decision.png](D-decision.png) | 显示具体问题、谁在等待、谁继续运行 |
| E | 待验收优先 | [E-review.png](E-review.png) | 突出成果就绪、验证摘要和打开成果入口 |

## 建议的产品行为

默认采用 B；需要决策时切到 D，需要验收时切到 E。若更偏好轻量总览，可采用 A 作为默认。用户尚未确认选择。

- 进度显示真实阶段、完成项或测试数；探索任务无法可靠估算时显示当前动作与持续时间。
- Agent 较多时展示最相关的 2–3 个，并保留总数；具体聚合范围（项目、工作空间或全部工作）待选型后确认。
- 操作入口打开对应决策或验收详情；锁屏只承载足够做介入判断的摘要。
- 实现时增加最后同步时间；过期或离线时明确显示状态，不将缓存冒充最新进展。
- 生命周期以当前工作会话为界；任务完成后展示可配置的短暂结果摘要，再结束活动。

## 技术起点

采用 SwiftUI + WidgetKit 扩展 + ActivityKit，Mac 汇总任务事件，服务端通过 APNs 更新 Live Activity。优先推送阶段变化、待决策、待验收、失败与完成，合并重复进展。同步灵动岛的紧凑摘要和待处理数。

iOS 代码的聚焦检索目前未发现 `ActivityKit`、`LiveActivity` 或 `NSSupportsLiveActivities` 标识；这里给出新能力的设计起点，不视为已有实现的完整审计。

Apple 的 Live Activity 设计以可扫读为目标；锁屏高度超过 160 pt 可能被截断。第二轮生成已压缩卡片高度并增加与底部系统按钮的间隔，但图片不能证明实际 SwiftUI 尺寸、Dynamic Type、VoiceOver 或按钮命中区域，仍需在选型后的真机实现中验收。

- [Live Activities HIG](https://developer.apple.com/design/human-interface-guidelines/live-activities)
- [Displaying live data with Live Activities](https://developer.apple.com/documentation/activitykit/displaying-live-data-with-live-activities)
- [Starting and updating Live Activities with ActivityKit push notifications](https://developer.apple.com/documentation/activitykit/starting-and-updating-live-activities-with-activitykit-push-notifications)

## 设计依据与检验

Apple 设计技能落实到系统字体、清晰层级、单一主操作、克制材质和锁屏原生空间；项目 iOS 规范落实到深蓝正文、蓝色操作、薄荷运行/完成、粉色需人处理，并保留文字状态标记。

第一性原理判断：当前截图只有 Working 和代码改动量，无法回答哪个 Agent 正在执行、为什么需要人处理；用户已明确表达这三个观察目标。最小验证就是这五个方向：让用户短暂扫读后指出正在工作的 Agent、当前阶段和自己下一步要做的事，再选择默认布局。此处没有据此宣称已验证普遍用户需求。

已逐张目视检查中文、Agent 名称、数值、状态提示、整体空间和系统按钮间隔；复制时校验 PNG 签名和源文件哈希一致。未运行 App 或代码测试。

原始参考保存在 [reference-lockscreen.jpg](reference-lockscreen.jpg)。完整初始与最终编辑提示词、生成工具和图片路径保存在 [generation-prompts.json](generation-prompts.json)。
