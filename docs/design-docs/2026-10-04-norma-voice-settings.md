# Norma Voice 设置改版

## 设计落地

参考 Google 的 [Ten things we know to be true](https://about.google/company-info/philosophy/)：

- **Focus on the user**：先选择 Voice Commands 或 Doubao 2.0 Voice，再显示对应配置。卡片说明用户可以完成的任务。
- **Fast is better than slow**：概览显示真实就绪数量，并提供定位到下一项缺失配置的操作。就绪后提供直接修改入口。
- **清晰的信息层级**：传统链路使用 Speech Recognition、AI Connection、Spoken Replies 三张完整宽度卡片；移除重复标题、步进轴与未就绪时没有操作的完成按钮。
- **即时反馈**：选中模式显示勾选和边框；按钮有 hover、pressed、Tab 焦点；新增操作支持 Enter/Space。播报与光效使用组件库 Switch，并补充焦点和键盘操作。

使用 `apple-design` 的清晰层级、直接操作、一致性及可访问性原则维持 macOS 体验；使用 `emil-design-eng` 和 `gpui-styling/elements/testing` 将这些原则落实到钉死的 GPUI 与组件库。状态文字在深浅主题中使用可读的主题前景色；窄窗口由父行换行，识别来源控件不与下一行表单重叠。

导航沿用 Norma → Persona / Memory / Voice，Persona 在最前面。

## 功能边界

视图复用现有控制器、后台持久化、模型安装、凭据验证及 TTS 草稿/预览/启用流程。查看另一识别来源不立即切换运行来源；取消声音选择不替换当前模型或音色。快捷键显示用户的实际配置。

## 验收证据

| 行为 | 验证方式 | 结果 |
|---|---|---|
| 就绪概览、配置定位、模式保存 | `voice_redesign_task_layout_and_mode_persistence`；真实 App 切换与重启 | 改版基础版本通过 |
| 来源入口并排、无重叠、640×700px 不溢出 | 上述聚焦测试；真实 App 深浅主题 | 改版基础版本通过 |
| 取消声音草稿保持原音色、播报开关保存 | `voice_redesign_ready_voice_change_cancel_and_preferences`；真实 App | 改版基础版本通过 |
| 设置回归 | `cargo test -p app --lib settings:: -- --test-threads=1` | 混合工作区 201 项通过；独立提交快照 196 项通过 |
| 编译与 release | `cargo check -p app`；`cargo build -p app --release` | 混合工作区与独立提交快照均通过 |
| Clippy | `cargo clippy -p app --lib -- -D warnings` | 独立提交快照通过；混合工作区此前有并行国际化的 26 个重复转换诊断 |
| 按钮与 Switch 键盘操作 | `voice_redesign_actions_support_keyboard_and_mouse`；`voice_redesign_switch_supports_keyboard_and_mouse`；真实 App | 通过：Tab 遍历、Enter 切换 Doubao、Space 恢复 Voice Commands、Enter 定位 AI 配置、Space 关闭及 Enter 恢复播报；光效开关可见焦点 |
| 云端声音不显示本地缺模型提示 | `voice_redesign_cloud_voice_ignores_missing_local_model_status` | 通过；本地缺模型提示仍保留 |

最终补丁的 5 项 `voice_redesign` 聚焦测试、201 项 Settings 回归、`cargo check -p app` 与 release 构建通过。格式、diff、文件行数、禁止 allow 和 Agent 指南行数检查通过。真实 App 已重新编译启动，完成模式切换、配置定位和开关键盘验收；已恢复 Voice Commands、原播报及光效偏好，并停留在英文 Voice 首页。

工作区同时存在另一任务的国际化修改。本次提交从这些交叠文件中单独提取语音改版，以原有英文文本保持独立编译；其余改动保留在工作区。独立提交快照的 196 项设置回归、编译、Clippy 与 release 构建已通过，混合工作区中此前的 `LocalizedText` 重复转换诊断不属于本次提交。

真实 App 截图位于 `/Users/alan/.codex/visualizations/2026/10/04/norma-voice-redesign/`。最新构建证据包括 `voice-final-dark.png`、`keyboard-doubao-dark.png`、`keyboard-next-ai-dark.png`、`keyboard-switch-off-dark.png` 和 `keyboard-switch-on-dark.png`；深浅主题与取消草稿的基础改版证据包括 `voice-light.png` 和 `voice-cancel-preserved.png`。本次未完成真人输入或完整云端通话测试，不能据此声明 ASR/TTS 网络链路全部通过。
