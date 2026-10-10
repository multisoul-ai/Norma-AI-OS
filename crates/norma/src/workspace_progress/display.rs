/// Bounds only human display text, never stable references or evidence clocks.
pub(super) fn excerpt(value: &str, limit: usize) -> String {
    let clean: String = value
        .chars()
        .filter(|c| !c.is_control() || c.is_whitespace())
        .collect();
    let clean = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.chars().count() <= limit {
        return clean;
    }
    let mut bounded: String = clean.chars().take(limit.saturating_sub(1)).collect();
    bounded.push('…');
    bounded
}
