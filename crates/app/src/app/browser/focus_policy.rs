//! An asynchronous native view must not overwrite a newer user focus intent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FocusSnapshot {
    pub(crate) foreground: isize,
    pub(crate) focused: isize,
}

pub(crate) fn may_return_focus(
    _expected: FocusSnapshot,
    _current: FocusSnapshot,
    _parent: isize,
    _source_owns_focus: bool,
) -> bool {
    // Legacy focus_parent was unconditional; retain that behavior for the RED witness.
    true
}
