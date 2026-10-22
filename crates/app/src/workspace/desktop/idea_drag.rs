//! 想法拖拽的纯状态机：起拖阈值、悬停命中、松手结果。不碰 GPUI、存储或时钟，
//! 调用方只在返回“变化”时才重绘，鼠标移动路径因此保持廉价。

#[cfg(test)]
#[path = "../../../tests/unit/desktop_idea_drag_tests.rs"]
mod tests;

/// 位移达到此值才算拖拽，更小的移动仍是点击。
const DRAG_THRESHOLD_PX: f32 = 5.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Rect {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) w: f32,
    pub(super) h: f32,
}

impl Rect {
    fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.x + self.w && y >= self.y && y <= self.y + self.h
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct CardTarget {
    pub(super) area_id: String,
    pub(super) node_id: u64,
    pub(super) rect: Rect,
}

/// 想法栏里一条待办行的窗口坐标矩形，按显示顺序排列。
#[derive(Clone, Debug, PartialEq)]
pub(super) struct RowSlot {
    pub(super) id: String,
    pub(super) rect: Rect,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum DropTarget {
    Existing {
        area_id: String,
        node_id: u64,
    },
    NewAgent,
    /// 栏内排序：放到 `before` 这条之前，`None` 为末尾。
    Reorder {
        before: Option<String>,
    },
}

/// 窗口坐标下的全部投放目标；拖拽开始时算一次，拖动中不再重算。
#[derive(Clone, Debug, PartialEq)]
pub(super) struct DragTargets {
    pub(super) cards: Vec<CardTarget>,
    pub(super) new_zone: Rect,
    /// 画布可见区域：卡片在区域外（被抽屉遮住或滚出）不可命中。
    pub(super) canvas: Rect,
    /// 想法栏的可见列表区域：指针留在这里是排序，不是派发。
    pub(super) list: Rect,
    pub(super) rows: Vec<RowSlot>,
}

impl DragTargets {
    /// 没有展开项目：只有想法栏可排序。其余目标用无穷远的空矩形，任何有限坐标（含原点）都命不中。
    pub(super) fn list_only(list: Rect, rows: Vec<RowSlot>) -> Self {
        let nowhere = Rect {
            x: f32::NEG_INFINITY,
            y: f32::NEG_INFINITY,
            w: 0.0,
            h: 0.0,
        };
        Self {
            cards: Vec::new(),
            new_zone: nowhere,
            canvas: nowhere,
            list,
            rows,
        }
    }

    pub(super) fn hit(&self, x: f32, y: f32) -> Option<DropTarget> {
        if self.new_zone.contains(x, y) {
            return Some(DropTarget::NewAgent);
        }
        if self.canvas.contains(x, y) {
            return self
                .cards
                .iter()
                .find(|card| card.rect.contains(x, y))
                .map(|card| DropTarget::Existing {
                    area_id: card.area_id.clone(),
                    node_id: card.node_id,
                });
        }
        if self.list.contains(x, y) && !self.rows.is_empty() {
            // 指针在哪一行的上半部分，就插到那一行之前；都不在则插到末尾。
            let before = self
                .rows
                .iter()
                .find(|row| y < row.rect.y + row.rect.h / 2.0)
                .map(|row| row.id.clone());
            return Some(DropTarget::Reorder { before });
        }
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DragEvent {
    /// 未过阈值，什么也没变。
    None,
    Started,
    /// 悬停目标变了，需要重绘高亮。
    HoverChanged,
    /// 只是位置移动，预览跟手即可。
    Moved,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum DragOutcome {
    /// 没有真正拖动：当作点击。
    Click,
    Deliver {
        target: DropTarget,
        /// 松手点（落在目标上）。
        at: (f32, f32),
        /// 按下点，即想法行所在位置：流星从这里出发。
        from: (f32, f32),
    },
    /// 在想法栏内松手：按新位置排序。
    Reorder { before: Option<String> },
    /// 落空或取消：预览弹回原行。
    Return,
}

#[derive(Clone, Debug)]
pub(super) struct IdeaDrag {
    pub(super) idea_id: String,
    start: (f32, f32),
    pos: (f32, f32),
    grab: (f32, f32),
    active: bool,
    hover: Option<DropTarget>,
}

impl IdeaDrag {
    /// `row_origin` 是想法行左上角，按下点相对它的偏移就是抓取偏移。
    pub(super) fn new(idea_id: String, start: (f32, f32), row_origin: (f32, f32)) -> Self {
        Self {
            idea_id,
            start,
            pos: start,
            grab: (start.0 - row_origin.0, start.1 - row_origin.1),
            active: false,
            hover: None,
        }
    }

    pub(super) fn is_active(&self) -> bool {
        self.active
    }

    pub(super) fn hover(&self) -> Option<&DropTarget> {
        self.hover.as_ref()
    }

    /// 预览条左上角：当前位置减去抓取偏移，抓住哪里就停在哪里。
    pub(super) fn ghost_origin(&self) -> (f32, f32) {
        (self.pos.0 - self.grab.0, self.pos.1 - self.grab.1)
    }

    pub(super) fn moved(&mut self, x: f32, y: f32, targets: &DragTargets) -> DragEvent {
        self.pos = (x, y);
        let travelled = (x - self.start.0).hypot(y - self.start.1);
        let started = !self.active && travelled >= DRAG_THRESHOLD_PX;
        if started {
            self.active = true;
        }
        if !self.active {
            return DragEvent::None;
        }
        let hover = targets.hit(x, y);
        let changed = hover != self.hover;
        self.hover = hover;
        match (started, changed) {
            (true, _) => DragEvent::Started,
            (false, true) => DragEvent::HoverChanged,
            (false, false) => DragEvent::Moved,
        }
    }

    pub(super) fn released(self, x: f32, y: f32, targets: &DragTargets) -> DragOutcome {
        if !self.active {
            return DragOutcome::Click;
        }
        match targets.hit(x, y) {
            Some(DropTarget::Reorder { before }) => DragOutcome::Reorder { before },
            Some(target) => DragOutcome::Deliver {
                target,
                at: (x, y),
                from: self.start,
            },
            None => DragOutcome::Return,
        }
    }

    pub(super) fn cancel(self) -> DragOutcome {
        if self.active {
            DragOutcome::Return
        } else {
            DragOutcome::Click
        }
    }
}
