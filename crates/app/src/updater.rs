//! App-global Sparkle 2 owner and the native “Check for Updates…” action seam.

use gpui::{App, Global};

pub(crate) fn install(cx: &mut App) {
    if cx.has_global::<SparkleUpdater>() {
        return;
    }

    let updater = SparkleUpdater::load();
    match &updater.backend {
        UpdaterBackend::Unavailable(reason) => {
            tracing::warn!(%reason, "Sparkle updater unavailable; update checks are disabled")
        }
        #[cfg(target_os = "macos")]
        UpdaterBackend::Ready(_) => tracing::info!("Sparkle updater initialized"),
    }
    cx.set_global(updater);
}

pub(crate) fn check_for_updates(cx: &mut App) {
    let Some(updater) = cx.try_global::<SparkleUpdater>() else {
        tracing::warn!("Sparkle updater is not installed; ignoring Check for Updates action");
        return;
    };

    if let Err(reason) = updater.check_for_updates() {
        tracing::warn!(%reason, "Check for Updates action is unavailable");
    }
}

struct SparkleUpdater {
    backend: UpdaterBackend,
}

impl Global for SparkleUpdater {}

impl SparkleUpdater {
    fn load() -> Self {
        Self {
            backend: UpdaterBackend::load(),
        }
    }

    fn check_for_updates(&self) -> Result<(), &str> {
        match &self.backend {
            UpdaterBackend::Unavailable(reason) => Err(reason),
            #[cfg(target_os = "macos")]
            UpdaterBackend::Ready(updater) => {
                updater.check_for_updates();
                Ok(())
            }
        }
    }
}

enum UpdaterBackend {
    Unavailable(&'static str),
    #[cfg(target_os = "macos")]
    Ready(macos::MacSparkleUpdater),
}

impl UpdaterBackend {
    fn load() -> Self {
        #[cfg(target_os = "macos")]
        {
            match macos::MacSparkleUpdater::load() {
                Ok(updater) => Self::Ready(updater),
                Err(reason) => Self::Unavailable(reason),
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            Self::Unavailable("Sparkle is only available on macOS")
        }
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject};
    use std::ffi::CString;
    use std::path::PathBuf;

    pub(super) struct MacSparkleUpdater {
        controller: Retained<AnyObject>,
        // Objective-C keeps class metadata and Sparkle may keep shutdown-time
        // objects alive. Retain the dlopen reference until process termination;
        // dynamically unloading this framework would make those pointers stale.
        _framework_handle: *mut libc::c_void,
    }

    impl MacSparkleUpdater {
        pub(super) fn load() -> Result<Self, &'static str> {
            if unsafe { libc::pthread_main_np() } != 1 {
                return Err("Sparkle must be initialized on the main thread");
            }

            let framework_path = framework_path()?;
            let path = CString::new(framework_path.to_string_lossy().as_bytes())
                .map_err(|_| "Sparkle framework path contains a NUL byte")?;
            let framework_handle =
                unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
            if framework_handle.is_null() {
                return Err("Sparkle.framework is absent from this development build");
            }

            let Some(class) = AnyClass::get(c"SPUStandardUpdaterController") else {
                unsafe { libc::dlclose(framework_handle) };
                return Err("Sparkle.framework did not register SPUStandardUpdaterController");
            };
            let controller = unsafe {
                let allocated: *mut AnyObject = msg_send![class, alloc];
                let initialized: *mut AnyObject = msg_send![
                    allocated,
                    initWithStartingUpdater: true,
                    updaterDelegate: std::ptr::null_mut::<AnyObject>(),
                    userDriverDelegate: std::ptr::null_mut::<AnyObject>()
                ];
                Retained::from_raw(initialized)
            };
            let Some(controller) = controller else {
                unsafe { libc::dlclose(framework_handle) };
                return Err("SPUStandardUpdaterController initialization returned nil");
            };

            Ok(Self {
                controller,
                _framework_handle: framework_handle,
            })
        }

        pub(super) fn check_for_updates(&self) {
            if unsafe { libc::pthread_main_np() } != 1 {
                tracing::error!("refusing to invoke Sparkle outside the main thread");
                return;
            }
            unsafe {
                let _: () = msg_send![
                    &*self.controller,
                    checkForUpdates: std::ptr::null_mut::<AnyObject>()
                ];
            }
        }
    }

    fn framework_path() -> Result<PathBuf, &'static str> {
        let executable = std::env::current_exe()
            .map_err(|_| "cannot resolve the current executable for Sparkle")?;
        let macos_dir = executable
            .parent()
            .ok_or("current executable has no parent directory")?;
        Ok(macos_dir.join("../Frameworks/Sparkle.framework/Sparkle"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    /// 开发/测试 bundle 未嵌入 Sparkle.framework 时，App-global updater 安全降级且可重复安装。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   安装调用次数 = 2：首次创建 global + 第二次命中幂等 guard
    ///   预期 global 数 = 1：同一 `App` 只持有一个 updater owner
    ///   测试 bundle Framework 数 = 0：cargo test 不构造 `Contents/Frameworks`
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 在真实 `TestAppContext` 调用 `install` 两次。
    ///   2. loader 检测非主线程或缺失 Framework，保存 `Unavailable`，不 panic。
    ///   3. 调用主动检查入口；它只记录 unavailable，不创建自定义 UI 或轮询。
    ///
    /// 预期结果：
    ///   - 正断言：App 中存在一个 `SparkleUpdater` global，生命周期由 App 持有。
    ///   - 负断言：测试 bundle 不得被误报为 Ready，也不能因主动检查而崩溃。
    #[test]
    fn test_missing_framework_degrades_to_one_unavailable_app_global() {
        let test_cx = TestAppContext::single();
        test_cx.update(|cx| {
            install(cx);
            install(cx);
            check_for_updates(cx);
        });

        assert!(
            test_cx.has_global::<SparkleUpdater>(),
            "两次 install 后必须存在唯一 App-global SparkleUpdater owner"
        );
        let is_ready = test_cx.read_global(|updater: &SparkleUpdater, _cx| {
            #[cfg(target_os = "macos")]
            {
                matches!(updater.backend, UpdaterBackend::Ready(_))
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = updater;
                false
            }
        });
        assert!(
            !is_ready,
            "未嵌入 Sparkle.framework 的测试 bundle 不应被误报为 Ready"
        );
    }
}
