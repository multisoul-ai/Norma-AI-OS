# 全深色主题系列

用户已改为仅保留深色主题，并明确偏好墨黑；该决策取代浅色主题 A/B 选择。保留当前布局与六主题目录。用户后续明确希望“更加透明一些”，抽屉主蒙层由原来的 179/255（约 70%）降低为 153/255（60% 不透明度），文字和终端材质独立保持。

## 方向

| 主题 | 基底 | 强调色 | 特点 |
| --- | --- | --- | --- |
| 墨黑 / Ink Black | #0C0C0E | #C4CCD9 | 默认；中性黑灰，银灰焦点 |
| 石墨灰 / Graphite | #17191C | #9AB8D3 | 较亮的灰黑层次，柔和雾蓝 |
| 暖曜黑 / Warm Obsidian | #151210 | #DDB780 | 暖黑底、香槟金，偏阅读 |
| 蓝宝石之夜 / Sapphire Night | #071324 | #54A2FF | 保留现有冷蓝 |
| 月岩 / Moonstone | #111117 | #C1B9D5 | 替换午夜紫，矿物黑与银灰微紫 |
| 深海石 / Deep Petrol | #0D1619 | #94BDC5 | 替换森林薄荷，蓝黑与磨砂灰青 |

Apple 设计原则落实：布局和操作位置保持，低饱和背景退居内容后；抽屉、卡片和终端由同一语义主题驱动；正文柔白、辅助字独立颜色，强调色用于选中和焦点。保持控件的按下、悬停和键盘反馈。

浅色主题不再在设置与命令目录中展示。原保存的 paper-blue / linen-sage / silver-violet 读取时迁移为 ink-black；旧 midnight-violet / forest-mint 读取时分别映射到 moonstone / deep-petrol；其余四个深色 id 保持原选择，未知值使用默认墨黑。历史浅色构造可留作兼容/极性回归夹具，不作为可选产品主题。

## 验收映射

- 目录六套均深色、默认墨黑、旧保存值迁移：ui 公开目录测试与 app 设置加载测试。
- 新中性色正文/终端可读、材质有层次：ui 亮度与对比测试。
- 宿主和项目同步、组件模式一致：现有主题传播与输入主题测试。
- 首页、左右抽屉、画布外框、终端、选中/焦点/悬停/按下：真实 App desktop-e2e。
- cargo check、GPUI release 和受影响护栏；HTML 只展示真实截图，不替代 App 验收。

70% 版本验证基线：六套深色已实现，目录/迁移/组件对比与 App 单测通过，check、debug 及 release 构建通过。真实 App 前台模式下六主题逐一切换通过，42 项模型视觉检查覆盖起始页、左右抽屉、画布外框、真实终端及交互状态。原版截图对照见 `artifacts/dark-theme-comparison/index.html`，证据来自 `artifacts/desktop-workspace-e2e/2026-10-28T01-16-12Z/report.json`。

60% 透明度增量：39 项抽屉测试、1 项六主题状态角标对比、9 项 Swift 模型协议测试、check、debug 与 release 构建通过。墨黑前台真实 App 的 7 项模型视觉检查通过；没有将旧 70% 的六主题截图视为当前 60% 证据。70%/60% 实机对照见 `artifacts/drawer-opacity-comparison/index.html`；当前构建及验收状态见同目录 `verification.md`。

限制：后台或仅可见模式的截图曾出现全黑，稳定性尚未解决；前台验收不代表后台模式通过。抽屉百分比指主蒙层不透明度，叠加模糊壁纸后墨黑会透出蓝灰；新三套终端底层较实，保留的蓝宝石终端透色更明显，月岩与深海石终端统一 248/255（约 97%）。六主题基线通过项与失败记录见 `artifacts/dark-theme-comparison/verification.md`。

最终选择：月岩、深海石来自 `docs/prototypes/dark-theme-lab/index.html` 的原型，保持原型中的背景渐变、正文、强调色和终端 RGB。旧 id 只在恢复入口兼容，不再出现在设置和命令目录；下次显式选择写入新 id，不在启动时强制改写数据库。其他原型候选只供历史对比，不进入产品。


## 月岩 / 深海石交付验证（2026-10-28 UTC）

审查基准：`b5c2f4c60b88c93d4dc5f2c9e5b1b9491476637e`（dev）。独立 Standards / Spec 两路审查覆盖全部主题一致性差异；确认并修复选中勾对比、运行/审批标签局部阅读底、幕僚按钮前景、已选分段 hover/pressed。增量审查修复 debug 驱动 Down 失败后的释放清理；两路代码审查无剩余已确认问题。

通过：
- `cargo test -p ui`：20项通过（10单元、4目录、6国际化）。
- `cargo test -p app --lib --config 'profile.dev.package.app.debug=0' -- --test-threads=1`：1633通过、0失败、9忽略。默认并行首轮3项locale相关测试失败，独立进程及完整串行复跑均通过；不宣称默认并行全绿。
- App check、debug构建和 `cargo build -p app --release -j 2` 通过。已有 `settings/content.rs` Effects重复分支警告未改动。
- debug输入帧屏障增量：13项e2e_control测试、Swift 4项ClickDelivery及9项VisualReview测试通过。
- 原型5候选×2场景和交互检查通过，无浏览器错误；只作原型证据。
- 代码行数、no-allow、Agent指南、性能、存储与视频壁纸护栏通过。

真实 App 按已验证范围记录：
- `artifacts/desktop-workspace-e2e/2026-10-28T05-25-30Z`：月岩首页、设置、项目、双抽屉、双终端、focus/disabled、selected/hover/pressed通过。已选分段同点RGB为(51,49,60)→(64,63,75)→(78,75,89)，证明实际状态变化。
- 同轮深海石首页、六主题设置目录、项目、双抽屉、终端、focus/disabled和selected通过；selected-hover未证实变化，之后隔离App无场景退出指令却以status=0退出。整轮exit 1，保留PRODUCT记录，不认定退出根因已解决。
- `2026-10-28T05-36-10Z` 后台复跑首页可读，打开设置后全黑；`2026-10-28T05-38-51Z` 前台复跑首页全黑，两轮exit 3 ENV，停止重试。
- 待补：深海石完整hover/pressed，以及两款有Agent内容时的运行/审批标签与按钮实机检查。相关颜色单测通过不能替代这些截图。MR保留Draft，不宣称六主题当前60%版本完整实机全绿。
- `2026-10-28T05-01-03Z`旧失败轮的月岩状态截图曾误判pass，像素复核表明未变化，撤销该轮交互通过结论。之后debug控制通道增加2s有界GPUI绘制提交屏障，超时归driver；release不编译该控制模块。Metal呈现异步，ACK仍不能取代看图。

可审阅截图保存在 `docs/assets/theme-consistency/`，对应05-25-30Z真实窗口，不使用HTML替代。普通正文在任意高亮自定义壁纸上的可读性未全面验收；上述视觉只覆盖默认地球背景。
