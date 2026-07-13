# Settings 页面视觉与导航刷新实施计划

**Spec:** [`docs/product-specs/2026-07-13-settings-visual-refresh.md`](../product-specs/2026-07-13-settings-visual-refresh.md)

## 约束

- 只改 Settings 呈现与自适应布局；不改任何配置值、持久化 key 或副作用处理器。
- 每个切片遵循 red → green：先补聚焦测试，再写最小实现。
- 使用现有 `SettingsE2eSnapshot` 和 `debug_selector` 作为公开验收 seam；不以私有渲染细节替代用户行为测试。
- 导航继续保留稳定 selector；因 `gpui-component` 的 Sidebar API 不能暴露该 selector 时，用 GPUI 原语承载该 seam，同时复用组件库图标与已有组件主题。

## 切片

1. 为 Settings 布局添加宽/中/窄三档模式的失败测试：窄模式必须将导航置顶并保证内容有正宽度；宽模式仍为左侧导航。
2. 实现布局模式与分组导航，保留五个入口的 id、点击处理和 Voice 模型状态刷新。
3. 用 `gpui-component` 的线性图标替换字符图标，并将关闭按钮、导航选中态和面板层级收敛为深色工作台风格。
4. 调整内容区、Keyboard、Voice、Agents、Hooks、Wallpaper 的间距与卡片表面，使窄宽度下可换行或滚动而不挤压核心操作。
5. 新增/更新 e2e：依次点击五个导航入口并断言每次保持 Settings 打开且可观察到原内容；回归现有 Voice/Wallpaper 流程。

## 验证

```bash
cargo test -p app
cargo test -p e2e
cargo check -p app
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
bash scripts/check-agent-guide-lines.sh
```
