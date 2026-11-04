use std::{
    ffi::c_void,
    io,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    ptr::null_mut,
};
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            SDDL_REVISION_1,
        },
        GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER,
    },
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

pub(super) struct Descriptor(pub *mut c_void);
impl Drop for Descriptor {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}
fn checked(value: i32) -> io::Result<()> {
    if value == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub(super) fn private_descriptor() -> io::Result<Descriptor> {
    // All SID pointers remain inside their owned token/buffer or LocalAlloc allocation.
    unsafe {
        let mut token = null_mut();
        checked(OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_QUERY,
            &mut token,
        ))?;
        let token = OwnedHandle::from_raw_handle(token);
        let mut length = 0;
        GetTokenInformation(token.as_raw_handle(), TokenUser, null_mut(), 0, &mut length);
        if length == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut bytes = vec![0usize; (length as usize).div_ceil(size_of::<usize>())];
        checked(GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            bytes.as_mut_ptr().cast(),
            length,
            &mut length,
        ))?;
        let user = &*bytes.as_ptr().cast::<TOKEN_USER>();
        let mut sid = null_mut();
        checked(ConvertSidToStringSidW(user.User.Sid, &mut sid))?;
        let _allocation = Descriptor(sid.cast());
        let mut count = 0;
        while *sid.add(count) != 0 {
            count += 1;
        }
        let sid = String::from_utf16_lossy(std::slice::from_raw_parts(sid, count));
        let text = format!("O:{sid}D:P(A;OICI;FA;;;{sid})\0")
            .encode_utf16()
            .collect::<Vec<_>>();
        let mut descriptor = null_mut();
        checked(ConvertStringSecurityDescriptorToSecurityDescriptorW(
            text.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            null_mut(),
        ))?;
        Ok(Descriptor(descriptor))
    }
}
