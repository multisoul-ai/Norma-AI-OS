//! 系统无障碍偏好。只在调用时读取，不缓存；设置变化在下一次同步时生效。

/// macOS “减少动态效果”。其他平台没有这个概念，视为关闭。
#[cfg(target_os = "macos")]
pub(crate) fn system_reduce_motion() -> bool {
    objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn system_reduce_motion() -> bool {
    false
}
