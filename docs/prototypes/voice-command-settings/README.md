# 语音指令设置 · Material 交互原型

状态：独立设计参考；用户已选择分组列表。
生产设置与单次候选提交已在独立工作树实施，记录见实施计划。
本原型继续使用固定示例，不连接生产路由或真实引擎。
`index.html` 为独立离线页面，无网络依赖。
模拟成员与指令均为人工编写，不使用真人录音。

## 打开与比较

打开 `docs/prototypes/voice-command-settings/index.html`。
底部切换器提供三版，键盘 1/2/3 或方向键切换。
选中布局通过 URL 参数 v 保留，不写入产品设置。
原型配置在三版之间共享；刷新后恢复演示默认。
浅深色和语言偏好只保存在原型自己的 localStorage 键中。

| 方向 | 轴 | 优点 | 代价 |
|---|---|---|---|
| 分组列表 | 直接展开编辑 | 常用设置一页可见 | 内容较长 |
| 流程分步 | 按流程逐段编辑 | 输入、理解、派发、回复清楚 | 跨组修改需切换 |
| 概览与抽屉 | 摘要后渐进展示 | 首屏紧凑 | 编辑多一步 |

## 可交互范围

- 本地/云端来源合并；本地引擎细节仅在诊断查看。
- 单次/持续聆听、按住/点按触发、麦克风搜索选择。
- 画布 Bruno/Henry、被提及者与执行者区分、未知目标澄清。
- 编辑原话后，旧候选失效，须重新预览。
- 回复开关、音量、快捷键显示、模拟麦克风检查。
- 深浅主题、中英文、就绪/准备/资源不足状态演示。
- 原型重置、云端配置模拟和本地帮助抽屉。

所有状态、麦克风测试、识别和派发均为模拟。
云端配置不提供真实密钥输入框。
Jev 标记为尚未接入；不使用系统环境变量发请求。
自定义输入未命中固定示例时，明确显示未覆盖。
侧栏其他设置为不可用的上下文，不假装已实现。

## 构建与验证

```bash
node docs/prototypes/voice-command-settings/build.mjs
node --test docs/prototypes/voice-command-settings/tests/model.test.mjs
node docs/prototypes/voice-command-settings/check-render.mjs
```

9 项逻辑测试先失败后实现。
模板检查直接调用渲染函数，不等同于浏览器验收。
浏览器工具返回“browser access host unavailable”，未取得实屏证据。
原型的真实浏览器交互和视觉对比尚未完成。
生产 App 的独立验收证据见实施计划 H 节。
这不影响本页作为可打开的设计草稿交付。

## 设计依据

采用多方案原型比较，并遵循固定 Picker 规范。
Picker 样式和键盘逻辑按规范复制，静态比较不显示回放按钮。
参考 `.agents/skills/apple-design/SKILL.md` 的平台习惯与可访问性。
参考 `.agents/skills/emil-design-eng/SKILL.md` 的必要反馈与克制动效。
页面视觉按用户要求采用 Google Material 的分组与状态层。
48 px 搜索菜单独立呈现已选值和键盘浏览状态。
不采用循环动效、固定大目录或重复的本地来源按钮。
原型使用必要的 140 ms 开关反馈，并支持减少动态效果。
这里是 HTML 原型，没有新增 GPUI 实现。

实施计划：`docs/exec-plans/2026-10-31-voice-command-redesign-plan.md`。
架构：`docs/design-docs/2026-10-31-voice-command-jev-architecture.md`。
