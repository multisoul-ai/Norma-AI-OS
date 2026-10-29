//! 桌面空间／项目的稳定循环导航，不依赖最近访问顺序。

/// 目录按展示顺序浏览：左右循环，上下同列；末行缺格吸附末项，垂直边界保持。
pub fn directory_grid_target(
    count: usize,
    columns: usize,
    current: Option<usize>,
    direction: canvas::NavigationDirection,
) -> Option<usize> {
    use canvas::NavigationDirection::*;
    let last = count.checked_sub(1)?;
    let columns = columns.max(1);
    let Some(index) = current.filter(|index| *index < count) else {
        return Some(if matches!(direction, Left | Up) {
            last
        } else {
            0
        });
    };
    Some(match direction {
        Left => {
            if index == 0 {
                last
            } else {
                index - 1
            }
        }
        Right => (index + 1) % count,
        Up => {
            if index >= columns {
                index - columns
            } else {
                index
            }
        }
        Down => {
            if index / columns < last / columns {
                (index + columns).min(last)
            } else {
                index
            }
        }
    })
}

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
