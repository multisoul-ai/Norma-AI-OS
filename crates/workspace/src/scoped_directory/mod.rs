//! Windows directory capabilities shared by memory publication and read-only project review.
//! Names are single components, opened relative to a pinned directory without following reparses.
mod security;
use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    os::windows::{
        ffi::OsStringExt,
        fs::OpenOptionsExt,
        io::{AsRawHandle, FromRawHandle},
    },
    path::{Component, Path, PathBuf},
    ptr::{null, null_mut},
    sync::Arc,
};
use windows_sys::{
    Wdk::{
        Foundation::OBJECT_ATTRIBUTES,
        Storage::FileSystem::{
            FileRenameInformationEx, NtCreateFile, NtFlushBuffersFile, NtSetInformationFile,
            FILE_CREATE, FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_OPEN, FILE_OPEN_IF,
            FILE_OPEN_REPARSE_POINT, FILE_RENAME_INFORMATION, FILE_RENAME_POSIX_SEMANTICS,
            FILE_RENAME_REPLACE_IF_EXISTS, FILE_SYNCHRONOUS_IO_NONALERT,
        },
    },
    Win32::{
        Foundation::{RtlNtStatusToDosError, OBJ_CASE_INSENSITIVE, UNICODE_STRING},
        Storage::FileSystem::{
            GetFileInformationByHandle, GetFinalPathNameByHandleW, BY_HANDLE_FILE_INFORMATION,
            DELETE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_REPARSE_POINT,
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ,
            FILE_GENERIC_WRITE, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
            FILE_SHARE_WRITE, SYNCHRONIZE,
        },
        System::IO::IO_STATUS_BLOCK,
    },
};

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "unsafe managed path")
}
fn status(code: i32) -> io::Result<()> {
    if code < 0 {
        Err(io::Error::from_raw_os_error(
            unsafe { RtlNtStatusToDosError(code) } as i32,
        ))
    } else {
        Ok(())
    }
}

