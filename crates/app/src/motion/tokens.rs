use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionTokens {
    pub hover_in: Duration,
    pub hover_out: Duration,
    pub press: Duration,
    pub focus_selected: Duration,
    pub layout: Duration,
    pub reduced_fade: Duration,
    pub breathing_fade: Duration,
    pub completion_settle: Duration,
    pub chase_fade: Duration,
}

impl Default for MotionTokens {
    fn default() -> Self {
        Self {
            hover_in: Duration::from_millis(120),
            hover_out: Duration::from_millis(180),
            press: Duration::from_millis(100),
            focus_selected: Duration::from_millis(180),
            layout: Duration::from_millis(420),
            reduced_fade: Duration::from_millis(160),
            breathing_fade: Duration::from_millis(150),
            completion_settle: Duration::from_millis(520),
            chase_fade: Duration::from_millis(150),
        }
    }
}
