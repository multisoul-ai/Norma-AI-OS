# UI 交互状态反馈规范

本规范适用于菜单项、按钮、图标按钮、可点击列表行、Tab、分段控件、Toggle，以及其他可由鼠标、触控板或键盘触发的用户界面元素。目标是让用户在操作提交前就能确认命中目标，并在操作过程中持续理解当前状态。

## 强制状态

交互组件必须根据能力覆盖下列状态；不适用的状态需在实现或评审中说明原因。

| 状态 | 强制反馈 |
| --- | --- |
| Rest | 可识别为可交互元素；不能只依赖 hover 才暴露用途。 |
| Hover | 鼠标进入后立即改变背景、边框、前景色或材质中的至少一项；变化必须在当前全部主题下可辨识。 |
| Pressed / Active | 按下期间提供比 hover 更明确、且与 hover 不同的反馈。 |
| Keyboard focus | 可键盘到达的控件必须有明确焦点标记；焦点不能只靠颜色含义表达。 |
| Selected / Toggled | 对持续选中或开关状态使用稳定标记，并与瞬时 hover、pressed 区分。 |
| Disabled | 同时禁止动作并降低视觉强调；不能只改变颜色却仍响应点击。 |

Hover 是桌面指针的预提交反馈，不能替代 focus、pressed 或 selected；触控路径也不能把 hover 作为理解或完成操作的唯一条件。

## GPUI 实现要求

1. 优先使用 `ui::component` 中已完整实现交互状态的组件。
2. 使用裸 GPUI 元素时，可点击区域至少应声明 `cursor_pointer()`、`.hover(...)` 和 `.active(...)`；可持续选中的组件还必须显式渲染 selected/toggled 状态。
3. Hover 应作用于完整 hitbox，而不是只作用于图标或文字子元素，避免鼠标在同一菜单行内移动时反馈闪烁。
4. 颜色必须来自 `Theme` token。浅色面板不得使用近白色 `edge_highlight` 作为唯一 hover 覆盖；深浅主题都应验证合成后的可辨识度。
5. Hover 不要求动画。高频菜单和按钮默认即时切换颜色或边框；只有动效能帮助理解状态或空间关系时才增加，并遵循 Reduce Motion。
6. Hover/pressed 样式不得改变控件尺寸、padding 或布局位置，避免命中目标在交互中跳动。

参考模式：

```rust
div()
    .cursor_pointer()
    .border_1()
    .border_color(rgba(0xffffff00))
    .hover(move |style| {
        style
            .bg(rgba(hover_fill))
            .border_color(rgba(hover_border))
    })
    .active(move |style| style.bg(rgba(pressed_fill)))
```

## 验收与测试

每个新增或修改的交互组件必须提供与风险相称的证据：

- 聚焦单元测试：验证 rest、hover、pressed/selected token 不相同；关键状态逐一断言，不能用“集合两侧相等”一类恒真条件。
- 主题测试：至少覆盖默认浅色主题和一个深色主题；颜色叠加后不能与容器背景相同或近似不可见。
- 用户流程 e2e：通过真实 debug bounds 移入或点击完整 hitbox，验证交互不会关闭错误菜单、改变布局或触发相邻项。
- 真实 App 验收：页面、布局、样式或视觉状态改动必须用 Computer Use 操作真实 App，检查移入、移出、按下和选中状态；headless 测试不能替代该步骤。
- 代码评审：逐项检查 hover、keyboard focus、pressed、selected/toggled、disabled；缺失项必须有明确“不适用”说明。

## 禁止模式

- 只有 `on_click`，没有 hover/pressed/focus 反馈。
- Hover 色与容器背景同色，或仅改变无法在某个主题中辨识的透明白色。
- 用 hover 代表持续选中，鼠标移出后用户无法知道当前选择。
- 通过改变 padding、边框宽度或尺寸制造反馈，导致布局跳动。
- 只在静态截图或源码字符串中验证视觉效果，却没有真实 App 操作验收。
