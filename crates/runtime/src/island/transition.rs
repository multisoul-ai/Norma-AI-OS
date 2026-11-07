//! Presentation-only geometry. No clocks, timers or platform handles are owned here.
use super::geometry::{IslandFrame, IslandLayout};

pub struct IslandTransition {
    from: IslandLayout,
    target: IslandLayout,
    envelope: IslandFrame,
    started: u64,
    duration: u64,
    fade_only: bool,
    opacity_from: f32,
    immediate: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct IslandPresentation {
    pub surface: IslandLayout,
    pub window: IslandFrame,
    pub opacity: f32,
    pub animating: bool,
}

impl IslandPresentation {
    /// Keep opaque content within the rounded surface while its measured width is changing.
    pub fn content_inset(&self) -> f64 {
        if self.animating {
            self.surface.radius.ceil().max(16.)
        } else {
            16.
        }
    }
}

impl IslandTransition {
    pub fn new(layout: IslandLayout) -> Self {
        Self {
            from: layout,
            target: layout,
            envelope: layout.frame,
            started: 0,
            duration: 0,
            fade_only: false,
            opacity_from: 1.,
            immediate: false,
        }
    }

    pub fn retarget(&mut self, target: IslandLayout, now: u64, reduce: bool, immediate: bool) {
        if target == self.target && !immediate {
            return;
        }
        let current = self.sample(now);
        // Screen changes are placement, not interaction: never fly across displays.
        let moved = top(self.target.frame) != top(target.frame)
            || center(self.target.frame) != center(target.frame);
        self.from = current.surface;
        self.immediate = immediate || moved;
        self.target = target;
        self.started = now;
        self.fade_only = reduce;
        self.opacity_from = if current.animating {
            current.opacity
        } else {
            0.75
        };
        self.duration = if immediate || moved || current.surface == target {
            0
        } else if reduce {
            100
        } else if target.frame.height >= current.surface.frame.height {
            180
        } else {
            120
        };
        self.envelope = if self.duration == 0 || reduce {
            target.frame
        } else {
            union(current.window, target.frame)
        };
    }

    pub fn retarget_content(&mut self, target: IslandLayout, now: u64, reduce: bool) {
        self.retarget(target, now, reduce, self.immediate);
    }

    pub fn sample(&self, now: u64) -> IslandPresentation {
        let fraction = if self.duration == 0 {
            1.
        } else {
            (now.saturating_sub(self.started) as f64 / self.duration as f64).min(1.)
        };
        let animating = fraction < 1.;
        let eased = 1. - (1. - fraction).powi(3);
        let surface = if !animating || self.fade_only {
            self.target
        } else {
            interpolate(self.from, self.target, eased)
        };
        IslandPresentation {
            surface,
            window: if animating {
                self.envelope
            } else {
                self.target.frame
            },
            opacity: if animating && self.fade_only {
                self.opacity_from + (1. - self.opacity_from) * eased as f32
            } else {
                1.
            },
            animating,
        }
    }
}

fn center(frame: IslandFrame) -> f64 {
    frame.x + frame.width / 2.
}
fn top(frame: IslandFrame) -> f64 {
    frame.y + frame.height
}

fn union(a: IslandFrame, b: IslandFrame) -> IslandFrame {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    IslandFrame {
        x,
        y,
        width: (a.x + a.width).max(b.x + b.width) - x,
        height: top(a).max(top(b)) - y,
    }
}

fn interpolate(a: IslandLayout, b: IslandLayout, t: f64) -> IslandLayout {
    let mix = |a: f64, b: f64| a + (b - a) * t;
    IslandLayout {
        frame: IslandFrame {
            x: mix(a.frame.x, b.frame.x),
            y: mix(a.frame.y, b.frame.y),
            width: mix(a.frame.width, b.frame.width),
            height: mix(a.frame.height, b.frame.height),
        },
        notch_width: b.notch_width,
        header_height: b.header_height,
        radius: mix(a.radius, b.radius),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/island_transition.rs"]
mod tests;
