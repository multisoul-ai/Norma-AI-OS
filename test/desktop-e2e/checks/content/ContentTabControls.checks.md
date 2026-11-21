# content-tab-controls 看图清单

运行 `scripts/desktop-e2e.sh content-tab-controls --visual`。使用中文墨黑。逐项打开原始 PNG，按画面判断。

| 证据 | 要看到 | 不应看到 |
| --- | --- | --- |
| `01-narrow-default.png`、`01b-narrow-scrolled.png` | 约 250pt 的右抽屉。标签溢出时，加号、另起一行的刷新和搜索、上下文栏的 ⋯ 仍完整可辨。横滚后的可见标签改变，正文仍是 FILE BEFORE。 | 工具被滚走、相互遮挡或贴边，或滚动改变选中项。 |
| `02-duplicate-sources.png`、`02b-full-source.png` | 搜索输入有聚焦反馈。同名 Plan.txt 两行有文件图标，长来源能区分 alpha-review 和 zulu-review，文字留在行内。悬停 zulu-review 行可看到完整路径。 | 行重叠、路径换行挤压下一行、提示被窗口裁切，或已切到另一标签。 |
| `03-search-empty.png`、`04-search-dismissed.png` | Esc 退出后由 Enter 重开并重新输入，空查询结果状态显示「未找到匹配的标签」。再按 Tab 后浮层消失，仍显示 FILE BEFORE，有可辨的键盘焦点。 | 莫名改变选中项或留有空浮层。 |
| `05-path-keyboard.png`、`06-path-revealed.png` | 只匹配 zulu-review，键盘高亮与当前勾标可区分。确认后 Plan.txt 完整进入可见区，正文为 ZULU FILE。 | alpha 路径、激活前提前切换、确认后标签被固定控件裁掉。 |
| `07-file-updated.png`、`08-file-refresh-failed.png`、`09-file-recovered.png` | 实际文件从 ZULU FILE 变为 ZULU UPDATED。文件缺失时显示刷新失败和重试且保留旧正文。恢复后显示 ZULU RECOVERED。 | 失败覆盖旧正文，重试后仍留错误提示或旧内容。 |
| `10-domain-search.png`、`11-web-before.png` | 按 127.0.0.1 只匹配 Loopback 网页，确认后显示真实 WEB BEFORE 页面。 | localhost 网页或文件误匹配、确认后仍占位。 |
| `12-search-over-web.png`、`13-menu-over-web.png`、`14-web-session-restored.png` | 搜索浮层与标签操作菜单可分别覆盖真实网页，末位菜单的关闭右侧操作禁用，退出后网页输入仍能从 SESSION KEPT 追加至 SESSION KEPT INTACT。 | WebView 盖住任一浮层或退出后替换原网页。 |
| `15-web-refreshing.png`、`16-refresh-other-tab.png`、`17-refresh-returned.png` | 持有真实 HTTP 响应期间，刷新中的转圈、进度线和中文提示可辨。切至 Plan.txt 可阅读 ZULU RECOVERED，切回原网页才恢复相应刷新态。 | 其他标签继承网页刷新状态或放行前出现新正文。 |
| `18-web-updated.png` | 同一 URL 显示 WEB UPDATED，刷新提示和进度线消失。 | WEB BEFORE、刷新失败或无限加载。 |

键盘和鼠标动作后的标签资源、顺序和选中均由隔离 SQLite 核对。HTTP 请求序号与地址由回环夹具核对。
