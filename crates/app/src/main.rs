//! multisoul-desktop 入口：委托库目标启动 GPUI App。

fn main() {
    // 卡片终端里 `cargo run -p app` 会继承来源 Agent 的身份变量。
    // 必须在日志、CEF 和 GPUI 起线程之前清掉。
    runtime::clear_inherited_agent_identity();
    app::run_norma_os();
}
