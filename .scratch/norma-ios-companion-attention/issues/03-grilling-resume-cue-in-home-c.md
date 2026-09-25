# Resume 线索落在首页 C 的哪里

Type: grilling  
Status: resolved  
Part of: [Norma iOS 伴侣：注意力优先指挥面](../map.md)

## Question

在「Norma 指挥条 + 闸门主舞台」下，**离开/回来**时的 resume 线索放在哪：决策卡片上的「回到这里时」一行、Norma 指挥条占位符、zoom 进 Agent 后的输入框预填、还是系统通知正文？最小可验收形态是什么（假数据原型里必须看得见的 1–2 处）？

## Answer

**决策与指挥分表面（R1=C）**

1. **待批决策 resume**：落在「需要你」决策卡上的一行灰字方案/下一步（R2=A）。不新开「回来时」专区；不单靠推送。
2. **指挥/半句 resume（R3=C）**：首页 Norma 指挥条 placeholder 显示短「继续：…」；仅当用户点「看上下文」zoom 进 Agent 时，输入框预填完整下一步。
3. **锁屏 / Live Activity（R4=A 为主融合 C）**：默认展示短下一步（≤两行）；**权限/失败必须带方案句**；普通提问可用标题 + 极短线索。禁止 Working 心跳刷屏。
4. **文案来源（R6=D）**：闸门/系统模板为底（如 `下一步：批准 X`）；若事件带 `suggested_action` 则优先采用。
5. **指挥条线索清除（R7）**：发出下一条或清空输入即清；另提供手动叉掉。
6. **原型必须看得见（R5=1+2+4）**：决策卡方案行、指挥条「继续：…」、至少一帧锁屏/Live Activity 示意；zoom 预填为可点下钻即可。

草图：[`../assets/norma-ios-r4-live-activity-three-options.png`](../assets/norma-ios-r4-live-activity-three-options.png)
