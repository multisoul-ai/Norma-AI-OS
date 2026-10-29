# README 媒体来源

用于根目录中英文 README 的展示素材。更新日期：2026-10-29。

## 双语文档

- 默认英文入口：`../../../README.md`。
- 中文入口：`../../../README.zh-CN.md`。
- 两版保持相同的产品结构、能力边界、演示与上手说明，顶部提供语言切换。
- 核心叙事是 Norma AI：自进化、辅助决策、多 Agent 管理与贾维斯式交互；无限画布和 Live Node 承载执行。
- 当前成长能力包含明确授权下的长期记忆读写、用户纠正与后续调用；自动学习、Dream 复盘与 Skill 成长仍属路线规划。

## 封面

| 文件 | 用途 |
| --- | --- |
| `norma-os-cover-zh.png` | 中文 README 品牌封面 |
| `norma-os-cover-en.png` | 英文 README 品牌封面 |

两张封面均使用内置 `image_gen` 生成并保存到本目录。它们是品牌插画，不是产品界面截图。

- 人物身份来自[Norma 原始人物参考](../../brand/assets/source-references/norma-character-reference.png)与[V2 角色系统](../../brand/assets/manual-v2/04-norma-ai-soul.png)。
- 保留浅紫长马尾、白色 X 发夹、黑色蝴蝶结、蓝紫眼睛、海军蓝服装与白色高领；新物料不保留原始参考中的旧 MultiSoul 字样。
- 视觉遵循[品牌规范](../../brand/norma-os-brand-guidelines.md)，使用暖白、墨黑与青柠高亮。已按用户要求替换早期云朵方案。
- 英文封面以最终中文封面为编辑目标，只本地化文字，保持人物和整体构图一致。
- README 顶部图标直接引用产品导出的 `../../../assets/app-icon-macos.png`。

<details>
<summary>中文版最终文案编辑提示词</summary>

编辑目标为已经使用官方人物制作的中文封面；第二输入是上面链接的原始人物参考。

```text
Use case: text-localization / identity-preserve.
Edit the supplied Norma OS README cover to update the product message around Norma AI, the core intelligent companion. This is a precise typography edit, keeping the official character intact.

Input 1 is the EDIT TARGET: the newly made cover featuring Norma with folded arms.
Input 2 is the original official CHARACTER IDENTITY reference only. Keep the same young adult character already in image 1; do not add any sheet decorations or old brand words.

Preserve from image 1: exact 2:1 wide format, right-side character pose, face, long cool-lilac ponytail, straight bangs, large white X hair clip, black ribbon, blue-violet eyes, navy belted tailored clothing with blue-violet patterned lapels and white collar. Preserve warm off-white background, soft window light, editorial typography, generous whitespace and existing acid-lime highlight. Do not redraw her as another person. Keep her head fully in frame.

Replace ALL cover text with the following exact Chinese/English copy, in the corresponding locations:
Top-left small brand wordmark: "Norma OS"
First large heading line: "会思考，会成长。"
Second large heading line, on acid-lime highlight: "陪你把事做成。"
Supporting line below headings: "Norma AI，你的贾维斯式 AI 伙伴。"
Bottom-left small line: "自进化 · 辅助决策 · 多 Agent 管理"
Bottom-right small monospaced line: "YOU SET THE DIRECTION."

Keep the supporting line on one line if possible by using an appropriate smaller size. Ensure every Chinese glyph is correct and readable. The large headline stays wholly on the left, does not overlap character. No old headline or old footer may remain.
The intent is a capable artificial intelligence with memory, decision support and multi-agent coordination. This is a restrained brand cover, not a UI screenshot.
No cloud mascot, no additional people or objects, no interface panels, no extra text, no Marvel/film character likeness, no superhero imagery, no Apple logo, no MultiSoul word, no watermark, no neon. Preserve the Norma character and polished overall composition exactly while changing the text.
```

</details>

<details>
<summary>英文版封面本地化提示词</summary>

编辑目标：`norma-os-cover-zh.png`。

