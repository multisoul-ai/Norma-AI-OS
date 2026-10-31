# WorkspaceIdeasProjectFocus 定向验收

本场景覆盖项目正文焦点下的 Ctrl+1～5，建议使用 `NORMA_E2E_INPUT=ns` 经过 AppKit。

| 截图 | 应看到 | 不应看到 |
| --- | --- | --- |
| `00-project-focus.png` | `FocusProbe` 标记位于项目终端正文，证明项目实际接收输入 | 标记出现在宿主抽屉或底部指令栏 |
| `01-missing-card.png` | 宿主想法栏显示第 5 张卡片不存在，唯一待办仍在，项目卡片保留 | 崩溃、空白窗口、待办被误派发 |
| `02-assigned.png` | 项目卡片与想法栏仍可见，唯一想法已指派 | 窗口消失、重复想法 |
| `03-empty-notice.png` | 宿主想法栏显示没有待办的提示，项目卡片仍可见 | 连续 Ctrl+数字后崩溃或重复派发 |

硬断言同时核对待办状态、真实任务引用及空待办后引用不变。双语运行使用 `NORMA_E2E_LANGUAGE=zh-Hans` 与 `NORMA_E2E_LANGUAGE=en`。
