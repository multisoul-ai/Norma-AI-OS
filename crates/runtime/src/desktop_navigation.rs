//! 桌面空间／项目的稳定循环导航，不依赖最近访问顺序。

pub fn adjacent_target<'a>(
    items: impl IntoIterator<Item = (i64, &'a str)>,
    current: Option<&str>,
    previous: bool,
) -> Option<&'a str> {
    let mut items: Vec<_> = items.into_iter().collect();
    items.sort_unstable();
    let last = items.len().checked_sub(1)?;
    let index = match items.iter().position(|(_, id)| Some(*id) == current) {
        Some(0) if previous => last,
        Some(index) if previous => index - 1,
        Some(index) => (index + 1) % items.len(),
        None if previous => last,
        None => 0,
    };
    Some(items[index].1)
}

#[cfg(test)]
#[path = "../tests/unit/desktop_navigation_tests.rs"]
mod tests;
