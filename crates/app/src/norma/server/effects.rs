use norma::protocol::CardCommandResult;

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct NormaCommandEffects {
    pub(crate) canvas_dirty: bool,
    pub(crate) session_dirty: bool,
    pub(crate) notify: bool,
}

impl NormaCommandEffects {
    pub(super) const NONE: Self = Self {
        canvas_dirty: false,
        session_dirty: false,
        notify: false,
    };

    pub(super) const CANVAS_SESSION: Self = Self {
        canvas_dirty: true,
        session_dirty: true,
        notify: true,
    };

    pub(super) const CANVAS: Self = Self {
        canvas_dirty: true,
        session_dirty: false,
        notify: true,
    };

    pub(super) const NOTIFY: Self = Self {
        canvas_dirty: false,
        session_dirty: false,
        notify: true,
    };
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct NormaExecution {
    pub(crate) result: CardCommandResult,
    pub(crate) effects: NormaCommandEffects,
    pub(crate) removed_agent_definition_id: Option<i64>,
}
