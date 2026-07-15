//! Production artifacts emitted by one completed Agent dialogue turn.

use dialogue::ReportPlayDecision;
use norma::protocol::CardCommand;

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct TemplateReportItem {
    pub(crate) text: String,
    pub(crate) decision: ReportPlayDecision,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct VoiceTurnArtifacts {
    pub(crate) reply_text: String,
    pub(crate) executed_commands: Vec<CardCommand>,
    pub(crate) template_reports: Vec<TemplateReportItem>,
    pub(crate) queued_template_reports: Vec<String>,
}
