# Norma iOS 伴侣：工作空间指挥面

Labels: `wayfinder:map`  
Tracker note: `gh issue create` 返回 `Resource not accessible by integration`（HTTP 403）。当前 token 不能建 GitHub Issue。本文件是待发布的 map 正文；权限恢复后应按此创建 `wayfinder:map`，并把下面各决策拆成子 issue。

## Destination

锁定 Norma iOS 伴侣的工作空间模型，并交付一份可点 HTML 原型：一件事是一个空间；Norma 是幕僚长，负责分派、进度和优先级；手机上先看空间列表，空间内是共享时间线，进度单独一页。生产 SwiftUI、真机联调和注意力分流不在本图完成条件里。

## Notes

- **Domain**: Norma OS 伴侣面。Norma 是唯一 Master；在本图里她是幕僚长，不是工人，也不是 Agent 卡片。
- **Skills**: 讨论用 grilling + domain-modeling；页面用 apple-design；原型用 prototype。
- **Standing preferences**:
  - 假数据可点 HTML 优先，不接真模型
  - 第一屏是空间列表；进度单独一页
  - 成员默认是你和 Norma；Agent 第一次被 @ 时加入，且在整个空间内是同一个人
  - 这一版原型不做「需要你 / 摘要 / 锁屏」
- **Plan, don't do**: 本图产决策和原型资产，不实现生产 App。
- **Prototype**: [prototype/index.html](./prototype/index.html)

## Decisions so far

- 一件事一个空间；时间线只追加；成员默认是你和 Norma，Agent 在第一次被 @ 时加入。
- Norma 是幕僚长：没点名 Agent 的消息先到她。指令拆成任务并标优先级，讨论只回复、不建任务。她不亲自做已派出去的活。
- 显式 `@` 某个 Agent 且说清动作，直接建任务，请求人是你。Norma 不改派，只纳入进度和优先级。没有 `@` 的名字不算点名。
- 只 `@`、没说动作：只加入成员，不建任务，由 Norma 追问一句。
- 一条消息 `@` 多人且动作说得清：一人一条任务；后面没接住的，不撤销前面已接住的。
- 显式 `@Norma` 是把事情交给幕僚长。她可以自己回复，也可以再 `@` 别人；转派的请求人记成 Norma。
- Agent 再 `@` 另一个 Agent 时带上父任务。不能 `@` 自己，也不能打回当前任务的祖先。
- 频道就是工作空间，可绑定一个或多个项目。空间里的 Agent 是共享的，不按项目拆成不同的人。
- 一条任务只在一个项目里执行。消息写了仓库就用它；没写，由 Norma 选一个或追问，不默认同时改所有项目。
- 界面收成三页：空间列表 → 空间页（时间线、发言、顶部成员）→ 进度页。Agent 名册不单独占页。
- 可点原型在 [prototype/index.html](./prototype/index.html)。「需要你 / 摘要 / 锁屏」不在这一版。

## Not yet specified

- 完成、提问、权限、失败要不要分开进入摘要和「需要你」
- 一条任务的心跳要不要写进时间线
- 薄 SPEC 的章节与验收条目
- 手机如何证明可指挥桌上的 Norma
- 通知等级、Shadow、语音是否为一等公民

## Out of scope

- 完整 SwiftUI / App Store
- 真机 Norma 联调作为本图完成条件
- 云端 Agent 会话（桌上可关仍指挥）
- 非 Norma 的通用多 CLI 驾驶舱
- 当前原型中的「需要你 / 摘要 / 锁屏」
