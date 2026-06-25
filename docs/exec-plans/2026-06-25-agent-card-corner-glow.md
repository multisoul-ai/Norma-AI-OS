# Agent 卡片四角光晕 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 Agent 卡片现有的单色呼吸描边之上，叠加一层固定四色（橙/蓝/粉/紫）的四角光晕，运行中/等待批准时随呼吸相位在模糊半径与强度上起伏，复用现有呼吸基础设施，零新增定时器、零新增 GPUI 渲染层。

**Architecture:** `motion_breathing.rs` 新增纯函数 `resolve_corner_glow(phase, fade_value, state) -> Option<CornerGlow>` 和四个固定颜色常量；`motion.rs::NodeMotion::resolve()` 调用它，把结果放进 `ResolvedNodeVisual.corner_glow`；`chrome.rs::node_shadow` 在原有 `Vec<BoxShadow>` 基础上，按 `corner_glow` 追加 4 个偏移指向四角、颜色固定、`blur_radius`/`alpha` 随相位呼吸的 `BoxShadow`；`canvas_view.rs` 在调用处按屏幕像素尺寸裁剪是否绘制光晕。

**Tech Stack:** Rust, GPUI 0.2.2（`BoxShadow`/`Hsla`/`rgba`），现有项目测试规范（`CLAUDE.md` 第 7 节：每个 test 要有场景/数据构造/执行过程/预期结果的文档注释，且每个 assert 带失败信息）。

## Global Constraints

- 单个 Rust 源文件不得超过 500 行（`scripts/check-max-file-lines.sh`）；`motion.rs`（490 行）、`canvas_view.rs`（494 行）已接近上限，每个任务必须把净增行数控制在个位数。
- 禁止 `#[allow(...)]`（`scripts/check-no-allow.sh`）。
- 每个 `assert!`/`assert_eq!`/`assert_ne!` 必须带失败信息字符串。
- 每个测试函数上方要有文档注释，包含：场景、数据构造（含关键数值推导）、执行过程、预期结果（正/负断言）。
- 四角光晕固定四色，不随 Cool/Warm 状态或主题切换：左上橙 `0xf8903aff`、右上蓝 `0x00a6ffff`、左下粉 `0xff0056ff`、右下紫 `0x6500ffff`。
- `corner_glow` 仅在 Agent 卡片 + `breathing_display_state != Inactive` + `breathing_fade.value > EPSILON`（且非 reduce-motion）时为 `Some`；其余情况为 `None`，且 `None` 路径不得有任何额外分配/绘制。
- 不改变现有单色呼吸描边（`stroke_color`）逻辑与既有测试断言；[[2026-06-24-SPEC-agent-card-breathing-border]] 的验收标准必须保持通过。
- 改动范围：`crates/app/src/motion_breathing.rs`、`crates/app/src/motion.rs`、`crates/app/src/cards/chrome.rs`、`crates/app/src/canvas_view.rs` 及对应测试文件；不触碰 `canvas`/`terminal`/`runtime`/`storage`。
- 参考 spec：`docs/product-specs/2026-06-25-SPEC-agent-card-corner-glow.md`。

---

### Task 1: `motion_breathing.rs` 新增四色常量与 `resolve_corner_glow` 纯函数

**Files:**
- Modify: `crates/app/src/motion_breathing.rs`
- Test: `crates/app/tests/unit/motion_breathing_tests.rs`

**Interfaces:**
- Consumes: 无新依赖；新增 `use crate::motion_util::EPSILON;`（`EPSILON` 是 `pub(crate) const EPSILON: f32 = 0.0001`，定义于 `crates/app/src/motion_util.rs:10`）。
- Produces:
  - `pub struct CornerGlow { pub blur: f32, pub alpha: f32 }`（`Clone + Copy + Debug + PartialEq`）
  - `pub const CORNER_GLOW_TOP_LEFT: u32 = 0xf8903aff;`
  - `pub const CORNER_GLOW_TOP_RIGHT: u32 = 0x00a6ffff;`
  - `pub const CORNER_GLOW_BOTTOM_LEFT: u32 = 0xff0056ff;`
  - `pub const CORNER_GLOW_BOTTOM_RIGHT: u32 = 0x6500ffff;`
  - `pub fn corner_glow_envelope(phase: f32) -> f32`（0.0..=1.0）
  - `pub fn resolve_corner_glow(phase: f32, fade_value: f32, state: BreathingState) -> Option<CornerGlow>`
  - 这 4 个函数/常量/类型在 Task 2 中被 `motion.rs` 直接 `use`。

