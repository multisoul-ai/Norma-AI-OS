#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloneStage {
    Connecting,
    Enumerating,
    Counting,
    Compressing,
    Receiving,
    Resolving,
    CheckingOut,
}

impl CloneStage {
    pub fn label(self) -> &'static str {
        match self {
            Self::Connecting => "Connecting to GitHub…",
            Self::Enumerating => "Enumerating objects…",
            Self::Counting => "Counting objects…",
            Self::Compressing => "Compressing objects…",
            Self::Receiving => "Receiving objects…",
            Self::Resolving => "Resolving deltas…",
            Self::CheckingOut => "Checking out files…",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CloneProgress {
    pub stage: CloneStage,
    pub percent: Option<u8>,
}

pub fn parse_git_progress_line(line: &str) -> Option<CloneProgress> {
    let line = line.trim().strip_prefix("remote: ").unwrap_or(line.trim());
    let (stage, detail) = if let Some(detail) = line.strip_prefix("Cloning into ") {
        (CloneStage::Connecting, detail)
    } else if let Some(detail) = line.strip_prefix("Enumerating objects:") {
        (CloneStage::Enumerating, detail)
    } else if let Some(detail) = line.strip_prefix("Counting objects:") {
        (CloneStage::Counting, detail)
    } else if let Some(detail) = line.strip_prefix("Compressing objects:") {
        (CloneStage::Compressing, detail)
    } else if let Some(detail) = line.strip_prefix("Receiving objects:") {
        (CloneStage::Receiving, detail)
    } else if let Some(detail) = line.strip_prefix("Resolving deltas:") {
        (CloneStage::Resolving, detail)
    } else if let Some(detail) = line.strip_prefix("Updating files:") {
        (CloneStage::CheckingOut, detail)
    } else {
        return None;
    };

    Some(CloneProgress {
        stage,
        percent: parse_percent(detail),
    })
}

fn parse_percent(detail: &str) -> Option<u8> {
    let before_percent = detail.split_once('%')?.0;
    let token = before_percent.split_whitespace().last()?;
    let percent = token.parse::<u8>().ok()?;
    (percent <= 100).then_some(percent)
}
