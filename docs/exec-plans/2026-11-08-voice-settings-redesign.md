# 语音设置精简改版实施计划

状态：草案，待确认 · 日期：2026-11-08 · 原型：[`docs/prototypes/voice-settings/voice-settings.html`](../prototypes/voice-settings/voice-settings.html)

## 目标

把语音设置从四段（01–04）加一个高级入口，收敛为「听 / 说」两组的主界面，低频配置收进「更多设置」。保留全部功能与凭据配置，移除「试一句」预览卡片，恢复 TTS 引擎与声音的完整选择。

## 范围

**包含**
- 主界面：语音指令、豆包 2.0、GPT-Live 三种模式的行数收敛。
- 语音回复：引擎（关闭 / 本机 / MiniMax / 系统旁白）与声音选择面板，「使用此声音」才生效。
- 更多设置：理解模型（供应商、基础地址、API Key、连接与模型）、Azure（区域、识别语言、Key、验证）、回复偏好、本地引擎诊断。
- 凭据：Azure Key、MiniMax Key、火山方舟 Key、OpenAI Key、理解模型 API Key。

**不包含**
- 新增供应商或新的识别、合成能力。
- JEV 面板（代码中受生产门禁控制，本次不动，单独立项）。
- 派发策略的运行时行为（本次只改设置界面）。

## 已确认的决策

1. **「试一句」整体删除**：删除预览功能，包括 `crates/app/src/settings/voice/view/command_preview.rs`、`state` 中对应逻辑，以及 `voice.command_preview_*` 键。实施前先 grep 确认没有其他入口引用。
2. **本机模型与音色对应关系**：暂不展示具体音色映射。先核对模型包后再决定，P3 面板的音色列表在此之前只显示已确认的内容。
3. **系统旁白声音**：由系统枚举 macOS 可用声音，供用户选择。

## 现有代码地图

| 区域 | 文件 | 改动 |
| --- | --- | --- |
| 分组布局 | `crates/app/src/settings/voice/view/grouped.rs` | 由 01–04 改为「听 / 说」，接入更多设置 |
| 唤起与模式 | `view/interaction.rs` | 对话方式、快捷键合并为一组选项 |
| 回复偏好 | `view/spoken_reply.rs` | 保留播报与光晕开关，移入更多设置 |
| 识别设置 | `view/setup/recognition.rs`, `view/setup/a1.rs`, `view/setup/models.rs` | 识别来源菜单化，A1 仅单次可用 |
| TTS 选择 | `view/tts_provider.rs`, `view/tts_provider/picker/`, `view/tts_provider/voice_list.rs`, `state/tts_picker.rs` | 复用为声音面板的数据源 |
| 凭据 | `view/provider/api_key.rs`, `view/doubao*.rs`, `view/gpt_live/key.rs` | 统一为 `keyField` 样式（保存 / 清除 / 状态） |
| 状态 | `state/mode.rs`, `state/values.rs`, `state/tts_provider.rs`, `state/gpt_live.rs` | 增加「草稿 → 应用」的 TTS 状态 |
| 文案 | `crates/ui/locales/voice.zh-CN.json`, `voice.en.json` | 新增键，删除不用的键 |

新增文件：`view/voice_sheet.rs`（声音选择面板）。

## 阶段

### P0 确认（0.5 天）
- 对上面三项决策取得答复。
- 确认验收标准与 `docs/quality/desktop-e2e.md` 中的场景划分。

### P1 状态与纯逻辑（TDD，先写失败测试）
- TTS 草稿模型：`prov / model / voice`，应用前不改变生效值，取消后不改变生效值。
- 规则：选择持续聆听时 A1 不可用，自动回落到本地 Sherpa（复用 `DesktopVoiceMode::supports_local_a1`）。
- 规则：克隆声音需要参考 WAV 与准确转写才能应用。
- 规则：MiniMax 声音需要验证 Voice ID 才能应用。
- 测试位置：`cargo test -p app --lib voice`、`cargo test -p tts`。

### P2 主界面重排
- `grouped.rs` 改为两组：听（识别、唤起、快捷键、麦克风）、说（语音回复一行）。
- 「更多设置」折叠区：理解模型、Azure、回复偏好、诊断。
- 每个文件保持 500 行以内，超出则拆分。

### P3 声音选择面板
- `view/voice_sheet.rs`：引擎切换、模型列表（含下载状态）、音色列表（含试听）、克隆参考、MiniMax 连接与 Voice ID、底部「取消 / 使用此声音」。
- 键盘：Esc 关闭并返回触发器焦点；Tab 在面板内循环；焦点与 hover、pressed、disabled 状态按 `docs/quality/ui-interaction-feedback.md` 实现。
- 设计依据：`docs/design-docs/material-menus.md`（菜单与选择器）和 Apple HIG（面板与层级），在验收记录中写明采纳的决策。

### P4 凭据与文案
- 统一凭据行：已保存状态、清除、保存；Key 不回显、不写日志。
- 新增双语键，删除不再使用的键。
- 运行 `python3 scripts/check-i18n.py` 与 `python3 scripts/test-i18n.py`。

### P5 验收
- 单元与集成：`cargo test -p app --lib voice`、`cargo test -p tts`、`cargo check -p app`。
- 约束脚本：`bash scripts/check-max-file-lines.sh`、`bash scripts/check-no-allow.sh`、`bash scripts/check-agent-guide-lines.sh`。
- 真实 App：新增或改版页面用 `scripts/desktop-e2e.sh voice --visual` 定向验收，覆盖语音指令、豆包 2.0、GPT-Live 三种模式，浅色与深色各一次。
- 双语：中文与英文各跑一次关键路径。

### P6 合并
- 通过 PR 合并到 `dev`，等待 CI（含 `rust checks`）全部通过。
- 不使用 `--no-verify`，不使用管理员强制合并。
- 提交信息末尾加 `Co-Authored-By: Claude Haiku 5.5 <noreply@anthropic.com>`。

## 验收映射

| 标准 | 证据 |
| --- | --- |
| 主界面仅保留听、说两组 | desktop-e2e 视觉场景（三种模式） |
| 试一句已移除 | desktop-e2e 检查页面无该卡片；i18n 检查无残留键 |
| 声音草稿取消不生效 | `cargo test -p app --lib voice`（草稿测试） |
| A1 仅单次可用 | `cargo test -p app --lib voice`（模式规则） |
| 凭据保存与清除 | 单元测试 + desktop-e2e 场景 |
| 双语文案完整 | `check-i18n.py`、`test-i18n.py` |

## 风险

- `grouped.rs` 与 `interaction.rs` 已较大，拆分时容易引入回归；先拆结构、后改样式。
- GPUI 0.2.2 与 gpui-component 0.5.1 的弹层与焦点 API 需先在邻近代码中确认，再实现面板。
- 原型中的 TTS 名称与分组为示意，落地前以代码与模型包为准。