- [ ] **Step 1: 写失败测试 —— `corner_glow_envelope` 关键相位点**

在 `crates/app/tests/unit/motion_breathing_tests.rs` 末尾（文件内已有 `use super::*;` 和既有测试，直接追加新测试函数，不要新建文件）追加：

```rust
/// corner_glow_envelope 在 phase=0.0/0.25/0.5/0.75 应分别得到 0.5/1.0/0.5/0.0。
///
/// 数据构造（含关键数值推导）：
///   envelope(phase) = (sin(phase * 2π) + 1) / 2
///   phase=0.00 → sin(0)=0      → envelope=0.5
///   phase=0.25 → sin(π/2)=1    → envelope=1.0
///   phase=0.50 → sin(π)=0      → envelope=0.5
///   phase=0.75 → sin(3π/2)=-1  → envelope=0.0
///
/// 执行过程：
///   1. 分别用 4 个相位点调用 corner_glow_envelope
///
/// 预期结果：
///   - 正断言：四个点的返回值分别接近 0.5/1.0/0.5/0.0（容差 1e-4）
///   - 负断言：phase=0.25 的结果不应等于 phase=0.75 的结果（峰谷不应混淆）
#[test]
fn test_corner_glow_envelope_matches_sine_breathing_curve() {
    let at_0 = corner_glow_envelope(0.0);
    let at_quarter = corner_glow_envelope(0.25);
    let at_half = corner_glow_envelope(0.5);
    let at_three_quarter = corner_glow_envelope(0.75);

    assert!(
        (at_0 - 0.5).abs() < 1e-4,
        "phase=0.0 时 envelope 应接近 0.5，实际为 {at_0}"
    );
    assert!(
        (at_quarter - 1.0).abs() < 1e-4,
        "phase=0.25 时 envelope 应接近峰值 1.0，实际为 {at_quarter}"
    );
    assert!(
        (at_half - 0.5).abs() < 1e-4,
        "phase=0.5 时 envelope 应接近 0.5，实际为 {at_half}"
    );
    assert!(
        (at_three_quarter - 0.0).abs() < 1e-4,
        "phase=0.75 时 envelope 应接近谷值 0.0，实际为 {at_three_quarter}"
    );
    assert_ne!(
        at_quarter, at_three_quarter,
        "波峰(0.25)与波谷(0.75)的 envelope 不应相等"
    );
}

/// resolve_corner_glow 仅在 Active 态且 fade_value 足够大时返回 Some，其余情况返回 None。
///
/// 数据构造（含关键数值推导）：
///   EPSILON = 0.0001（crate 内呼吸淡入淡出判定阈值，与 motion.rs 共用同一常量）
///   phase 固定取 0.25（envelope 峰值 1.0，便于断言 blur/alpha 上界）
///   CORNER_GLOW_BLUR_MIN/MAX = 8.0/14.0（见 spec 4.3 节）
///
/// 执行过程：
///   1. state=Cool, fade_value=1.0 → 应为 Some，alpha=1.0*1.0=1.0，blur=14.0（envelope=1.0 时取上界）
///   2. state=Inactive, fade_value=1.0 → 应为 None（未激活）
///   3. state=Cool, fade_value=0.0（<=EPSILON）→ 应为 None（已完全淡出）
///
/// 预期结果：
///   - 正断言：场景 1 返回 Some，且 alpha≈1.0、blur≈14.0
///   - 正断言：场景 2、3 均返回 None
///   - 负断言：场景 1 的 blur 不应等于 BLUR_MIN（说明确实随 envelope 抬升到上界附近）
#[test]
fn test_resolve_corner_glow_gates_on_active_state_and_fade_value() {
    let active = resolve_corner_glow(0.25, 1.0, BreathingState::Cool)
        .expect("Cool 态且 fade_value=1.0 时应返回 Some");
    assert!(
        (active.alpha - 1.0).abs() < 1e-4,
        "fade_value=1.0、envelope=1.0 时 alpha 应接近 1.0，实际为 {}",
        active.alpha
    );
    assert!(
        (active.blur - 14.0).abs() < 1e-4,
        "envelope=1.0 时 blur 应接近上界 14.0，实际为 {}",
        active.blur
    );
    assert_ne!(
        active.blur, 8.0,
        "envelope=1.0 时 blur 不应停留在下界 8.0"
    );

    assert!(
        resolve_corner_glow(0.25, 1.0, BreathingState::Inactive).is_none(),
        "Inactive 态不应产生四角光晕"
    );
    assert!(
        resolve_corner_glow(0.25, 0.0, BreathingState::Cool).is_none(),
        "fade_value=0.0（已淡出完毕）不应产生四角光晕"
    );
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test -p app corner_glow --no-run`
Expected: 编译失败，报 `corner_glow_envelope`/`resolve_corner_glow`/`BreathingState` 未导入或未定义之类的错误（因为实现还不存在）。

