use std::ffi::c_void;
use std::fs::File;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::Path;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{LocalFree, GENERIC_WRITE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo,
    SetSecurityInfo, SDDL_REVISION_1, SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::{
    EqualSid, GetAce, GetSecurityDescriptorControl, GetSecurityDescriptorDacl,
    GetSecurityDescriptorOwner, GetTokenInformation, TokenUser, ACCESS_ALLOWED_ACE,
    DACL_SECURITY_INFORMATION, INHERITED_ACE, OWNER_SECURITY_INFORMATION,
    PROTECTED_DACL_SECURITY_INFORMATION, SECURITY_ATTRIBUTES, SE_DACL_PROTECTED, TOKEN_QUERY,
    TOKEN_USER,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateDirectoryW, CreateFileW, GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    CREATE_NEW, FILE_ALL_ACCESS, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_REPARSE_POINT,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, OPEN_EXISTING, READ_CONTROL, WRITE_DAC,
};
use windows_sys::Win32::System::SystemServices::ACCESS_ALLOWED_ACE_TYPE;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

struct LocalMemory(*mut c_void);

impl Drop for LocalMemory {
    fn drop(&mut self) {
        // Win32 security conversion/query APIs allocate with LocalAlloc.
        unsafe {
            LocalFree(self.0);
        }
    }
}

fn checked(success: i32) -> io::Result<()> {
    if success == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn checked_code(code: u32) -> io::Result<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(code as i32))
    }
}

fn wide_path(path: &Path) -> io::Result<Vec<u16>> {
    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    if wide.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path contains NUL",
        ));
    }
    wide.push(0);
    Ok(wide)
}

fn current_user_sid() -> io::Result<String> {
    let mut token = null_mut();
    // The owned token and aligned buffer remain alive while their SID is converted.
    unsafe {
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
        let mut buffer = vec![0usize; (length as usize).div_ceil(std::mem::size_of::<usize>())];
        checked(GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            buffer.as_mut_ptr().cast(),
            length,
            &mut length,
        ))?;
        let user = &*buffer.as_ptr().cast::<TOKEN_USER>();
        let mut text = null_mut();
        checked(ConvertSidToStringSidW(user.User.Sid, &mut text))?;
        let _allocation = LocalMemory(text.cast());
        let mut count = 0;
        while *text.add(count) != 0 {
            count += 1;
        }
        Ok(String::from_utf16_lossy(std::slice::from_raw_parts(
            text, count,
        )))
    }
}

fn descriptor() -> io::Result<LocalMemory> {
    let sid = current_user_sid()?;
    // Protect the DACL from inherited grants; child files/directories inherit only this owner.
    let sddl: Vec<u16> = format!("O:{sid}D:P(A;OICI;FA;;;{sid})\0")
        .encode_utf16()
        .collect();
    let mut value = null_mut();
    unsafe {
        checked(ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut value,
            null_mut(),
        ))?;
    }
    Ok(LocalMemory(value))
}

fn attributes(descriptor: &LocalMemory) -> SECURITY_ATTRIBUTES {
    SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: 0,
    }
}

fn open_security_handle(path: &Path, access: u32) -> io::Result<File> {
    let path = wide_path(path)?;
    // Bind security operations to one handle, including directories and the reparse point itself.
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let file = unsafe { File::from_raw_handle(handle) };
    let mut information: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    checked(unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut information) })?;
    if information.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "managed security path is a reparse point",
        ));
    }
    Ok(file)
}

pub(super) fn create_file(path: &Path) -> io::Result<File> {
    let path = wide_path(path)?;
    let descriptor = descriptor()?;
    let security = attributes(&descriptor);
    // A private DACL is applied at creation, before any credential bytes are written.
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            GENERIC_WRITE,
            0,
            &security,
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL,
            null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_handle(handle) })
}

pub(super) fn create_private_dir(path: &Path) -> io::Result<()> {
    let wide = wide_path(path)?;
    let descriptor = descriptor()?;
    let security = attributes(&descriptor);
    if unsafe { CreateDirectoryW(wide.as_ptr(), &security) } != 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if error.kind() != io::ErrorKind::AlreadyExists {
        return Err(error);
    }
    if !path.is_dir() {
        return Err(error);
    }
    restrict_to_owner(path)
}

fn security(file: &File) -> io::Result<LocalMemory> {
    let mut descriptor = null_mut();
    unsafe {
        checked_code(GetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            &mut descriptor,
        ))?;
    }
    Ok(LocalMemory(descriptor))
}

fn owner(descriptor: &LocalMemory) -> io::Result<*mut c_void> {
    let mut sid = null_mut();
    let mut defaulted = 0;
    unsafe {
        checked(GetSecurityDescriptorOwner(
            descriptor.0,
            &mut sid,
            &mut defaulted,
        ))?;
    }
    Ok(sid)
}

fn dacl(descriptor: &LocalMemory) -> io::Result<*mut windows_sys::Win32::Security::ACL> {
    let mut acl = null_mut();
    let mut present = 0;
    let mut defaulted = 0;
    unsafe {
        checked(GetSecurityDescriptorDacl(
            descriptor.0,
            &mut present,
            &mut acl,
            &mut defaulted,
        ))?;
    }
    if present == 0 || acl.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "missing private DACL",
        ));
    }
    Ok(acl)
}

pub(super) fn restrict_to_owner(path: &Path) -> io::Result<()> {
    let file = open_security_handle(path, READ_CONTROL | WRITE_DAC)?;
    let actual = security(&file)?;
    let desired = descriptor()?;
    if unsafe { EqualSid(owner(&actual)?, owner(&desired)?) } == 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "managed file belongs to another owner",
        ));
    }
    unsafe {
        checked_code(SetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            dacl(&desired)?,
            null(),
        ))
    }
}

pub(super) fn permissions_are_private(path: &Path) -> io::Result<bool> {
    let file = open_security_handle(path, READ_CONTROL)?;
    file_permissions_are_private(&file)
}

pub(super) fn file_permissions_are_private(file: &File) -> io::Result<bool> {
    let actual = security(file)?;
    let desired = descriptor()?;
    let owner = owner(&desired)?;
    let mut control = 0;
    let mut revision = 0;
    // All pointers below refer to live Win32-allocated descriptors and their bounded ACEs.
    unsafe {
        checked(GetSecurityDescriptorControl(
            actual.0,
            &mut control,
            &mut revision,
        ))?;
        if EqualSid(self::owner(&actual)?, owner) == 0 || control & SE_DACL_PROTECTED == 0 {
            return Ok(false);
        }
        let acl = dacl(&actual)?;
        if (*acl).AceCount != 1 {
            return Ok(false);
        }
        let mut ace = null_mut();
        checked(GetAce(acl, 0, &mut ace))?;
        let ace = &*ace.cast::<ACCESS_ALLOWED_ACE>();
        Ok(u32::from(ace.Header.AceType) == ACCESS_ALLOWED_ACE_TYPE
            && u32::from(ace.Header.AceFlags) & INHERITED_ACE == 0
            && ace.Mask == FILE_ALL_ACCESS
            && EqualSid((&ace.SidStart as *const u32).cast_mut().cast(), owner) != 0)
    }
}
