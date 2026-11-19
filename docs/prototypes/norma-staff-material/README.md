# 幕僚布局原型（2026-11-13）

用户已选择 **1 · 双栏工作台**，正在迁移到真实 App。保留此原型供对照试用。

- 双栏工作台：任务列表始终可见，适合频繁切换任务；占用固定侧栏宽度。
- 窄轨专注：按需展开任务列表，留出更多阅读空间；切换任务多一步。
- 任务收件箱：先浏览多任务状态再处理；并行阅读对话的空间较少。

布局按 Google Material 的层级、调性表面与状态反馈设计：工作空间和视图导航只出现一次，项目筛选归任务列表，执行状态在任务内展示。macOS 的 Cmd+Enter、Escape 与键盘焦点习惯保留。

## 运行

```sh
python3 -m http.server 8850 --bind 127.0.0.1 --directory docs/prototypes/norma-staff-material
```

打开 `http://127.0.0.1:8850/?v=1`；底部选择器或数字 1/2/3 切换。支持工作空间/项目/状态筛选、任务草稿隔离、新任务、执行计划演示、浅深色、文件预览及窄窗导航。

**所有数据与执行过程均为浏览器内演示，不会派发真实任务。**

## 原型验证

先用 Jev Browser 检查实际渲染与基本交互；可重复的行为验收使用本机已安装 Chrome + Playwright，不安装新依赖：

```sh
PLAYWRIGHT_MODULE=/path/to/installed/playwright node docs/prototypes/norma-staff-material/verify.cjs
```

已通过 3 种布局的 14 组行为检查：唯一导航、详情、画布返回、状态/项目/搜索、草稿隔离、采用后才执行、计划推进、任务重开、工作空间隔离、新任务、浅色与 600px 窄窗。控制台无错误。证据位于 `artifacts/staff-m1/material/`，它们只证明原型；真实 App 另外使用 staff desktop-e2e `--visual` 验收。

设计参考：https://developer.android.com/design/ui/mobile/guides/layout-and-content/layout-and-nav-patterns
