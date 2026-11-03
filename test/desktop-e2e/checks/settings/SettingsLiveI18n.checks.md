# GPT-Live 标题国际化

参考 apple-design：沿用现有标题层级和设置导航，复用同一产品名称键，不增加动效。

- `0-zh-CN-live-i18n.png`：标题为 GPT-Live，说明与状态为中文；没有裸翻译键或裁切。
- `1-en-live-i18n.png`：在基础设置切为 English 后，标题保持 GPT-Live，说明与状态立即变为英文；语言持久值为 en。
- `2-zh-CN-live-i18n.png`：同一进程切回简体中文，标题正常，说明与状态恢复中文；语言持久值为 zh-CN。
- 三张图均未配置 Key、连接按钮禁用、没有活动通话控件；不启动云端通话，不用重启代替语言切换。

按用户最新要求使用默认虚拟显示器模式验收，不设置 `NORMA_E2E_VISIBLE` 或 `NORMA_E2E_FOREGROUND`。
