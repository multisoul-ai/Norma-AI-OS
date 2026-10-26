# SettingsShortcutDefaults 视觉检查

脚本退出码只代表 OCR 和 SQLite 断言。下面每一张由 Agent 打开图片后单独判断，结论写「符合 / 不符合 / 不确定」。`light-` 与 `dark-` 两组各检查一遍；符号键帽 OCR 不稳定，只能看图。

| 截图 | 要看到 | 不应看到 |
| --- | --- | --- |
| `<主题>-01-default-switch-workspace.png` | Settings → Keyboard Shortcuts 中「Switch Workspace」行键帽为 ⌘⇧↑，无 Modified 标记、无 Restore | 键帽为 ⌘⇧K、Modified 标记、Restore 按钮 |
| `<主题>-02-default-all-workspaces.png` | 「All Workspaces」行键帽为 ⌘⇧K，无 Modified 标记 | 键帽为 ⌘⇧↑、Modified 标记 |
| `<主题>-03-custom-switch-workspace.png` | 既有自定义下「Switch Workspace」行键帽为 ⌃⌥J，带 Modified 标记与 Restore 按钮 | 被默认 ⌘⇧↑ 覆盖、Modified 或 Restore 缺失 |
| `<主题>-04-custom-switcher.png` | 按 ⌃⌥J 后打开工作空间切换器 | 仍在目录页 |
| `<主题>-05-custom-k-not-directory.png` | 按 ⇧⌘K（已被用户自定义给新建浏览器）后仍停在切换器 | 跳到全部工作空间目录 |