```text
Use case: text-localization.
Asset type: English GitHub README cover for Norma OS.
Input 1 is the EDIT TARGET: the approved Chinese cover featuring the official Norma character. Localize ONLY the typography to English. Keep the entire character identity and artwork, pose, expression, face, lilac ponytail and bangs, white X clip, black ribbon, blue-violet eyes, patterned navy tailoring, white collar, lighting and composition unchanged.
Preserve the wide 2:1 format, warm off-white paper background, delicate window light, clean black type, generous whitespace and acid-lime second-line highlight. Character remains on the right. Keep head, hair clip and ribbon fully in frame.

Replace ALL cover text with this EXACT English copy, placed in the same corresponding locations:
Top-left small wordmark: "Norma OS"
First large headline line: "Think. Grow."
Second large headline line on acid-lime highlight: "Build. Together."
Supporting line below: "Norma AI, your Jarvis-like AI companion."
Bottom-left small line: "Self-evolution · Decision support · Agent orchestration"
Bottom-right monospaced small line: "YOU SET THE DIRECTION."

Set the English headline large and confident with each supplied line unbroken; adapt font size to fit entirely in the left half and never overlap the character. Adjust the lime highlight width to the new second headline. The supporting line and bottom-left line may use smaller type to fit within the left text column. All text must be accurate and legible. Remove every Chinese glyph and old copy. Preserve all non-text imagery. This is a brand illustration, not an app screenshot. No cloud mascot, no new characters, no new panels, no product mockups, no Marvel/film character likeness, no superhero imagery, no Apple logo, no MultiSoul wording, no extra text, no watermark.
```

</details>

## 真实演示

中英文 README 与 App 功能引导、`/help` 使用同一组九段终版产品录屏（2026-10-29）。README 直接展示静音 GIF，点击进入仓库内的完整 MP4，可在 GitHub 播放并听到原声；不再依赖旧官网视频链接。

GIF 保持原速，宽 640 像素、8 fps、128 色 Bayer 调色；短片完整展示，较长的语音、幕僚和卡片创建录屏使用片段预览。准确起点、时长、源文件和输出校验值见 `final-demos.json`。完整 MP4 保留原片全部时长和音轨，压缩参数见 `crates/app/assets/feature-guide/README.md`。

| GIF | 完整视频 | 内容 |
| --- | --- | --- |
| `demo-voice-command.gif` | `crates/app/assets/feature-guide/demo-voice-command.mp4` | 语音交互 |
| `demo-agent-collaboration.gif` | `crates/app/assets/feature-guide/demo-agent-collaboration.mp4` | 多 Agent 协作 |
| `demo-live-node-shortcuts.gif` | `crates/app/assets/feature-guide/demo-live-node-shortcuts.mp4` | 快速创建卡片 |
| `demo-layout-command-m.gif` | `crates/app/assets/feature-guide/demo-layout-command-m.mp4` | 卡片展开 |
| `demo-chief-of-staff.gif` | `crates/app/assets/feature-guide/demo-chief-of-staff.mp4` | 幕僚 |
| `demo-right-drawer.gif` | `crates/app/assets/feature-guide/demo-right-drawer.mp4` | 右侧抽屉 |
| `demo-switch-workspaces.gif` | `crates/app/assets/feature-guide/demo-switch-workspaces.mp4` | 项目与工作空间 |
| `demo-workspace-home.gif` | `crates/app/assets/feature-guide/demo-workspace-home.mp4` | 工作空间首页 |
| `demo-slash-commands.gif` | `crates/app/assets/feature-guide/demo-slash-commands.mp4` | 斜杠指令 |

GIF 使用 FFmpeg 的 palettegen / paletteuse 生成：

```bash
ffmpeg -ss "$START" -t "$DURATION" -i "$SOURCE" \
  -filter_complex 'fps=8,scale=640:-2:flags=lanczos,split[a][b];[a]palettegen=max_colors=128:stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=3' \
  -loop 0 "$OUTPUT.gif"
```

## 内容校准

- 结构参考：[MultiSoul README](https://github.com/yakami129/multisoul)。参考其居中品牌区、视觉介绍、能力、上手与开发文档的组织方式。
- Norma AI 的现行能力依据项目源码、[记忆接入记录](../../exec-plans/2026-10-03-norma-memory-access-and-settings.md)与[幕僚设计](../../design-docs/2026-10-23-norma-brain-acp.md)；[记忆与成长蓝图](../../design-docs/2026-10-02-norma-master-memory-and-growth.md)中的未完成能力明确标为规划。
- 终版快速创建演示使用 `/` 指令；⌘M 展开选中卡片。数字键快捷创建规则以 `../../../crates/app/src/shortcuts/catalog.rs` 为准。
- Agent 可执行命令以 `../../../crates/runtime/src/nodes/agent.rs` 为准，包括 Cursor 的 `cursor-agent` 以及 Claude/Codex 的自动执行权限。
- 开发签名要求以 `../../../scripts/run-macos-dev-app.sh` 为准。
- 下载链接沿用官网的独立发布仓库。更新时最新公开版本为 v0.1.77；README 使用 latest 地址避免固定旧版本。
