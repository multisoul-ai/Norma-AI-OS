use std::io::Read;

fn main() {
    assert_eq!(
        std::env::args().nth(1).as_deref(),
        Some("nodejs"),
        "生产调用必须保留官方helper的nodejs子命令"
    );
    let mut script = String::new();
    std::io::stdin().read_to_string(&mut script).unwrap();
    assert_eq!(
        script, "fixture script",
        "脚本必须经真实stdin完整传递并关闭"
    );
    println!(r#"{{"task_space_id":"99","task_space_name":"普通日志"}}"#);
    eprintln!(r#"NORMA_EGO_RESULT={{"task_space_id":"5","task_space_name":"Norma · Ego"}}"#);
}
