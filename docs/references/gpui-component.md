# GPUI Component（Longbridge）

Norma OS 的 GUI 控件优先使用 [gpui-component](https://longbridge.github.io/gpui-component/)，非必要不要用裸 `div()` / 自绘控件重复造轮子。

## 与 GPUI 的关系

| 层 | Crate | 职责 |
|----|-------|------|
| 框架 | `gpui =0.2.2` | 窗口、渲染、布局、事件、`div()` 原语 |
| 组件库 | `gpui-component =0.5.1` | Button、Input、Dialog、Table 等现成控件 |

`gpui-component` 构建在 `gpui` 之上。本仓库钉死 crates.io 的 `0.5.1`，因其依赖 `gpui 0.2.2`，与 workspace pin 对齐。**不要**改用文档里的 Zed git 版 `gpui`，那会冲掉版本钉死。

## 接入约定

1. 启动时调用 `ui::init_components(cx)`（`Application::run` 与 `MultisoulApp::build` 已调用，幂等）。
2. 在 `crates/app` / `crates/ui` 写 GUI 时：
   - 优先：`use ui::component::button::Button;`（或 `ui::component::...`）
   - 其次：本地 `gpui-*` skills + 现有样式
   - 仅当组件库没有对应能力、或与画布/终端等域渲染强耦合时，才用 GPUI 原语自建
3. Dialog / Notification / Sheet 等需要窗口根为 `Root` 的能力：新增窗口或改造现有窗口时再包 `gpui_component::Root`；不要为了用 Button 而大改窗口树。
4. 图标可选依赖 `gpui-component-assets`；未接入前优先用文字 label，或沿用 `crates/ui` 现有 icons。

## 文档

- 组件 API：[Button 等](https://longbridge.github.io/gpui-component/docs/components/button)
- 仓库：[longbridge/gpui-component](https://github.com/longbridge/gpui-component)
