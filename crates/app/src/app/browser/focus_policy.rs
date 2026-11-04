//! An asynchronous native view must not overwrite a newer user focus intent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FocusSnapshot {
    pub(crate) foreground: isize,
    pub(crate) focused: isize,
}

pub(crate) fn may_return_focus(
    expected: FocusSnapshot,
    current: FocusSnapshot,
    parent: isize,
    source_owns_focus: bool,
) -> bool {
    expected == current && current.foreground == parent && source_owns_focus
}
