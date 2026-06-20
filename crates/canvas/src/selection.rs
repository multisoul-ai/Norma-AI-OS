//! 选择集（零 gpui）。本切片用 BTreeSet 保证遍历顺序稳定、便于测试。

use crate::node::NodeId;
use std::collections::BTreeSet;

#[derive(Debug, Default, Clone)]
pub struct Selection {
    ids: BTreeSet<NodeId>,
}

impl Selection {
    pub fn new() -> Self {
        Selection::default()
    }

    /// 单击选中语义：清空后只选当前。
    pub fn select_only(&mut self, id: NodeId) {
        self.ids.clear();
        self.ids.insert(id);
    }

    pub fn add(&mut self, id: NodeId) {
        self.ids.insert(id);
    }

    pub fn toggle(&mut self, id: NodeId) {
        if !self.ids.remove(&id) {
            self.ids.insert(id);
        }
    }

    pub fn deselect(&mut self, id: NodeId) {
        self.ids.remove(&id);
    }

    pub fn clear(&mut self) {
        self.ids.clear();
    }

    pub fn contains(&self, id: NodeId) -> bool {
        self.ids.contains(&id)
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.ids.iter().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 单击选中语义：select_only 清空旧选择并只保留当前 id。
    ///
    /// 执行过程：
    ///   1. select_only(10) → {10}
    ///   2. select_only(20) → {20}（10 被清掉）
    ///
    /// 预期结果：
    ///   - 正断言：选中 20（contains(20)==true）
    ///   - 负断言：不再选中 10（contains(10)==false）—— 证明是"替换"而非"累加"
    ///   - 正断言：len()==1
    #[test]
    fn test_select_only_replaces_previous() {
        let mut sel = Selection::new();
        sel.select_only(10);
        sel.select_only(20);

        assert!(sel.contains(20), "select_only(20) 后应选中 20");
        assert!(!sel.contains(10), "select_only 应清空旧选择，10 不应仍被选中");
        assert_eq!(sel.len(), 1, "单选后选择集大小应为 1");
    }

    /// add 累加多选；toggle 在选中/未选中间翻转。
    ///
    /// 执行过程：
    ///   1. add(1), add(2)        → {1,2}
    ///   2. toggle(2)             → {1}（2 原本在，翻转移除）
    ///   3. toggle(3)             → {1,3}（3 原本不在，翻转加入）
    ///
    /// 预期结果：
    ///   - 正断言：contains(1)==true（自始至终保留）
    ///   - 负断言：contains(2)==false（被 toggle 移除）
    ///   - 正断言：contains(3)==true（被 toggle 加入）
    #[test]
    fn test_add_and_toggle() {
        let mut sel = Selection::new();
        sel.add(1);
        sel.add(2);
        sel.toggle(2);
        sel.toggle(3);

        assert!(sel.contains(1), "1 应始终被选中");
        assert!(!sel.contains(2), "2 被 toggle 后应移除");
        assert!(sel.contains(3), "3 被 toggle 后应加入");
    }

    /// deselect 移除指定 id；clear 清空全部。
    ///
    /// 预期结果：
    ///   - 正断言：deselect 后该 id 不在、其余仍在
    ///   - 正断言：clear 后 is_empty()==true
    ///   - 负断言：clear 后 contains(任意)==false
    #[test]
    fn test_deselect_and_clear() {
        let mut sel = Selection::new();
        sel.add(1);
        sel.add(2);

        sel.deselect(1);
        assert!(!sel.contains(1), "deselect(1) 后 1 不应被选中");
        assert!(sel.contains(2), "deselect(1) 不应影响 2");

        sel.clear();
        assert!(sel.is_empty(), "clear 后选择集应为空");
        assert!(!sel.contains(2), "clear 后 2 也不应被选中");
    }
}
