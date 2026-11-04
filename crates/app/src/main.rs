#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
//! multisoul-desktop 入口：委托库目标启动 GPUI App。

fn main() {
    app::run_norma_os();
}
