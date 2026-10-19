# 助手模式右侧内容工作台

一期：项目文件与 Git、普通浏览器、图片/视频、Markdown/HTML。实现沿用现有 workspace 内容标签、Wry/WebKit 与编辑输入组件。用户授权本地编辑、暂存/取消暂存、撤销未暂存修改和提交；远程同步、分支切换、合并及 IDE 语言服务不在一期。

## 行为

- 文件标签跟随当前展开项目，允许可搜索项目选择；具体文件和网页标签继续保留自己的来源。
- 文件工作台右侧按需目录树和文件名筛选；窄抽屉在目录与正文间切换，避免嵌套窄栏。
- 本地变更区分已暂存/未暂存/未跟踪；分支比较为 merge-base 到 HEAD，不包含工作树编辑。
- 文件级暂存、批量暂存、取消暂存、明确确认后的已跟踪文件撤销；提交只使用索引，正常运行 Git hooks。
- 文本上限 1 MiB，UTF-8；保存比较读取版本并采用原子交换，在最后替换处发现冲突时恢复磁盘版本并保留恢复副本。
- Markdown/HTML 支持源码编辑与预览，保存后重建预览；图片支持适应/实际尺寸/缩放/平移，视频使用系统 WebView 控件。
- 本地内容经 token-scoped `norma-content` 协议读取，资源限制在来源项目，Git 内部和越界链接拒绝读取；关闭撤销 token。
- 文件和 Git IO 均后台执行，刷新单飞；工作台实例 epoch、根目录、相对路径共同拒绝迟到结果。
- 未保存离开动作保留原意图；打开抽屉提示保存/放弃/取消，完成后继续关闭、切换或退出。

## 分层与约束

`workspace::content` 提供目录、文本版本保存、Git 读取与本地操作；`app` 持有工作台状态、后台任务、预览和 GPUI 组件；`storage` 增加 `project_files` 标签类型和工作台 JSON 状态，旧标签原样迁移。

参考 `apple-design`：内容优先，复用现有抽屉动效，高频点击不加新动画；选择器复用 gpui-component 可搜索 Select，目录/变更列表使用虚拟化，窄抽屉切换目录避免遮挡正文。`ponytail`：不引入新的浏览器引擎或编辑器依赖。安装环境没有检索到 `gpui-*` skills，API 已按钉死 GPUI 0.2.2、gpui-component 0.5.1 源码与邻近实现验证。

当前边界：Git 功能要求来源为工作树根目录（包括 worktree）；普通目录仍可浏览编辑。图像格式及视频编解码支持受 WebKit/macOS 限制。不支持的文件提供系统打开入口。不会自动安装编解码器或改变用户 Git 配置。

## 验收映射

| 行为 | 证据入口 |
|---|---|
| 目录选项、越界/.git 别名、外部保存冲突、重命名、首次提交前取消暂存、删除父目录恢复、分支比较、hooks 拒绝 | `cargo test -p workspace` |
| 新标签与旧标签兼容、状态存储 | `cargo test -p storage` |
| 迟到内容拒绝、Markdown 保存后渲染更新 | `cargo test -p app --lib workbench::tests` |
| 本地资源关闭后失效、视频 Range | `cargo test -p app --lib local_content::tests` |
| 真窗口目录/编辑/Git/Markdown/HTML/图片/视频 | `bash scripts/desktop-e2e.sh content-workbench` |
| 发布编译与机械边界 | app check/build、文件行数/no-allow/storage/performance/video 护栏 |

当前完成状态以本任务最终验收记录为准。未执行的真实窗口、登录、播放生命周期与主题检查不能以测试存在或代码编译代替。
