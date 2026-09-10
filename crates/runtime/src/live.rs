//! Live Node 抽象：runtime 层用 NodeId 管生命周期，保持零 gpui。

use std::fmt;
use std::sync::Arc;
use terminal::{Cell, GridSnapshot, KeyInput, PtyError, TerminalStatus};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Caps {
    pub focusable: bool,
    pub accepts_key: bool,
    pub term_resize: bool,
}

impl Caps {
    pub const NONE: Caps = Caps {
        focusable: false,
        accepts_key: false,
        term_resize: false,
    };

    pub const TERM: Caps = Caps {
        focusable: true,
        accepts_key: true,
        term_resize: true,
    };
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeInput {
    Key(KeyInput),
    /// 普通文本输入（含 IME commit）：按原始 UTF-8 写入 PTY，不套用 paste 协议。
    Text(String),
    /// 显式粘贴：根据终端声明的 bracketed-paste mode 编码后写入 PTY。
    Paste(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeResize {
    TermGrid { cols: u16, rows: u16 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeScroll {
    TermLines(i32),
    TermWheel { lines: i32, row: u16, col: u16 },
}

#[derive(Debug, Clone)]
pub enum NodeView {
    Term {
        snapshot: Arc<GridSnapshot>,
        revision: u64,
        status: TerminalStatus,
    },
}

#[derive(Debug)]
pub enum NodeError {
    MissingNode,
    UnsupportedInput,
    UnsupportedResize,
    UnsupportedScroll,
    Pty(PtyError),
}

impl fmt::Display for NodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeError::MissingNode => write!(f, "live node missing"),
            NodeError::UnsupportedInput => write!(f, "live node does not support this input"),
            NodeError::UnsupportedResize => write!(f, "live node does not support this resize"),
            NodeError::UnsupportedScroll => write!(f, "live node does not support scrolling"),
            NodeError::Pty(error) => write!(f, "{error}"),
        }
    }
}

impl From<PtyError> for NodeError {
    fn from(error: PtyError) -> Self {
        NodeError::Pty(error)
    }
}

pub trait LiveNode {
    fn caps(&self) -> Caps;
    fn drain(&mut self) -> bool;
    fn terminal_status(&self) -> Option<TerminalStatus> {
        None
    }
    fn terminal_bracketed_paste_enabled(&self) -> bool {
        false
    }
    fn resize(&mut self, resize: NodeResize) -> Result<(), NodeError>;
    fn input(&mut self, input: NodeInput) -> Result<(), NodeError>;
    fn scroll(&mut self, _scroll: NodeScroll) -> Result<bool, NodeError> {
        Err(NodeError::UnsupportedScroll)
    }
    fn view(&self) -> Option<NodeView>;
    fn kill(&mut self);
    fn child_pid(&self) -> Option<u32> {
        None
    }
    fn child_process_observer(&self) -> Option<terminal::ChildProcessObserver> {
        None
    }
    /// 终端节点读取绝对行范围 `[start_abs, end_abs]`（含两端）的中性 Cell 行数据。
    /// 非终端节点返回 `None`。绝对行坐标系见 [`terminal::GridSnapshot`]。
    fn terminal_rows(&self, _start_abs: usize, _end_abs: usize) -> Option<Vec<Vec<Cell>>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Caps::TERM 描述 terminal-backed live node 的交互能力。
    ///
    /// 数据构造（含关键数值推导）：
    ///   Caps::TERM 是常量能力集，不依赖运行时节点数量
    ///   focusable/accepts_key/term_resize 三个布尔位都应为 true
    ///
    /// 执行过程：
    ///   1. 读取 Caps::TERM
    ///   2. 逐项检查它是否覆盖终端节点当前需要的能力
    ///
    /// 预期结果：
    ///   - 正断言：TERM 可聚焦、可接收键盘、可按 terminal grid resize
    ///   - 负断言：TERM 不应等于 Caps::NONE
    #[test]
    fn test_term_caps_cover_focus_key_and_resize() {
        let caps = Caps::TERM;

        assert!(caps.focusable, "TERM live node 应可聚焦");
        assert!(caps.accepts_key, "TERM live node 应接收键盘输入");
        assert!(
            caps.term_resize,
            "TERM live node 应支持按 terminal grid rows/cols resize"
        );
        assert_ne!(
            caps,
            Caps::NONE,
            "TERM 能力不应退化为 NONE，否则节点无法交互"
        );
    }
}
