# GPT-Live 设置页精简（2026-10-19）

本次只调整 GPT-Live 设置呈现。实施工作区为 `.worktrees/gpt-live-duplex`，不改 GPT-Live 通话协议、项目工具、豆包和语音指令实现。

## 配置取舍

| 项目 | 新行为 | 原因 |
| --- | --- | --- |
| API Key | 未配置时直接输入；已保存时只显示状态与更换入口 | 避免常驻空编辑框，不回填已保存 Key |
| 实时／按住说话 | 保留独立选择和实际快捷键提示 | 两种不同的日常对话方式 |
| Marin／Cedar | 保留选中态与下次通话生效提示 | 实际改变回复音色 |
| 手动选择麦克风 | GPT-Live 页移除，显示系统默认输入与系统设置入口 | 采集层的 `_input_device_id` 当前未使用，原选择器并不切换硬件 |
| 任务核对／用量 | 放入默认收起的诊断 | 是排错信息，不是开始对话的必填配置 |
| 静音／停止播报 | 仅在通话期间显示，连接未就绪时禁用 | 避免未连接时显示无效操作 |
| 返回主界面 | 移除重复按钮，沿用设置页关闭入口 | 避免同一操作重复出现 |

只读麦克风行仍提供刷新、声音设置与必要的权限请求；连接与工具错误直接显示，不收进诊断。Key 移除和保存失败重试仍可达，不改变原后台持久化、并发回滚或通话取消语义。

Google Material 落地为紧凑设置行、4/8px 间距、明确主操作、克制的语义色、选中勾选与渐进展示。沿用 gpui-component Button 的 hover/pressed/focus 状态，并对键盘激活显式检查禁用。Apple Design 用于 macOS 系统字体、焦点和即时反馈，不添加动效。

## 验收映射

| 验收内容 | 证据 |
| --- | --- |
| 旧页面的冗余操作确实可被新测试捕获 | `desktop-red.log`，真实旧 App 退出 1，命中静音／停播／核对默认可见 |
| 已保存 Key 收起、更换展开、诊断展开、旧配置未覆盖 | `gpt_live_settings_material_tests.rs`，通过真实渲染入口和模拟点击补充验证 |
| Key、音色、模式持久化和并发行为 | 现有 GPT-Live 设置测试，本次定向回归 |
| 深浅主题、配置精简、诊断、保存 Key、模式隔离 | `settings-live-configuration` 真实 App 场景，逐图检查 |
| 无语音运行时回归 | 本轮 125 个受保护文件 SHA256 对比；GPT-Live／豆包／旧语音测试 |

证据目录：`/Users/openclawd/.codex/artifacts/gpt-live-20261018/diagnosis/settings-material-20261019/`。

## 原生回归结果

本轮定向回归共 298 条通过，0 失败：Material 页面交互 3 条、其余 GPT-Live 63 条、豆包 71 条、旧语音相关 161 条。另有 4 条原本标记 ignored 的云端／声学用例未运行，不视为通过。UI crate 7 条单测及 5 条国际化集成测试通过，locale 修改后未再变化，复用同轮证据。

Material 测试覆盖完整 KeyDown/KeyUp 只执行一次、已保存密钥渐进展示、首次输入一个字符落库后继续输入，以及 Enter／隐藏／移除三种结束方式。最终检查隔离凭据的完整无效夹具值，不输出凭据正文；移除后再按 Enter 不能恢复旧值。

本任务 App 单测仅对 App 测试 crate 使用外部 rustc wrapper：`opt-level=0, lto=off, codegen-units=16`；依赖复用 release 缓存。生产测试包使用默认 release 参数重新构建，不使用此 wrapper。

## 最终真实窗口验收

默认 release 构建通过（5m41s），`cargo check -p app --release` 通过。将同一 release 二进制复制到本 worktree 的 `target/debug/norma-os` 后，运行 `bash scripts/desktop-e2e.sh settings-live-configuration`：退出码 0，8 步通过、26.94s，最终场景通过率 1/1。11 张关键截图已全部实际打开，逐张符合清单；SQLite 验证了模式、音色、移除结果与停顿续写后的完整无效夹具值。125 个语音运行时／豆包／Agent 受保护文件哈希均未变化。

最终目录：`artifacts/desktop-workspace-e2e/2026-10-19T02-38-22Z/`，包含 `report.json`、`app.log` 与全部截图。外部证据目录保存 `desktop-report.json`、`visual-review.json`、各组测试日志和源码哈希。上一轮同一产品包在 OCR 把“已安全保存”读作“己安全保存”时退出 1；实际失败图确认编辑器与保存状态正确，只修正场景 OCR 候选后重跑，不为 OCR 修改产品。

测试包保持独立的 bundle ID、数据目录与启动器，更新时保留已有 Key，旧包另行备份；不修改 `/Applications/Norma OS.app`。本次没有真实云端通话、物理声学验收、通话中控件或全量 hover/pressed 遍历的通过结论。

## 独立的麦克风故障

本次改版不代表已修复 `-10876` 声学故障。原生探针发现系统默认输入 ID 为 0，设备输入格式为 0 声道／0 Hz；新进程与先创建输出节点两种方式均复现同一 `kAudioUnitErr_NoConnection`。当前 macOS 只枚举到 Mac mini 扬声器，需要连接输入设备后再验证实际采集。没有关闭 AEC、回退无回声消除采集或改动旧语音实现。
