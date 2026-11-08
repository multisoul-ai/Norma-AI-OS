//! 卡片初始化动画的紧凑帧资源。
//!
//! 每个资源用一个 byte 表示一个 100×41 网格单元：低 7 bit 是 ASCII 字符（0x7f
//! 代表中点 `·`），最高位表示强调色。这样运行时无需解析 Ghostty 的 HTML span，
//! 两种动画也可共享同一个自绘 renderer。

use crate::motion::InitializationAnimationKind;

mod paint;

pub(crate) use paint::render_intro_overlay;

pub(crate) const INTRO_ANIMATION_FRAME_COUNT: usize = 32;
pub(crate) const INTRO_ANIMATION_ROWS: usize = 41;
pub(crate) const INTRO_ANIMATION_COLUMNS: usize = 100;
const INTRO_ANIMATION_FRAME_BYTES: usize = INTRO_ANIMATION_ROWS * INTRO_ANIMATION_COLUMNS;
const INTRO_ANIMATION_TOTAL_BYTES: usize =
    INTRO_ANIMATION_FRAME_COUNT * INTRO_ANIMATION_FRAME_BYTES;
const ACCENT_BIT: u8 = 0x80;
const MIDDLE_DOT: u8 = 0x7f;

const TERMINAL_FRAMES: &[u8; INTRO_ANIMATION_TOTAL_BYTES] =
    include_bytes!("../../../assets/init-animation/ghostty-terminal-16-47.bin");
const CODEX_FRAMES: &[u8; INTRO_ANIMATION_TOTAL_BYTES] =
    include_bytes!("../../../assets/init-animation/codex-brand-ascii.bin");
const CLAUDE_FRAMES: &[u8; INTRO_ANIMATION_TOTAL_BYTES] =
    include_bytes!("../../../assets/init-animation/claude-code-brand-ascii.bin");
const CURSOR_FRAMES: &[u8; INTRO_ANIMATION_TOTAL_BYTES] =
    include_bytes!("../../../assets/init-animation/cursor-brand-ascii.bin");
const GROK_FRAMES: &[u8; INTRO_ANIMATION_TOTAL_BYTES] =
    include_bytes!("../../../assets/init-animation/grok-brand-ascii.bin");
const OPENCODE_FRAMES: &[u8; INTRO_ANIMATION_TOTAL_BYTES] =
    include_bytes!("../../../assets/init-animation/opencode-brand-ascii.bin");
const INFCODE_FRAMES: &[u8; INTRO_ANIMATION_TOTAL_BYTES] =
    include_bytes!("../../../assets/init-animation/infcode-brand-ascii.bin");
const KIMI_FRAMES: &[u8; INTRO_ANIMATION_TOTAL_BYTES] =
    include_bytes!("../../../assets/init-animation/kimi-brand-ascii.bin");
const ZCODE_FRAMES: &[u8; INTRO_ANIMATION_TOTAL_BYTES] =
    include_bytes!("../../../assets/init-animation/zcode-brand-ascii.bin");
const PI_FRAMES: &[u8; INTRO_ANIMATION_TOTAL_BYTES] =
    include_bytes!("../../../assets/init-animation/pi-brand-ascii.bin");
const HERMES_FRAMES: &[u8; INTRO_ANIMATION_TOTAL_BYTES] =
    include_bytes!("../../../assets/init-animation/hermes-brand-ascii.bin");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct IntroCell {
    pub(crate) glyph: char,
    pub(crate) accent: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct IntroFrame {
    cells: &'static [u8],
}

impl IntroFrame {
    pub(crate) fn cell(self, row: usize, column: usize) -> IntroCell {
        let packed = self.cells[row * INTRO_ANIMATION_COLUMNS + column];
        IntroCell {
            glyph: decode_glyph(packed & !ACCENT_BIT),
            accent: packed & ACCENT_BIT != 0,
        }
    }
}

pub(crate) fn intro_frame(kind: InitializationAnimationKind, frame_index: usize) -> IntroFrame {
    let frames = match kind {
        InitializationAnimationKind::Terminal => TERMINAL_FRAMES,
        InitializationAnimationKind::Codex => CODEX_FRAMES,
        InitializationAnimationKind::Claude => CLAUDE_FRAMES,
        InitializationAnimationKind::Cursor => CURSOR_FRAMES,
        InitializationAnimationKind::Grok => GROK_FRAMES,
        InitializationAnimationKind::OpenCode => OPENCODE_FRAMES,
        InitializationAnimationKind::InfCode => INFCODE_FRAMES,
        InitializationAnimationKind::Kimi => KIMI_FRAMES,
        InitializationAnimationKind::ZCode => ZCODE_FRAMES,
        InitializationAnimationKind::Pi => PI_FRAMES,
        InitializationAnimationKind::Hermes => HERMES_FRAMES,
    };
    let index = frame_index.min(INTRO_ANIMATION_FRAME_COUNT - 1);
    let start = index * INTRO_ANIMATION_FRAME_BYTES;
    IntroFrame {
        cells: &frames[start..start + INTRO_ANIMATION_FRAME_BYTES],
    }
}

pub(crate) fn intro_asset_name(kind: InitializationAnimationKind) -> &'static str {
    match kind {
        InitializationAnimationKind::Terminal => "ghostty-terminal-16-47",
        InitializationAnimationKind::Codex => "codex-brand-ascii",
        InitializationAnimationKind::Claude => "claude-code-brand-ascii",
        InitializationAnimationKind::Cursor => "cursor-brand-ascii",
        InitializationAnimationKind::Grok => "grok-brand-ascii",
        InitializationAnimationKind::OpenCode => "opencode-brand-ascii",
        InitializationAnimationKind::InfCode => "infcode-brand-ascii",
        InitializationAnimationKind::Kimi => "kimi-brand-ascii",
        InitializationAnimationKind::ZCode => "zcode-brand-ascii",
        InitializationAnimationKind::Pi => "pi-brand-ascii",
        InitializationAnimationKind::Hermes => "hermes-brand-ascii",
    }
}

pub(crate) fn intro_visible_cell_counts(
    kind: InitializationAnimationKind,
    frame_index: usize,
) -> (usize, usize) {
    let frame = intro_frame(kind, frame_index);
    let mut neutral = 0;
    let mut accent = 0;
    for row in 0..INTRO_ANIMATION_ROWS {
        for column in 0..INTRO_ANIMATION_COLUMNS {
            let cell = frame.cell(row, column);
            if cell.glyph == ' ' {
                continue;
            }
            if cell.accent {
                accent += 1;
            } else {
                neutral += 1;
            }
        }
    }
    (neutral, accent)
}

fn decode_glyph(code: u8) -> char {
    if code == MIDDLE_DOT {
        '·'
    } else {
        char::from(code)
    }
}