- [ ] **Step 3: 实现 `motion_breathing.rs` 新增内容**

在 `crates/app/src/motion_breathing.rs` 顶部 `use` 区域（紧跟现有 `use std::time::Duration;` / `use ui::Theme;`）追加：

```rust
use crate::motion_util::EPSILON;
```

在文件末尾 `with_alpha` 函数之后、`#[cfg(test)]` 块之前，追加：

```rust
/// 四角光晕固定色：还原 Uiverse 参考 CSS 的橙/蓝/粉/紫，不随主题或 Cool/Warm 状态切换。
pub const CORNER_GLOW_TOP_LEFT: u32 = 0xf8903aff;
pub const CORNER_GLOW_TOP_RIGHT: u32 = 0x00a6ffff;
pub const CORNER_GLOW_BOTTOM_LEFT: u32 = 0xff0056ff;
pub const CORNER_GLOW_BOTTOM_RIGHT: u32 = 0x6500ffff;

const CORNER_GLOW_BLUR_MIN: f32 = 8.0;
const CORNER_GLOW_BLUR_MAX: f32 = 14.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CornerGlow {
    pub blur: f32,
    pub alpha: f32,
}

/// 呼吸包络：phase∈[0,1) → 0.0..=1.0，与 breathing_alpha 共享同一个 sin 公式，
/// 但映射到 0..1 而不是 0x66..0xFF 的字节区间，供 blur 与 alpha 共同复用。
pub fn corner_glow_envelope(phase: f32) -> f32 {
    let s = (phase * std::f32::consts::TAU).sin();
    (s + 1.0) / 2.0
}

/// 给定相位、呼吸淡入淡出强度与当前呼吸态，解析出四角光晕的模糊/强度参数。
/// Inactive 或已完全淡出（fade_value <= EPSILON）时返回 None，渲染层应据此完全跳过四角光晕。
pub fn resolve_corner_glow(phase: f32, fade_value: f32, state: BreathingState) -> Option<CornerGlow> {
    if state == BreathingState::Inactive || fade_value <= EPSILON {
        return None;
    }
    let envelope = corner_glow_envelope(phase);
    Some(CornerGlow {
        blur: CORNER_GLOW_BLUR_MIN + envelope * (CORNER_GLOW_BLUR_MAX - CORNER_GLOW_BLUR_MIN),
        alpha: fade_value * envelope,
    })
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test -p app corner_glow`
Expected: `test_corner_glow_envelope_matches_sine_breathing_curve` 和 `test_resolve_corner_glow_gates_on_active_state_and_fade_value` 均 PASS。

- [ ] **Step 5: 行数与 lint 检查**

Run: `wc -l crates/app/src/motion_breathing.rs && bash scripts/check-no-allow.sh`
Expected: 行数远低于 500（预计约 145 行）；no-allow 检查无输出/退出码 0。

- [ ] **Step 6: Commit**

```bash
git add crates/app/src/motion_breathing.rs crates/app/tests/unit/motion_breathing_tests.rs
git commit -m "feat(app): add corner glow envelope and resolver to motion_breathing"
```

---

### Task 2: `motion.rs` 把 `corner_glow` 接入 `ResolvedNodeVisual`

**Files:**
- Modify: `crates/app/src/motion.rs:8`（导入区）、`crates/app/src/motion.rs:52-66`（`ResolvedNodeVisual` 定义）、`crates/app/src/motion.rs:409-434`（`resolve()` 构造返回值处）
- Test: `crates/app/tests/unit/motion_corner_glow_tests.rs`（新建）

**Interfaces:**
- Consumes: Task 1 产出的 `resolve_corner_glow(phase, fade_value, state) -> Option<CornerGlow>` 与 `CornerGlow { blur, alpha }`（从 `crate::motion_breathing` 导入）。
- Produces:
  - `ResolvedNodeVisual.corner_glow: Option<CornerGlow>`（新字段，`pub`）。
  - 这个字段在 Task 3 中被 `chrome.rs::node_shadow(visual, zoom, corner_glow_visible)` 读取。

