use canvas::CanvasRect;

use crate::motion::breathing::CornerGlow;
use crate::motion::working::WorkingSnake;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VisualPriority {
    Idle,
    Hover,
    Pressed,
    Dragging,
    Selected,
    Focused,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedNodeVisual {
    pub priority: VisualPriority,
    pub rect: CanvasRect,
    pub fill_color: u32,
    pub stroke_color: u32,
    pub status_color: u32,
    pub title_bar_color: u32,
    pub edge_highlight_color: u32,
    pub shadow_color: u32,
    pub shadow_y: f32,
    pub shadow_blur: f32,
    pub shadow_spread: f32,
    pub border_is_emphasized: bool,
    pub opacity: f32,
    pub corner_glow: Option<CornerGlow>,
    pub working_snake: Option<WorkingSnake>,
}
