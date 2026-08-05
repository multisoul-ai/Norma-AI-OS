fn main() {
    let task_space_name = std::env::var("NORMA_EGO_L3_LIVE_TASK_SPACE_NAME")
        .expect("set NORMA_EGO_L3_LIVE_TASK_SPACE_NAME to a dynamic Ego Task Space title");
    match app::run_ego_live_capture_probe(&task_space_name) {
        Ok(report) => {
            println!(
                "Ego live capture passed: task={:?} source={} frames={} fps={:.2} max_step={} sequence={}..{} size={}x{}",
                report.task_space_name,
                report.source,
                report.presented_frames,
                report.effective_fps,
                report.maximum_sequence_step,
                report.first_sequence,
                report.last_sequence,
                report.width,
                report.height,
            );
        }
        Err(message) => {
            eprintln!("Ego live capture failed: {message}");
            std::process::exit(1);
        }
    }
}
