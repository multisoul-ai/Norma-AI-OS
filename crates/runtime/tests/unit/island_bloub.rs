use super::*;

const ALL_MOODS: [Mood; 5] = [
    Mood::Idle,
    Mood::Working,
    Mood::Waiting,
    Mood::Failed,
    Mood::Responded,
];

#[test]
fn sample_is_a_pure_function_of_time() {
    let p = profile("claude-1");
    assert_eq!(
        sample(&p, Mood::Working, 12.3),
        sample(&p, Mood::Working, 12.3)
    );
}

#[test]
fn profile_is_stable_per_seed_and_varies_across_seeds() {
    assert_eq!(profile("alice"), profile("alice"));
    let shapes: std::collections::HashSet<_> = (0..40)
        .map(|i| format!("{:?}", profile(&format!("agent-{i}")).silhouette))
        .collect();
    assert!(
        shapes.len() >= 3,
        "seeds should spread over silhouettes: {shapes:?}"
    );
}

#[test]
fn every_frame_is_finite_and_inside_the_unit_disc_budget() {
    for seed in ["a", "b", "c", "d", "e"] {
        let p = profile(seed);
        for mood in ALL_MOODS {
            for step in 0..200 {
                let f = sample(&p, mood, step as f32 * 0.37);
                for pt in f.body.iter().chain(f.eyes.iter().flatten().flatten()) {
                    assert!(pt.x.is_finite() && pt.y.is_finite());
                    assert!(
                        pt.x.abs() < 1.5 && pt.y.abs() < 1.5,
                        "{seed} {mood:?} {pt:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn blink_squashes_the_eye_then_reopens() {
    let height = |t: f32| {
        let f = sample(&profile("x"), Mood::Waiting, t);
        let eye = f.eyes.iter().flatten().next().expect("a visible eye");
        let (lo, hi) = eye.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| {
            (lo.min(p.y), hi.max(p.y))
        });
        hi - lo
    };
    let phase = profile("x").phase;
    let start = tables().blinks[0] - phase;
    let closed = height(start + BLINK_DUR * 0.45);
    assert!(
        closed < height(start - 0.5) * 0.3,
        "eye should be squashed mid-blink"
    );
    assert!((height(start + 1.0) - height(start - 0.5)).abs() < 0.05);
}

#[test]
fn only_working_is_busy() {
    assert!(ALL_MOODS
        .iter()
        .filter(|m| m.is_busy())
        .eq([&Mood::Working]));
}