/// Reject Win32 aliases even though native relative opens do not interpret DOS device names.
pub fn validate_component(name: &str) -> io::Result<()> {
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let reserved = matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || ["COM", "LPT"].iter().any(|prefix| {
        stem.strip_prefix(prefix).is_some_and(|n| {
            matches!(
                n,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
    });
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains(['/', '\\', ':', '\0', '<', '>', '"', '|', '?', '*'])
        || name.ends_with(['.', ' '])
        || reserved
    {
        return Err(invalid());
    }
    Ok(())
}

pub fn information(file: &File) -> io::Result<BY_HANDLE_FILE_INFORMATION> {
    let mut value = BY_HANDLE_FILE_INFORMATION::default();
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut value) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(value)
}
pub fn require_regular(file: &File) -> io::Result<()> {
    let value = information(file)?;
    if value.dwFileAttributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) != 0
        || value.nNumberOfLinks != 1
    {
        return Err(invalid());
    }
    Ok(())
}
pub fn final_path(file: &File) -> io::Result<PathBuf> {
    let mut buffer = vec![0u16; 32768];
    let length = unsafe {
        GetFinalPathNameByHandleW(
            file.as_raw_handle(),
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            0,
        )
    } as usize;
    if length == 0 {
        return Err(io::Error::last_os_error());
    }
    if length >= buffer.len() {
        return Err(invalid());
    }
    Ok(std::ffi::OsString::from_wide(&buffer[..length]).into())
}

pub struct Directory {
    file: Arc<File>,
    ancestors: Vec<Arc<File>>,
    writable: bool,
}
impl Directory {
    pub fn open(path: &Path, writable: bool) -> io::Result<Self> {
        if !path.is_absolute() {
            return Err(invalid());
        }
        let mut parts = path.components();
        let prefix = parts.next().ok_or_else(invalid)?;
        if !matches!(prefix, Component::Prefix(_)) || parts.next() != Some(Component::RootDir) {
            return Err(invalid());
        }
        let mut root = PathBuf::from(prefix.as_os_str());
        root.push("\\");
        let file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(root)?;
        let mut directory = Self {
            file: Arc::new(file),
            ancestors: vec![],
            writable: false,
        };
        let names = parts.collect::<Vec<_>>();
        for (i, part) in names.iter().enumerate() {
            let Component::Normal(name) = part else {
                return Err(invalid());
            };
            let name = name.to_str().ok_or_else(invalid)?;
            directory.writable = writable && i + 1 == names.len();
            directory = directory.child(name, false)?;
        }
        Ok(directory)
    }
    pub fn child(&self, name: &str, create: bool) -> io::Result<Self> {
        if create && !self.writable {
            return Err(io::Error::from(io::ErrorKind::PermissionDenied));
        }
        let access = FILE_GENERIC_READ | if self.writable { FILE_GENERIC_WRITE } else { 0 };
        let file = self.open_relative(
            name,
            access,
            if create { FILE_OPEN_IF } else { FILE_OPEN },
            FILE_DIRECTORY_FILE,
            false,
        )?;
        let info = information(&file)?;
        if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(invalid());
        }
        let mut ancestors = self.ancestors.clone();
        ancestors.push(self.file.clone());
        Ok(Self {
            file: Arc::new(file),
            ancestors,
            writable: self.writable,
        })
    }
    pub fn create_child(&self, name: &str) -> io::Result<Self> {
        self.require_writable()?;
        let file = self.open_relative(
            name,
            FILE_GENERIC_READ | FILE_GENERIC_WRITE,
            FILE_CREATE,
            FILE_DIRECTORY_FILE,
            false,
        )?;
        let mut ancestors = self.ancestors.clone();
        ancestors.push(self.file.clone());
        Ok(Self {
            file: Arc::new(file),
            ancestors,
            writable: true,
        })
    }
    pub fn open_entry(&self, name: &str) -> io::Result<File> {
        self.open_relative(name, FILE_GENERIC_READ, FILE_OPEN, 0, true)
    }
    pub fn open_pinned_entry(&self, name: &str) -> io::Result<File> {
        self.open_relative(name, FILE_GENERIC_READ, FILE_OPEN, 0, false)
    }
    fn open_relative(
        &self,
        name: &str,
        access: u32,
        disposition: u32,
        options: u32,
        share_delete: bool,
    ) -> io::Result<File> {
        validate_component(name)?;
        let mut name = name.encode_utf16().collect::<Vec<_>>();
        let length = u16::try_from(name.len() * 2).map_err(|_| invalid())?;
        let unicode = UNICODE_STRING {
            Length: length,
            MaximumLength: length,
            Buffer: name.as_mut_ptr(),
        };
        let descriptor = if disposition != FILE_OPEN {
            Some(security::private_descriptor()?)
        } else {
            None
        };
        let attributes = OBJECT_ATTRIBUTES {
            Length: size_of::<OBJECT_ATTRIBUTES>() as u32,
            RootDirectory: self.file.as_raw_handle(),
            ObjectName: &unicode,
            Attributes: OBJ_CASE_INSENSITIVE,
            SecurityDescriptor: descriptor.as_ref().map_or(null(), |d| d.0.cast()),
            SecurityQualityOfService: null(),
        };
        let mut file = null_mut();
        let mut io_status = IO_STATUS_BLOCK::default();
        // The relative name, descriptor and parent handle stay alive for this synchronous NT open.
        status(unsafe {
            NtCreateFile(
                &mut file,
                access | SYNCHRONIZE,
                &attributes,
                &mut io_status,
                null(),
                FILE_ATTRIBUTE_NORMAL,
                FILE_SHARE_READ
                    | FILE_SHARE_WRITE
                    | if share_delete { FILE_SHARE_DELETE } else { 0 },
                disposition,
                options | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
                null(),
                0,
            )
        })?;
        Ok(unsafe { File::from_raw_handle(file) })
    }
    pub fn path(&self) -> io::Result<PathBuf> {
        final_path(&self.file)
    }
    pub fn read(&self, name: &str, max: usize) -> io::Result<Vec<u8>> {
        let file = self.open_entry(name)?;
        require_regular(&file)?;
        let mut bytes = Vec::new();
        file.take(max as u64 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > max {
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                "managed byte limit exceeded",
            ));
        }
        Ok(bytes)
    }
    pub fn write_new(&self, name: &str, bytes: &[u8]) -> io::Result<()> {
        self.require_writable()?;
        let mut file = self.open_relative(
            name,
            FILE_GENERIC_READ | FILE_GENERIC_WRITE,
            FILE_CREATE,
            FILE_NON_DIRECTORY_FILE,
            false,
        )?;
        require_regular(&file)?;
        file.write_all(bytes)?;
        file.sync_all()
    }
    pub fn lock_file(&self) -> io::Result<File> {
        self.require_writable()?;
        let file = self.open_relative(
            "LOCK",
            FILE_GENERIC_READ | FILE_GENERIC_WRITE,
            FILE_OPEN_IF,
            FILE_NON_DIRECTORY_FILE,
            false,
        )?;
        require_regular(&file)?;
        Ok(file)
    }
    pub fn rename(&self, from: &str, to: &str) -> io::Result<()> {
        self.require_writable()?;
        validate_component(to)?;
        let file = self.open_relative(
            from,
            DELETE | FILE_READ_ATTRIBUTES,
            FILE_OPEN,
            FILE_NON_DIRECTORY_FILE,
            false,
        )?;
        require_regular(&file)?;
        let name = to.encode_utf16().collect::<Vec<_>>();
        let offset = std::mem::offset_of!(FILE_RENAME_INFORMATION, FileName);
        let length = offset + name.len() * 2;
        let mut buffer = vec![0usize; length.div_ceil(size_of::<usize>())];
        let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
        // Aligned storage includes the complete variable-length UTF-16 name; RootDirectory pins both sides.
        unsafe {
            (*info).Anonymous.Flags = FILE_RENAME_REPLACE_IF_EXISTS | FILE_RENAME_POSIX_SEMANTICS;
            (*info).RootDirectory = self.file.as_raw_handle();
            (*info).FileNameLength = (name.len() * 2) as u32;
            std::ptr::copy_nonoverlapping(
                name.as_ptr(),
                buffer.as_mut_ptr().cast::<u8>().add(offset).cast::<u16>(),
                name.len(),
            );
            let mut io_status = IO_STATUS_BLOCK::default();
            status(NtSetInformationFile(
                file.as_raw_handle(),
                &mut io_status,
                info.cast(),
                length as u32,
                FileRenameInformationEx,
            ))
        }
    }
    fn require_writable(&self) -> io::Result<()> {
        if self.writable {
            Ok(())
        } else {
            Err(io::Error::from(io::ErrorKind::PermissionDenied))
        }
    }
    pub fn sync(&self) -> io::Result<()> {
        let mut io_status = IO_STATUS_BLOCK::default();
        status(unsafe { NtFlushBuffersFile(self.file.as_raw_handle(), &mut io_status) })
    }
}
