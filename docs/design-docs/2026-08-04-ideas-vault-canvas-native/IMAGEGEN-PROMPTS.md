# Ideas Vault Canvas-native 原型生成提示词

- 生成方式：内置 `image_gen`
- 生成日期：2026-08-04
- 参考图：真实 Canvas、真实 Agent queue sidecar，以及本组前序生成稿
- 说明：以下记录最终交付图使用的提示词约束；图片是设计原型，不是当前产品截图

## 00-canvas-native-overview.png

```text
Use case: ui-mockup
Asset type: high-fidelity product interaction storyboard for Norma OS design documentation
Primary request: Create a wide 16:9 three-stage interaction overview for a canvas-native “Ideas Vault” feature. Match the visual language of the supplied real Norma OS screenshots: translucent macOS glass cards on an infinite wallpaper canvas, identical shared card title bars, bottom floating command dock with Voice, bottom-right minimap, subtle blue accent, native typography, restrained shadows.
Reference images: Image 1 is the real overall Canvas shell and card styling; Image 2 is the real anchored sidecar pattern and actual AgentCard appearance. Preserve these visual rules.
Composition: one continuous wide canvas with three clearly numbered regions connected by thin restrained arrows, not separate browser windows.
Stage 1: on the left, a real terminal-backed Agent card titled “Codex” remains focused. A bottom Voice control shows a small transcript bubble with exact Chinese text “记一下：先做工作区级想法库”. Next to Codex is one native card with type pill “IDEAS”, title “Ideas Vault”, badge “+1 NEW”. The focused Codex card keeps its blue focus ring, proving the capture does not steal focus.
Stage 2: center, the same single Ideas Vault card is selected and enlarged in the existing master layout. Its body shows a native search field “搜索想法…”, filter chips “全部”, “草稿”, “待推进”, and compact rows. One row is selected: “工作区级想法库”. To the right, an existing Preview card shows an image attachment, clearly a separate Canvas card.
Stage 3: right, show the Ideas Vault card’s detail state within the same card, with “‹ 返回想法列表”, title “工作区级想法库”, status “草稿”, sections “描述”, “设计说明”, “素材”, and attachment chips “原型图.png”, “来源 Codex”. Include a small close/reopen annotation using exact text “关闭卡片 ≠ 删除想法”.
Constraints: one and only one Ideas Vault card per stage; AgentCard must look terminal-backed, never like a chat UI; Ideas Vault must share the same card chrome as Terminal/Agent/Preview; Preview remains a separate attachment card; show bottom Voice as the only Norma conversation entry; no sidebar library, no browser-backed local app, no modal dialog, no device frame, no watermark.
```

## 01-vault-card-modes.png

```text
Use case: ui-mockup
Asset type: high-fidelity UI state board for Norma OS product design documentation
Primary request: Create one wide 16:9 design board showing three interaction states of the exact same canvas-native Ideas Vault node: Inbox, Detail, and Editor sidecar. Match the supplied generated Ideas Vault card styling and the real Norma OS anchored Agent queue sidecar.
State 1 — Inbox: one card with type pill “IDEAS”, title “Ideas Vault”, badge “12 IDEAS”. Body has native search field “搜索想法…”, chips “全部”, “草稿”, “待推进”, and rows “工作区级想法库”, “Preview 图片切换”, “Voice 实时转写”, “Agent 来源关联”. Bottom action “＋ 新想法”.
State 2 — Detail: the same Ideas Vault card, title “Ideas Vault / 工作区级想法库”, badge “草稿”. Body shows “‹ 返回想法列表”, “编辑”, “描述”, “设计说明”, “素材”, tags “#canvas” and “#workspace”, and attachments “原型图.png”, “交互说明.md”, “来源 Codex”.
State 3 — Editor sidecar: show the Ideas Vault card and a white translucent anchored sidecar on its right with a small pointer, closely matching the real Agent queue sidecar. Sidecar title “新想法”, fields “标题”, “描述”, “标签”, “状态”, section “素材”, buttons “取消” and “保存草稿”, and drop target “拖入图片、文档或视频”.
Constraints: all three states are one Ideas Vault node changing body state; identical card chrome and dimensions; sidecar is anchored and nonmodal; no app-wide sidebar; no Browser Library; no chat UI; no modal window; no extra Ideas cards; no watermark.
```

## 02-focus-layout-and-recovery.png

```text
Use case: infographic-diagram
Asset type: product interaction state map for Norma OS design documentation
Primary request: Create a wide 16:9 visual state map showing four canvas-native Ideas Vault interaction contracts, using polished high-fidelity Norma OS Canvas scenes.
Panel 1 “首次捕获：创建单例”: show Terminal, terminal-backed Claude AgentCard, and one Ideas Vault card in the standard grid. Caption “创建节点 · 全量重排 · CanvasSelect”.
Panel 2 “再次捕获：不抢焦点”: show focused Codex AgentCard, existing Ideas Vault with “+1 NEW”, and active bottom Voice. Caption “不新增节点 · 不重排 · 焦点保留”.
Panel 3 “回看素材：沿用主从布局”: show Ideas Vault as larger master and Preview “原型图.png” as the detail card. Caption “Vault 是主卡 · Preview 是附件卡”.
Panel 4 “关闭再打开：数据仍在”: show shared close removing only the Vault card, then New → Ideas Vault recreating the single card with the same rows. Caption “关闭视图 ≠ 删除想法”.
Constraints: AgentCard is terminal-backed, never chat; Norma exists only through bottom Voice; Ideas Vault uses shared card chrome; one Vault per panel; placement follows grid or master-detail; no browser library, no sidebar database, no watermark.
```

最终图额外执行了一次局部纠错：把初稿中错误出现的 `Norma` AgentCard 改为 terminal-backed `Claude`，并保持 Norma 只存在于底部 Voice 入口。
