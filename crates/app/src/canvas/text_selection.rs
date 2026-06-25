use canvas::NodeId;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct TextPoint {
    pub row: usize,
    pub col: usize,
}

impl TextPoint {
    pub fn new(row: usize, col: usize) -> Self {
        Self { row, col }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextRange {
    pub anchor: TextPoint,
    pub head: TextPoint,
}

impl TextRange {
    pub fn new(anchor: TextPoint, head: TextPoint) -> Self {
        Self { anchor, head }
    }

    pub fn is_empty(self) -> bool {
        self.anchor == self.head
    }

    pub fn normalized(self) -> NormalizedTextRange {
        if self.anchor <= self.head {
            NormalizedTextRange {
                start: self.anchor,
                end: self.head,
            }
        } else {
            NormalizedTextRange {
                start: self.head,
                end: self.anchor,
            }
        }
    }

    pub fn spans_for_columns(self, cols: usize) -> Vec<SelectionSpan> {
        if self.is_empty() {
            return Vec::new();
        }

        let normalized = self.normalized();
        let start_col = normalized.start.col.min(cols);
        let end_col = normalized.end.col.min(cols);

        if normalized.start.row == normalized.end.row {
            return vec![SelectionSpan::new(normalized.start.row, start_col, end_col)];
        }

        let mut spans = Vec::with_capacity(normalized.end.row - normalized.start.row + 1);
        spans.push(SelectionSpan::new(normalized.start.row, start_col, cols));
        for row in (normalized.start.row + 1)..normalized.end.row {
            spans.push(SelectionSpan::new(row, 0, cols));
        }
        spans.push(SelectionSpan::new(normalized.end.row, 0, end_col));
        spans
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NormalizedTextRange {
    pub start: TextPoint,
    pub end: TextPoint,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SelectionSpan {
    pub row: usize,
    pub start_col: usize,
    pub end_col: usize,
}

impl SelectionSpan {
    pub fn new(row: usize, start_col: usize, end_col: usize) -> Self {
        Self {
            row,
            start_col,
            end_col,
        }
    }

    pub fn is_empty(self) -> bool {
        self.start_col >= self.end_col
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextSelection {
    pub node_id: NodeId,
    pub anchor: TextPoint,
    pub head: TextPoint,
    pub dragging: bool,
}

impl TextSelection {
    pub fn new(node_id: NodeId, anchor: TextPoint) -> Self {
        Self {
            node_id,
            anchor,
            head: anchor,
            dragging: true,
        }
    }

    pub fn range(self) -> TextRange {
        TextRange::new(self.anchor, self.head)
    }

    #[cfg(test)]
    pub fn with_head(mut self, head: TextPoint) -> Self {
        self.head = head;
        self
    }

    pub fn finish(mut self) -> Self {
        self.dragging = false;
        self
    }

    pub fn is_empty(self) -> bool {
        self.range().is_empty()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/text_selection_tests.rs"]
mod tests;
