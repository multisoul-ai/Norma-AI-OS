use super::*;

pub(super) fn log_workspace_open_timing(
    stage: &'static str,
    root: &Path,
    stage_start: &mut Instant,
    total_start: Instant,
) {
    tracing::info!(
        stage,
        path = %root.display(),
        stage_ms = stage_start.elapsed().as_secs_f64() * 1000.0,
        total_ms = total_start.elapsed().as_secs_f64() * 1000.0,
        "workspace_open_timing"
    );
    *stage_start = Instant::now();
}
