//! Keep file-permission tests independent of the hosted runner's enabled restore privilege.
use std::{
    marker::PhantomData,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    rc::Rc,
};
use windows_sys::Win32::{
    Foundation::ERROR_NO_TOKEN,
    Security::{
        CreateRestrictedToken, ImpersonateLoggedOnUser, RevertToSelf, DISABLE_MAX_PRIVILEGE,
        TOKEN_DUPLICATE, TOKEN_QUERY,
    },
    System::Threading::{GetCurrentProcess, GetCurrentThread, OpenProcessToken, OpenThreadToken},
};

// Impersonation belongs to this thread; the guard must not move to another one.
pub(super) struct WithoutTokenPrivileges(PhantomData<Rc<()>>);

impl WithoutTokenPrivileges {
    pub(super) fn new() -> Self {
        let mut previous = std::ptr::null_mut();
        let has_previous =
            unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut previous) };
        let error = std::io::Error::last_os_error();
        if has_previous != 0 {
            let _previous = unsafe { OwnedHandle::from_raw_handle(previous) };
            panic!("权限夹具必须从没有模拟token的独立测试线程开始");
        }
        assert_eq!(
            error.raw_os_error(),
            Some(ERROR_NO_TOKEN as i32),
            "必须确实没有旧线程token，而不是查询失败：{error}"
        );
        let mut process = std::ptr::null_mut();
        assert_ne!(
            unsafe {
                OpenProcessToken(
                    GetCurrentProcess(),
                    TOKEN_QUERY | TOKEN_DUPLICATE,
                    &mut process,
                )
            },
            0,
            "必须读取实际进程token：{}",
            std::io::Error::last_os_error()
        );
        let process = unsafe { OwnedHandle::from_raw_handle(process) };
        let mut restricted = std::ptr::null_mut();
        assert_ne!(
            unsafe {
                CreateRestrictedToken(
                    process.as_raw_handle(),
                    DISABLE_MAX_PRIVILEGE,
                    0,
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    &mut restricted,
                )
            },
            0,
            "必须创建不带备份/恢复特权的同用户token：{}",
            std::io::Error::last_os_error()
        );
        let restricted = unsafe { OwnedHandle::from_raw_handle(restricted) };
        assert_ne!(
            unsafe { ImpersonateLoggedOnUser(restricted.as_raw_handle()) },
            0,
            "只能在当前测试线程使用受限token：{}",
            std::io::Error::last_os_error()
        );
        Self(PhantomData)
    }
}

impl Drop for WithoutTokenPrivileges {
    fn drop(&mut self) {
        assert_ne!(
            unsafe { RevertToSelf() },
            0,
            "离开权限夹具必须恢复原线程身份：{}",
            std::io::Error::last_os_error()
        );
    }
}
