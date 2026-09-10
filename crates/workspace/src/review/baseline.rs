use super::{git_text, ReviewError, Root};
use std::time::Instant;

/// Tree comparison includes changes already committed during the task. Paths stay relative to cwd.
pub(super) fn committed_paths(
    root: &Root,
    before: Option<&str>,
    after: Option<&str>,
    deadline: Instant,
) -> Result<Vec<String>, ReviewError> {
    if before == after {
        return Ok(vec![]);
    }
    let raw = match (before, after) {
        (Some(before), Some(after)) => git_text(
            root,
            &[
                "diff",
                "--relative",
                "--name-only",
                "-z",
                "--no-ext-diff",
                "--no-textconv",
                "--no-renames",
                before,
                after,
                "--",
                ".",
            ],
            deadline,
            512 * 1024,
        )?,
        (Some(hash), None) | (None, Some(hash)) => git_text(
            root,
            &["ls-tree", "-r", "--name-only", "-z", hash, "--", "."],
            deadline,
            512 * 1024,
        )?,
        (None, None) => return Ok(vec![]),
    };
    raw.split('\0')
        .filter(|s| !s.is_empty() && !super::scoped::is_internal(s))
        .map(|path| {
            super::scoped::validate(path)?;
            Ok(path.to_owned())
        })
        .collect()
}
