//! 侧栏任务行右侧的时间、头像底色：纯函数，便于不开窗口测试。
use crate::app::brain_host::{civil_month_day, local_offset_secs};

const DAY: i64 = 86_400;

/// 今天更新的显示「时:分」，更早的显示「月/日」。`offset_secs` 是本机时区相对 UTC 的偏移。
pub(super) fn task_time_label(updated_secs: u64, now_secs: u64, offset_secs: i64) -> String {
    let updated = updated_secs as i64 + offset_secs;
    let now = now_secs as i64 + offset_secs;
    if updated.div_euclid(DAY) == now.div_euclid(DAY) {
        let seconds = updated.rem_euclid(DAY);
        format!("{}:{:02}", seconds / 3600, seconds % 3600 / 60)
    } else {
        let (month, day) = civil_month_day(updated.div_euclid(DAY));
        format!("{month}/{day}")
    }
}

/// 以当前系统时区格式化。
pub(super) fn task_time_now(updated_secs: u64) -> String {
    let now = norma::protocol::timestamp();
    task_time_label(updated_secs, now, local_offset_secs(now as i64 * 1000))
}

/// 头像底色：同一个 Agent 名字永远是同一种颜色。
const AVATAR_COLORS: [u32; 6] = [
    0xc9b8ffff, 0xff9d6bff, 0x6fe0c5ff, 0x7db4ffff, 0xffd36bff, 0xf59bd0ff,
];

pub(super) fn avatar_color(name: &str) -> u32 {
    let hash = name.bytes().fold(0u32, |acc, byte| {
        acc.wrapping_mul(31).wrapping_add(byte as u32)
    });
    AVATAR_COLORS[hash as usize % AVATAR_COLORS.len()]
}

/// 头像里的首字：Agent 名的第一个字符（大写），没有名字时用问号。
pub(super) fn avatar_initial(name: &str) -> String {
    name.chars()
        .next()
        .map_or_else(|| "?".to_owned(), |c| c.to_uppercase().collect())
}