- [ ] **Step 1: 写失败测试 —— `resolve_node_visual` 输出 `corner_glow`**

新建 `crates/app/tests/unit/motion_corner_glow_tests.rs`：

```rust
use super::*;

fn rect(x: f32, y: f32, w: f32, h: f32) -> CanvasRect {
    CanvasRect::new(x, y, w, h)
}

/// CG-1: Agent 卡片处于 Cool 呼吸态时，resolve_node_visual 应产出非空 corner_glow。
///
/// 数据构造（含关键数值推导）：
///   node_id = 51，Cool 呼吸周期 = 2500ms。
///   breathing_fade 淡入时长 = MotionTokens::default().breathing_fade = 150ms；
///   tick(200ms) 后 fade 已完全淡入（200ms > 150ms），fade.value 应为 1.0。
///
/// 执行过程：
///   1. ensure_node 创建空闲节点
///   2. set_breathing_intent(Cool)
///   3. tick(200ms)，让 breathing_fade 淡入完成、breathing_phase 前进
///   4. resolve_node_visual 读取 corner_glow
///
/// 预期结果：
///   - 正断言：corner_glow 为 Some，且 alpha 在 (0.0, 1.0] 区间内
///   - 正断言：blur 落在 [8.0, 14.0] 区间内（CORNER_GLOW_BLUR_MIN/MAX）
///   - 负断言：corner_glow 不应为 None（Cool 态下不应缺失光晕）
#[test]
fn test_resolve_node_visual_emits_corner_glow_when_breathing_active() {
    let mut store = MotionStore::new(false);
    let theme = Theme::glass_dark();
    let node_id = 51;
    let node_rect = rect(0.0, 0.0, 100.0, 80.0);
    store.ensure_node(node_id, node_rect);

    store.set_breathing_intent(node_id, BreathingIntent::Cool);
    store.tick(Duration::from_millis(200));

    let visual = store.resolve_node_visual(node_id, node_rect, theme);
    let glow = visual
        .corner_glow
        .expect("Cool 呼吸态下 corner_glow 不应为 None");

    assert!(
        glow.alpha > 0.0 && glow.alpha <= 1.0,
        "corner_glow.alpha 应落在 (0.0, 1.0] 区间，实际为 {}",
        glow.alpha
    );
    assert!(
        (8.0..=14.0).contains(&glow.blur),
        "corner_glow.blur 应落在 [8.0, 14.0] 区间，实际为 {}",
        glow.blur
    );
    assert!(
        visual.corner_glow.is_some(),
        "Cool 呼吸态下 corner_glow 不应缺失"
    );
}

/// CG-2: 未激活/已落定的呼吸态下，resolve_node_visual 不应产出 corner_glow。
///
/// 数据构造（含关键数值推导）：
///   node_id = 52，初始 BreathingIntent::Inactive（从未进入呼吸态）。
///
/// 执行过程：
///   1. ensure_node 创建空闲节点（不调用 set_breathing_intent，保持默认 Inactive）
///   2. resolve_node_visual 读取 corner_glow
///
/// 预期结果：
///   - 正断言：corner_glow 为 None
///   - 负断言：stroke_color 不应受影响地仍是静态 node_stroke（确认本改动未触碰既有描边路径）
#[test]
fn test_resolve_node_visual_omits_corner_glow_when_inactive() {
    let mut store = MotionStore::new(false);
    let theme = Theme::glass_dark();
    let node_id = 52;
    let node_rect = rect(0.0, 0.0, 100.0, 80.0);
    store.ensure_node(node_id, node_rect);

    let visual = store.resolve_node_visual(node_id, node_rect, theme);

    assert!(
        visual.corner_glow.is_none(),
        "Inactive 呼吸态下 corner_glow 应为 None"
    );
    assert_eq!(
        visual.stroke_color, theme.node_stroke,
        "未激活呼吸时 stroke_color 应保持静态 node_stroke，不应被本次改动影响"
    );
}
```

在 `crates/app/src/motion.rs` 文件末尾的测试挂载区（紧跟现有两个 `#[cfg(test)] #[path = ...] mod ...;`）追加第三个挂载：

```rust
#[cfg(test)]
#[path = "../tests/unit/motion_corner_glow_tests.rs"]
mod corner_glow_tests;
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test -p app corner_glow --no-run`
Expected: 编译失败，提示 `ResolvedNodeVisual` 没有字段 `corner_glow`。

- [ ] **Step 3: 实现 `motion.rs` 改动**

修改 `crates/app/src/motion.rs:8`，把：

