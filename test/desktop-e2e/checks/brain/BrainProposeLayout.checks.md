# 下一步提议行版式（brain-propose-layout，--visual）

只看“需要你处理”里的提议行，不把画布、统计栏、活动面板当作本次差异。

- `01-layout-*`：行间无描边/分隔线；拒绝/采纳在标题下方单独一行靠右，高度小于标题行；标题与副标题不被挤成多行或截断。
- `02-hover-button-*` / `03-hover-row-*`：悬停按钮和标题行都有可见底色变化，行悬停不出现描边。
- `04-keyboard-focus-*`：图钉和三个统计卡之后，第五个 Tab 聚焦第一条提议标题；移开鼠标后标题底色高亮且无描边，Enter/Space 只展开/收起该提议。
- `05-expanded-*`：展开后按钮仍在标题下一行，不叠正文；英文 Reason / Output 标签各自单行横排。
- `06-pressed-*` / `07-ignored-*`：按下态比悬停更深；拒绝后只剩一条。
- `08-disabled-*`：目标等待确认时采纳禁用并有原因提示；实际点击 Waiting / 等待确认后没有派发，提议仍保留一条（程序断言）。
- `09-dispatched-*`：采纳后提议行消失，目标 Bella（node 2）恰好收到一次原文且提议文件清空（程序断言）；回执保存在 received.jsonl。

覆盖：ink-black 与 moonstone 两个现行主题 × zh-Hans / en。现行可选主题均为深色（旧浅色 paper-blue 等已迁移为 Ink Black），因此没有可选浅色主题。
