//! Inline detail identity and presentation rules for pending staff items.
use norma::protocol::QuestionRef;

pub(super) fn approval_receipt_confirmed(value: &serde_json::Value) -> bool {
    value["status"] == "submitted"
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DetailKey {
    Proposal((String, u64)),
    Question {
        area_id: String,
        target: Box<QuestionRef>,
    },
    Approval {
        area_id: String,
        node_id: u64,
        run_id: String,
        request_id: String,
    },
}

#[derive(Default)]
pub(crate) struct AttentionDetails {
    pub(crate) open: Option<DetailKey>,
    pub(crate) original_open: bool,
    pub(crate) approval_details: Option<(String, String)>,
    pub(crate) decided: Vec<(DetailKey, bool)>,
    pub(crate) pending: Vec<DetailKey>,
    workspace: Option<String>,
}

impl AttentionDetails {
    pub(crate) fn is_open(&self, key: &DetailKey) -> bool {
        self.open.as_ref() == Some(key)
    }

    pub(crate) fn toggle(&mut self, key: DetailKey) {
        self.open = if self.is_open(&key) { None } else { Some(key) };
        self.original_open = false;
        self.approval_details = None;
    }

    pub(crate) fn toggle_original(&mut self) {
        self.original_open = self.open.is_some() && !self.original_open;
    }

    pub(crate) fn reconcile(&mut self, workspace: Option<&str>, keys: &[DetailKey]) {
        if self.workspace.as_deref() != workspace {
            self.decided.clear();
            self.pending.clear();
        }
        self.decided.retain(|(key, _)| keys.contains(key));
        if self.workspace.as_deref() != workspace
            || self.open.as_ref().is_some_and(|key| !keys.contains(key))
        {
            self.open = None;
            self.original_open = false;
            self.approval_details = None;
        }
        self.workspace = workspace.map(str::to_owned);
    }
}

pub(super) fn compact_title<'a>(text: &'a str, fallback: &'a str) -> &'a str {
    if text.trim().is_empty() || text.chars().count() > 50 || text.contains(['\n', '\r']) {
        fallback
    } else {
        text.trim()
    }
}

pub(super) use brain::task_outline::parse as task_summary;
#[cfg(test)]
use brain::task_outline::task_points;

#[cfg(test)]
#[path = "../../../../tests/unit/overview_details_tests.rs"]
mod tests;