```rust
use crate::motion_breathing::{breathing_color_for_phase, BreathingIntent, BreathingState};
```

改为：

```rust
use crate::motion_breathing::{
    breathing_color_for_phase, resolve_corner_glow, BreathingIntent, BreathingState, CornerGlow,
};
```

在 `ResolvedNodeVisual` 定义（`crates/app/src/motion.rs:52-66`）的 `pub opacity: f32,` 字段后追加一行：

```rust
    pub corner_glow: Option<CornerGlow>,
```

在 `resolve()` 方法内，`let settle = self.completion_settle.value.clamp(0.0, 1.0);` 那一行之前（即紧跟在 `stroke_color` 计算完成之后）插入：

```rust
        let corner_glow =
            resolve_corner_glow(self.breathing_phase, self.breathing_fade.value, self.breathing_display_state);
```

在 `ResolvedNodeVisual { ... }` 构造字面量里，紧跟 `opacity: self.opacity.value,` 之前或之后追加一行：

```rust
            corner_glow,
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test -p app corner_glow`
Expected: `test_resolve_node_visual_emits_corner_glow_when_breathing_active` 和 `test_resolve_node_visual_omits_corner_glow_when_inactive` 均 PASS。

- [ ] **Step 5: 跑既有 motion 测试确认无回归**

Run: `cargo test -p app --lib`
Expected: 全部既有测试（含 `motion_tests.rs`、`motion_completion_tests.rs`、`motion_breathing_tests.rs`）依然 PASS，不因新增字段破坏。

- [ ] **Step 6: 行数检查**

Run: `wc -l crates/app/src/motion.rs`
Expected: 约 494 行，仍低于 500。

- [ ] **Step 7: Commit**

```bash
git add crates/app/src/motion.rs crates/app/tests/unit/motion_corner_glow_tests.rs
git commit -m "feat(app): resolve corner glow into ResolvedNodeVisual"
```

---

### Task 3: `chrome.rs::node_shadow` 渲染四角光晕

**Files:**
- Modify: `crates/app/src/cards/chrome.rs:1-72`

**Interfaces:**
- Consumes: `ResolvedNodeVisual.corner_glow: Option<CornerGlow>`（Task 2）；`CornerGlow { blur, alpha }`、四个颜色常量、来自 `crate::motion_breathing`。
- Produces:
  - `pub const CORNER_GLOW_MIN_SCREEN_PX: f32 = 32.0;`（Task 4 在 `canvas_view.rs` 引用，作为屏幕尺寸裁剪阈值）。
  - `pub fn node_shadow(visual: ResolvedNodeVisual, zoom: f32, corner_glow_visible: bool) -> Vec<BoxShadow>`（签名变更：原来是 `node_shadow(visual: ResolvedNodeVisual)`，Task 4 必须同步更新唯一调用点）。

- [ ] **Step 1: 写失败测试 —— `node_shadow` 在四角光晕可见时追加 4 个 shadow**

