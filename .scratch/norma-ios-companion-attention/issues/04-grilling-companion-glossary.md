# 伴侣面词汇：首页区域与物件怎么命名

Type: grilling  
Status: resolved  
Part of: [Norma iOS 伴侣：注意力优先指挥面](../map.md)

## Question

相对 `CONTEXT.md` 已有 Norma / Agent 卡片 / Norma 回合，iOS 伴侣首页 C 的区域与物件应叫什么（例如：需要你 / 摘要 / 指挥条 / 闸门 / 决策卡），英文键名是否需要产品名（App 名）？哪些词要 Avoid，以免和桌面「设置·编排」或通用「Inbox」混淆？

## Answer

1. **产品名（G1=C）**：首版 UI 不拍板 App 品牌名；界面用功能名。文档/SPEC 称 **Norma iOS 伴侣**。
2. **主队列（G2=A）**：**需要你**（对应 Cursor Needs Attention，非 Inbox）。
3. **Digest 区（G3=A）**：**摘要**。
4. **指挥入口（G4=C）**：对内称 **指挥条**；UI 无组件标题，靠 placeholder 自解释（Cursor「Plan, ask, build...」风）。
5. **条目（G6=B）**：对内可称决策卡；**对用户显示类型徽标**：**提问** / **权限** / **失败**。
6. **舰队二级页（G7=C）**：**全部 Agent**。
7. **空状态（G8）**：主文案 **暂时没有需要你的事**；可选副行 **Norma 会在需要时找你**。
8. **Avoid（用户可见主文案）**：**Inbox**、把 **Agent Chief / 闸门** 当品牌露出、桌面已占用的 **编排**。

已写入根目录 `CONTEXT.md`「Norma iOS 伴侣（草案）」节。
