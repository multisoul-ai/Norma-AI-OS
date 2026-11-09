//! Native, main-thread probe of the actual production DisplayLink module.
extern crate self as util;

use std::{
    ffi::c_void,
    io::{self, BufRead, Write},
    sync::atomic::{AtomicUsize, Ordering},
};

mod bindings;
pub mod dispatch_sys {
    pub use super::bindings::*;
    pub use super::bindings::{Object as dispatch_object_t, Source as dispatch_source_t};
}

pub fn dispatch_get_main_queue() -> bindings::Source {
    &raw const bindings::_dispatch_main_q as *mut c_void
}

pub trait ResultExt<T> {
    fn log_err(self) -> Option<T>;
}
impl<T, E: std::fmt::Debug> ResultExt<T> for Result<T, E> {
    fn log_err(self) -> Option<T> {
        self.map_err(|e| eprintln!("{e:?}")).ok()
    }
}

#[path = "../../src/platform/mac/display_link.rs"]
mod display_link;

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFRunLoopDefaultMode: *const c_void;
    fn CFRunLoopRunInMode(mode: *const c_void, seconds: f64, once: bool) -> i32;
}
static A: AtomicUsize = AtomicUsize::new(0);
static B: AtomicUsize = AtomicUsize::new(0);
unsafe extern "C" fn tick(data: *mut c_void) {
    unsafe { &*(data as *const AtomicUsize) }.fetch_add(1, Ordering::SeqCst);
}
fn pump() {
    // 0.2 seconds permits about 12 frames on a 60 Hz display.
    unsafe {
        CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.2, false);
    }
}
fn link(counter: &'static AtomicUsize) -> anyhow::Result<display_link::DisplayLink> {
    display_link::DisplayLink::new(
        unsafe { core_graphics::display::CGMainDisplayID() },
        counter as *const _ as *mut c_void,
        tick,
    )
}

/// 同屏两个订阅独立启停，并在关闭后不再接收帧。
/// 数据构造：A、B 共两个窗口；采样 0.2 秒约 12 帧。
/// 执行过程：同时启动 → 暂停 A → B 继续 → 恢复 A → 直接销毁运行中的 A、B。
/// 预期结果：A 暂停后不增长、恢复后增长；关闭 A 不影响 B；销毁后均不再增长。
fn exercise() -> anyhow::Result<()> {
    let mut a = link(&A)?;
    let mut b = link(&B)?;
    let initial_a = A.load(Ordering::SeqCst);
    let initial_b = B.load(Ordering::SeqCst);
    assert_eq!(
        a.display_id(),
        b.display_id(),
        "both windows must target the same display"
    );
    a.start()?;
    a.start()?; // Repeated start must be idempotent.
    b.start()?;
    pump();
    assert!(
        A.load(Ordering::SeqCst) > initial_a,
        "active A must receive real CoreVideo frames"
    );
    assert!(
        B.load(Ordering::SeqCst) > initial_b,
        "active B must receive real CoreVideo frames"
    );
    assert!(
        a.pause()?,
        "a running window must receive a paint-resume token"
    );
    assert!(
        !a.pause()?,
        "an already hidden/paused window must not receive a resume token"
    );
    let stopped_a = A.load(Ordering::SeqCst);
    let active_b = B.load(Ordering::SeqCst);
    pump();
    assert_eq!(
        A.load(Ordering::SeqCst),
        stopped_a,
        "stopped A must not receive pending frames"
    );
    assert!(
        B.load(Ordering::SeqCst) > active_b,
        "stopping A must not stop B on the same display"
    );
    a.start()?;
    pump();
    assert!(
        A.load(Ordering::SeqCst) > stopped_a,
        "the same window must resume receiving frames"
    );
    a.stop()?;
    a.stop()?;
    a.start()?;
    let stopped_a = A.load(Ordering::SeqCst);
    let active_b = B.load(Ordering::SeqCst);
    // Real window teardown drops a running link without an explicit stop.
    drop(a);
    pump();
    assert_eq!(
        A.load(Ordering::SeqCst),
        stopped_a,
        "closed A must not receive late callbacks"
    );
    assert!(
        B.load(Ordering::SeqCst) > active_b,
        "dropping active A must not stop B on the shared display"
    );
    let stopped_b = B.load(Ordering::SeqCst);
    drop(b);
    pump();
    assert_eq!(
        A.load(Ordering::SeqCst),
        stopped_a,
        "closed A must remain unsubscribed when the last window closes"
    );
    assert_eq!(
        B.load(Ordering::SeqCst),
        stopped_b,
        "closed B must not receive late callbacks"
    );
    Ok(())
}

fn main() -> anyhow::Result<()> {
    exercise()?;
    println!("READY");
    io::stdout().flush()?;
    for line in io::stdin().lock().lines() {
        match line?.as_str() {
            "churn" => {
                // 32 cycles × 2 lifetimes (never started / started) = 64 closes.
                // Old per-window forget therefore adds 64 native links.
                for _ in 0..32 {
                    drop(link(&A)?);
                    let mut source = link(&B)?;
                    source.start()?;
                    source.stop()?;
                    drop(source);
                }
                exercise()?;
                println!("READY");
                io::stdout().flush()?;
            }
            "quit" => break,
            _ => anyhow::bail!("unknown probe command"),
        }
    }
    Ok(())
}
