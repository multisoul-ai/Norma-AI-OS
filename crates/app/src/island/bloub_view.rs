//! Island avatar: the Bloub engine from `runtime::island::bloub` painted as GPUI paths. Every
//! mood animates; a window schedules one coalesced redraw per tick instead of one per avatar, at
//! 30 fps for busy moods and 15 fps for the rest, never at the display rate.
use gpui::{
    canvas, point, prelude::*, px, rgb, App, Bounds, Div, PathBuilder, Pixels, Point, Window,
    WindowId,
};
use runtime::island::bloub::{profile, sample, Frame, Mood, Pt, EYE_POINTS};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const BUSY_TICK: Duration = Duration::from_millis(33);
const CALM_TICK: Duration = Duration::from_millis(66);

/// Circle diameter is 0.80 of the box. The triangle's 1.153 scale then
/// reaches about half a point past a 20pt slot, which keeps the four areas level.
const BODY_SHARE: f32 = 0.40;
const EYE_COLOR: u32 = 0x0b0d10;

fn clock() -> f32 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f32()
}

/// Windows with a redraw already scheduled, and the fastest tick they asked for.
fn pending() -> &'static Mutex<HashMap<WindowId, Duration>> {
    static PENDING: OnceLock<Mutex<HashMap<WindowId, Duration>>> = OnceLock::new();
    PENDING.get_or_init(Mutex::default)
}

fn schedule_redraw(window: &mut Window, cx: &mut App, tick: Duration) {
    let id = window.window_handle().window_id();
    let Ok(mut queue) = pending().lock() else {
        return;
    };
    if let Some(queued) = queue.get_mut(&id) {
        *queued = (*queued).min(tick);
        return;
    }
    queue.insert(id, tick);
    drop(queue);
    window
        .spawn(cx, async move |cx| {
            // Read the tick late so a busy avatar painted meanwhile can speed this one up.
            let wait = pending()
                .lock()
                .ok()
                .and_then(|p| p.get(&id).copied())
                .unwrap_or(tick);
            cx.background_executor().timer(wait).await;
            if let Ok(mut p) = pending().lock() {
                p.remove(&id);
            }
            let _ = cx.update(|window, _| window.refresh());
        })
        .detach();
}

pub(super) fn bloub(
    seed: &str,
    color: u32,
    size: f32,
    mood: Mood,
    opacity: f32,
    animate: bool,
) -> Div {
    let profile = profile(seed);
    let still = -profile.phase;
    let tick = if mood.is_busy() { BUSY_TICK } else { CALM_TICK };
    gpui::div()
        .size(px(size))
        .flex_none()
        .relative()
        .opacity(opacity)
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, cx| {
                    let time = if animate { clock() } else { still };
                    paint(&sample(&profile, mood, time), bounds, color, window);
                    if animate {
                        schedule_redraw(window, cx, tick);
                    }
                },
            )
            .absolute()
            .size_full(),
        )
}

fn paint(frame: &Frame, bounds: Bounds<Pixels>, color: u32, window: &mut Window) {
    let radius = bounds.size.width.min(bounds.size.height) * BODY_SHARE;
    let center = bounds.center();
    let at = |p: &Pt| -> Point<Pixels> { point(center.x + radius * p.x, center.y + radius * p.y) };

    let mut body = PathBuilder::fill();
    let n = frame.body.len();
    body.move_to(at(&frame.body[0]));
    for i in 0..n {
        let (p0, p1, p2, p3) = (
            &frame.body[(i + n - 1) % n],
            &frame.body[i],
            &frame.body[(i + 1) % n],
            &frame.body[(i + 2) % n],
        );
        // Catmull-Rom to cubic, tension 1/6.
        let c1 = Pt {
            x: p1.x + (p2.x - p0.x) / 6.,
            y: p1.y + (p2.y - p0.y) / 6.,
        };
        let c2 = Pt {
            x: p2.x - (p3.x - p1.x) / 6.,
            y: p2.y - (p3.y - p1.y) / 6.,
        };
        body.cubic_bezier_to(at(p2), at(&c1), at(&c2));
    }
    body.close();
    if let Ok(path) = body.build() {
        window.paint_path(path, rgb(color));
    }

    for eye in frame.eyes.iter().flatten() {
        if frame.smile {
            paint_smile(eye, &at, radius, window);
            continue;
        }
        let points: Vec<Point<Pixels>> = eye.iter().map(at).collect();
        let mut builder = PathBuilder::fill();
        builder.add_polygon(&points, true);
        if let Ok(path) = builder.build() {
            window.paint_path(path, rgb(EYE_COLOR));
        }
    }
}

/// Prototype `eh`: an upward arc (a closed, smiling eye) as wide as the capsule it replaces.
fn paint_smile(
    eye: &[Pt; EYE_POINTS],
    at: &impl Fn(&Pt) -> Point<Pixels>,
    radius: Pixels,
    window: &mut Window,
) {
    let n = eye.len() as f32;
    let (cx, cy) = (
        eye.iter().map(|p| p.x).sum::<f32>() / n,
        eye.iter().map(|p| p.y).sum::<f32>() / n,
    );
    let half = eye
        .iter()
        .map(|p| (p.x - cx).abs())
        .fold(0f32, f32::max)
        .max(0.12);
    let mut arc = PathBuilder::stroke((radius * 0.17).max(px(1.2)));
    arc.move_to(at(&Pt {
        x: cx - half,
        y: cy + half * 0.55,
    }));
    arc.curve_to(
        at(&Pt {
            x: cx + half,
            y: cy + half * 0.55,
        }),
        at(&Pt {
            x: cx,
            y: cy - half * 1.05,
        }),
    );
    if let Ok(path) = arc.build() {
        window.paint_path(path, rgb(EYE_COLOR));
    }
}