在 `crates/app/src/cards/chrome.rs` 文件末尾追加测试模块（紧跟 `EmptyCard` 之后的现有内容，文件目前没有 `#[cfg(test)]` 块，新增一个）：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn base_visual(corner_glow: Option<CornerGlow>) -> ResolvedNodeVisual {
        ResolvedNodeVisual {
            priority: VisualPriority::Idle,
            rect: CanvasRect::new(0.0, 0.0, 100.0, 80.0),
            fill_color: 0x000000ff,
            stroke_color: 0xffffffff,
            status_color: 0xffffffff,
            title_bar_color: 0x000000ff,
            edge_highlight_color: 0xffffffff,
            shadow_color: 0x000000aa,
            shadow_y: 4.0,
            shadow_blur: 20.0,
            shadow_spread: -10.0,
            border_is_emphasized: false,
            opacity: 1.0,
            corner_glow,
        }
    }

    /// CS-1: corner_glow=Some 且 corner_glow_visible=true 时，node_shadow 应在基础阴影之外追加 4 个角光晕。
    ///
    /// 数据构造（含关键数值推导）：
    ///   base_visual 的 priority=Idle（不触发 Focused 专属光晕），corner_glow.alpha=0.8、blur=10.0。
    ///   zoom=1.0，故角光晕的 offset/blur 不被缩放，便于直接断言数值。
    ///
    /// 执行过程：
    ///   1. 构造 corner_glow=Some(CornerGlow{blur:10.0, alpha:0.8})
    ///   2. 调用 node_shadow(visual, 1.0, true)
    ///
    /// 预期结果：
    ///   - 正断言：返回的 Vec 长度为 5（1 个基础阴影 + 4 个角光晕）
    ///   - 正断言：新增的 4 个 shadow 的 blur_radius 均为 10.0px
    ///   - 负断言：返回长度不应是 1（说明角光晕确实被追加）
    #[test]
    fn test_node_shadow_appends_four_corner_glows_when_visible() {
        let visual = base_visual(Some(CornerGlow {
            blur: 10.0,
            alpha: 0.8,
        }));

        let shadows = node_shadow(visual, 1.0, true);

        assert_eq!(
            shadows.len(),
            5,
            "Idle 优先级 + 可见的四角光晕应产出 5 个 shadow（1 基础 + 4 角），实际为 {}",
            shadows.len()
        );
        for glow_shadow in &shadows[1..] {
            assert_eq!(
                f32::from(glow_shadow.blur_radius),
                10.0,
                "四角光晕的 blur_radius 应等于 corner_glow.blur=10.0"
            );
        }
        assert_ne!(shadows.len(), 1, "追加四角光晕后不应仍只有 1 个基础阴影");
    }

    /// CS-2: corner_glow=None 或 corner_glow_visible=false 时，node_shadow 不应追加角光晕。
    ///
    /// 数据构造（含关键数值推导）：
    ///   场景 A：corner_glow=None（呼吸未激活）
    ///   场景 B：corner_glow=Some(...) 但 corner_glow_visible=false（屏幕尺寸低于裁剪阈值）
    ///
    /// 执行过程：
    ///   1. 场景 A：node_shadow(visual_without_glow, 1.0, true)
    ///   2. 场景 B：node_shadow(visual_with_glow, 1.0, false)
    ///
    /// 预期结果：
    ///   - 正断言：两个场景返回长度都为 1（只有基础阴影）
    ///   - 负断言：长度不应为 5（不应误触发角光晕）
    #[test]
    fn test_node_shadow_skips_corner_glow_when_absent_or_culled() {
        let without_glow = node_shadow(base_visual(None), 1.0, true);
        let culled = node_shadow(
            base_visual(Some(CornerGlow {
                blur: 10.0,
                alpha: 0.8,
            })),
            1.0,
            false,
        );

        assert_eq!(
            without_glow.len(),
            1,
            "corner_glow=None 时应只有基础阴影，实际为 {}",
            without_glow.len()
        );
        assert_eq!(
            culled.len(),
            1,
            "corner_glow_visible=false 时应只有基础阴影，实际为 {}",
            culled.len()
        );
        assert_ne!(without_glow.len(), 5, "corner_glow=None 时不应误产出角光晕");
        assert_ne!(culled.len(), 5, "裁剪命中时不应误产出角光晕");
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test -p app node_shadow --no-run`
Expected: 编译失败，提示 `node_shadow` 期望 1 个参数，实际传了 3 个；以及 `ResolvedNodeVisual` 没有 `corner_glow` 字段的初始化报错已在 Task 2 解决，这里应只报参数数量不匹配。

- [ ] **Step 3: 实现 `chrome.rs` 改动**

把 `crates/app/src/cards/chrome.rs:4` 的 import 改为（新增 `BoxShadow` 已存在，新增四色常量与 `CornerGlow`）：

```rust
use canvas::{CanvasNode, CanvasRect, NodeType};
use gpui::{div, point, prelude::*, px, rgba, AnyElement, BoxShadow, Div, FontWeight, IntoElement};
use runtime::{NodeResize, Runtime};
use ui::Theme;

use super::{Card, CardBodyContext};
use crate::motion::{ResolvedNodeVisual, VisualPriority};
use crate::motion_breathing::{
    CornerGlow, CORNER_GLOW_BOTTOM_LEFT, CORNER_GLOW_BOTTOM_RIGHT, CORNER_GLOW_TOP_LEFT,
    CORNER_GLOW_TOP_RIGHT,
};
```

把 `crates/app/src/cards/chrome.rs:54-72` 的 `node_shadow` 函数整体替换为：

```rust
/// 四角光晕在屏幕上的最小可见尺寸（px）；卡片缩小到这个尺寸以下时跳过光晕绘制。
pub const CORNER_GLOW_MIN_SCREEN_PX: f32 = 32.0;

const CORNER_GLOW_OFFSET_PX: f32 = 6.0;
const CORNER_GLOW_SPREAD_PX: f32 = -2.0;

pub fn node_shadow(
    visual: ResolvedNodeVisual,
    zoom: f32,
    corner_glow_visible: bool,
) -> Vec<BoxShadow> {
    let mut shadows = Vec::with_capacity(6);
    shadows.push(BoxShadow {
        color: rgba(visual.shadow_color).into(),
        offset: point(px(0.0), px(visual.shadow_y)),
        blur_radius: px(visual.shadow_blur),
        spread_radius: px(visual.shadow_spread),
    });

    if visual.priority == VisualPriority::Focused {
        shadows.push(BoxShadow {
            color: rgba(with_alpha(visual.stroke_color, 0x38)).into(),
            offset: point(px(0.0), px(0.0)),
            blur_radius: px(18.0),
            spread_radius: px(0.0),
        });
    }

    if corner_glow_visible {
        if let Some(glow) = visual.corner_glow {
            push_corner_glow(&mut shadows, glow, zoom);
        }
    }

    shadows
}

fn push_corner_glow(shadows: &mut Vec<BoxShadow>, glow: CornerGlow, zoom: f32) {
    let offset = CORNER_GLOW_OFFSET_PX * zoom;
    let blur = px(glow.blur * zoom);
    let spread = px(CORNER_GLOW_SPREAD_PX * zoom);
    let corners = [
        (CORNER_GLOW_TOP_LEFT, -offset, -offset),
        (CORNER_GLOW_TOP_RIGHT, offset, -offset),
        (CORNER_GLOW_BOTTOM_LEFT, -offset, offset),
        (CORNER_GLOW_BOTTOM_RIGHT, offset, offset),
    ];
    for (color, dx, dy) in corners {
        shadows.push(BoxShadow {
            color: rgba(with_glow_alpha(color, glow.alpha)).into(),
            offset: point(px(dx), px(dy)),
            blur_radius: blur,
            spread_radius: spread,
        });
    }
}

fn with_glow_alpha(color: u32, intensity: f32) -> u32 {
    let intensity = intensity.clamp(0.0, 1.0);
    let base_alpha = (color & 0xff) as f32;
    with_alpha(color, (base_alpha * intensity).round() as u8)
}
```

注意：本步骤里测试用到的 `node_shadow(visual, 1.0, true)` 中 `zoom=1.0` 时 `blur = px(glow.blur * 1.0) = px(10.0)`，与 Step 1 断言一致。

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test -p app node_shadow`
Expected: `test_node_shadow_appends_four_corner_glows_when_visible` 与 `test_node_shadow_skips_corner_glow_when_absent_or_culled` 均 PASS。

- [ ] **Step 5: 行数检查**

Run: `wc -l crates/app/src/cards/chrome.rs`
Expected: 约 175 行，远低于 500。

- [ ] **Step 6: Commit**

```bash
git add crates/app/src/cards/chrome.rs
git commit -m "feat(app): render four corner glow shadows in node_shadow"
```

---

### Task 4: `canvas_view.rs` 接入屏幕尺寸裁剪并更新调用点

**Files:**
- Modify: `crates/app/src/canvas_view.rs:14`（import）、`crates/app/src/canvas_view.rs:341`（调用点）

**Interfaces:**
- Consumes: Task 3 产出的 `node_shadow(visual, zoom, corner_glow_visible)` 新签名与 `CORNER_GLOW_MIN_SCREEN_PX`（均从 `crate::cards::chrome` 导入）。
- Produces: 无新增公共接口；这是该功能链路的最后一环，把 `w`/`h`（已在 `crates/app/src/canvas_view.rs:317-318` 计算好的屏幕像素尺寸）与阈值比较后传入 `node_shadow`。

- [ ] **Step 1: 确认当前调用点编译失败（因为 Task 3 已改签名）**

Run: `cargo check -p app`
Expected: 编译报错，`crates/app/src/canvas_view.rs:341` 处 `node_shadow(visual)` 参数数量不匹配（Task 3 已经把签名改成 3 个参数）。这一步本身就是"失败测试"——用编译器的类型检查代替手写单测，因为这里是纯粹的接线（wiring），没有新的纯函数逻辑需要单独测。

- [ ] **Step 2: 修改 import**

把 `crates/app/src/canvas_view.rs:14` 的：

```rust
use crate::cards::chrome::{node_shadow, render_task_badge, TITLE_BAR_H};
```

改为：

```rust
use crate::cards::chrome::{node_shadow, render_task_badge, CORNER_GLOW_MIN_SCREEN_PX, TITLE_BAR_H};
```

- [ ] **Step 3: 修改调用点**

把 `crates/app/src/canvas_view.rs:341` 的：

```rust
            .shadow(node_shadow(visual))
```

改为：

```rust
            .shadow(node_shadow(
                visual,
                vp.zoom,
                w >= CORNER_GLOW_MIN_SCREEN_PX && h >= CORNER_GLOW_MIN_SCREEN_PX,
            ))
```

（`w`、`h` 已在 `crates/app/src/canvas_view.rs:317-318` 定义为 `let w = visual.rect.size.x * vp.zoom; let h = visual.rect.size.y * vp.zoom;`，本步骤直接复用，不新增变量。）

- [ ] **Step 4: 编译确认通过**

Run: `cargo check -p app`
Expected: 编译成功，无错误。

- [ ] **Step 5: 跑全量 app 测试确认无回归**

Run: `cargo test -p app`
Expected: 全部测试 PASS（包括 Task 1-3 新增的测试与既有的 motion/canvas_view 测试）。

- [ ] **Step 6: 行数与依赖边界检查**

Run: `wc -l crates/app/src/canvas_view.rs && cargo tree -p canvas -p terminal -p runtime | grep -i gpui`
Expected: 行数约 495 行（低于 500）；`gpui` 依赖检查无输出（domain crate 边界未被破坏，本任务也完全没有改 domain crate）。

- [ ] **Step 7: 全量 lint**

Run: `bash scripts/check-max-file-lines.sh && bash scripts/check-no-allow.sh`
Expected: 两个脚本均无错误输出，退出码 0。

- [ ] **Step 8: Commit**

```bash
git add crates/app/src/canvas_view.rs
git commit -m "feat(app): wire corner glow screen-size culling into canvas_view"
```

---

### Task 5: 端到端验证与收尾

**Files:** 无新增/修改文件；本任务只做验证。

**Interfaces:** 无。

- [ ] **Step 1: 全量 workspace 测试**

Run: `cargo test --workspace`
Expected: 全部 crate 测试 PASS，无回归（含 `canvas`/`terminal`/`runtime`/`storage`/`ui`/`app`）。

- [ ] **Step 2: app release 构建检查（确认 GPUI 渲染路径无编译期问题）**

Run: `cargo build -p app --release`
Expected: 编译成功。

- [ ] **Step 3: 人工运行验证（按 spec 第 5 节验收标准逐条核对）**

Run: `cargo run -p app`，注册/打开一个 Agent 卡片并让它进入 Thinking/RunningTool 或 WaitingApproval 状态（或用现有测试夹具触发 `AgentTaskState` 更新）。

Expected（逐条对照 `docs/product-specs/2026-06-25-SPEC-agent-card-corner-glow.md` 第 5 节）：
  - 卡片四角可见橙/蓝/粉/紫光晕，随呼吸节奏在模糊与强度上起伏。
  - `Thinking`/`RunningTool` 与 `WaitingApproval` 之间四角颜色不变，只有节奏快慢不同（2.5s vs 1.2s）。
  - 切到 `Ready`/`Done`/`Failed`/`Ended` 后光晕在约 150ms 内随既有 `breathing_fade` 一起淡出。
  - 设置 `MULTISOUL_REDUCE_MOTION=1` 重新运行后不出现光晕。
  - 缩小画布到节点屏幕尺寸低于 32px 时光晕不绘制；放大回去后恢复，无闪烁残留。

如果没有可交互验证环境（无显示器/无法操作 GUI），明确告知用户："本任务无法在当前环境里做可视化人工验证，已完成的是 `cargo test --workspace` 和 `cargo build -p app --release` 的自动化验证；视觉效果需要你本地运行 `cargo run -p app` 用真实 Agent 任务状态切换来确认。" 不要在没有实际看到画面的情况下声称视觉效果已确认。

- [ ] **Step 4: 更新 SPEC 关联（可选，若发现实现期间对 spec 的任何取舍偏差）**

如果 Step 3 验证中发现某条验收标准实现细节与 spec 描述有出入（例如 32px 阈值实际效果不理想需要调整数值），在 `docs/product-specs/2026-06-25-SPEC-agent-card-corner-glow.md` 对应章节做最小修订并说明原因，而不是悄悄改代码却不同步文档。

- [ ] **Step 5: 最终 Commit（若 Step 4 有文档修订）**

```bash
git add docs/product-specs/2026-06-25-SPEC-agent-card-corner-glow.md
git commit -m "docs(spec): reconcile corner glow spec with implementation findings"
```

若 Step 4 无修订，跳过本步骤，不创建空提交。
