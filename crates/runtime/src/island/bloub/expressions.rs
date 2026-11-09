//! Resting faces, ported from Bloub (MIT): head gaze, eye spacing and per-eye proportions.
#[derive(Clone, Copy)]
pub struct Eye {
    pub w: f32,
    pub h: f32,
    /// Degrees, positive = the top of the capsule leans right.
    pub tilt: f32,
    /// 1 = open; below 1 the lid is half down (same mechanism as a blink).
    pub open: f32,
}

#[derive(Clone, Copy)]
pub struct Expression {
    pub yaw: f32,
    pub pitch: f32,
    pub roll: f32,
    pub split: f32,
    pub eyes: [Eye; 2],
}

pub const EYE_SPLIT: f32 = 15.46;
const EYE_W: f32 = 0.186;
const EYE_H: f32 = 0.412;

const fn eye(w: f32, h: f32, tilt: f32, open: f32) -> Eye {
    Eye { w, h, tilt, open }
}

/// Mirrored tilt on the two eyes.
const fn pair(w: f32, h: f32, tilt: f32, open: f32) -> [Eye; 2] {
    [eye(w, h, tilt, open), eye(w, h, -tilt, open)]
}

const fn face(yaw: f32, pitch: f32, roll: f32, split: f32, eyes: [Eye; 2]) -> Expression {
    Expression {
        yaw,
        pitch,
        roll,
        split,
        eyes,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpressionId {
    Neutral,
    Attentive,
    Happy,
    Sad,
    Curious,
    Proud,
    Shy,
    Bored,
}

/// Faces a seed may pick as an agent's resting face; the negative ones are reserved for moods.
pub const RESTING: [ExpressionId; 7] = [
    ExpressionId::Neutral,
    ExpressionId::Attentive,
    ExpressionId::Happy,
    ExpressionId::Curious,
    ExpressionId::Proud,
    ExpressionId::Shy,
    ExpressionId::Bored,
];

pub fn expression(id: ExpressionId) -> Expression {
    match id {
        ExpressionId::Neutral => face(28.49, 28.62, -13.0, EYE_SPLIT, pair(EYE_W, EYE_H, 0.0, 1.0)),
        ExpressionId::Attentive => face(4.0, 5.0, -4.0, 16.0, pair(0.21, 0.44, 0.0, 1.0)),
        ExpressionId::Happy => face(5.0, 9.0, 0.0, 17.0, pair(0.27, 0.17, 14.0, 1.0)),
        ExpressionId::Sad => face(3.0, -13.0, 0.0, 16.0, pair(0.22, 0.4, -28.0, 1.0)),
        ExpressionId::Curious => face(
            16.0,
            -9.0,
            -15.0,
            16.5,
            [eye(0.24, 0.46, -8.0, 1.0), eye(0.2, 0.38, -8.0, 1.0)],
        ),
        ExpressionId::Proud => face(5.0, 17.0, 0.0, 17.0, pair(0.3, 0.15, 18.0, 1.0)),
        ExpressionId::Shy => face(-19.0, -14.0, -7.0, 14.0, pair(0.17, 0.3, 0.0, 1.0)),
        ExpressionId::Bored => face(-22.0, 2.0, 0.0, 16.0, pair(0.3, 0.12, 0.0, 1.0)),
    }
}
