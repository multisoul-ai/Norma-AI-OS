//! Identity-bound recursive deletion through pinned, non-reparse directory handles.
use super::{information, invalid, Directory};
use std::os::windows::io::AsRawHandle;
use std::{
    fs::File,
    io,
    mem::{offset_of, size_of},
    sync::Arc,
};
use windows_sys::Wdk::Storage::FileSystem::FILE_OPEN;

#[cfg(test)]
#[path = "../../tests/unit/scoped_directory_delete_tests.rs"]
mod tests;
use windows_sys::Win32::{
    Foundation::ERROR_NO_MORE_FILES,
    Storage::FileSystem::{
        FileDispositionInfoEx, FileIdBothDirectoryInfo, FileIdBothDirectoryRestartInfo,
        GetFileInformationByHandleEx, SetFileInformationByHandle, DELETE, FILE_ATTRIBUTE_DIRECTORY,
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_DISPOSITION_FLAG_DELETE,
        FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE, FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
        FILE_DISPOSITION_INFO_EX, FILE_GENERIC_READ, FILE_ID_BOTH_DIR_INFO,
    },
};

impl Directory {
    pub fn identity(&self) -> io::Result<(u32, u64)> {
        let info = information(&self.file)?;
        Ok((
            info.dwVolumeSerialNumber,
            (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
        ))
    }

    /// Remove only the captured directory, retaining every ancestor and child pin until deletion.
    pub fn remove_child_tree(&self, name: &str, expected: (u32, u64)) -> io::Result<bool> {
        self.remove_child_tree_with(name, expected, |_| {})
    }

    fn remove_child_tree_with(
        &self,
        name: &str,
        expected: (u32, u64),
        mut on_pin: impl FnMut(&File),
    ) -> io::Result<bool> {
        self.require_writable()?;
        let file = self.open_relative(name, DELETE | FILE_GENERIC_READ, FILE_OPEN, 0, false)?;
        let info = information(&file)?;
        let identity = (
            info.dwVolumeSerialNumber,
            (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
        );
        if identity != expected
            || info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0
            || info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        {
            return Ok(false);
        }
        self.remove_open_tree(file, &mut on_pin)?;
        Ok(true)
    }

    fn remove_open_tree(&self, file: File, on_pin: &mut impl FnMut(&File)) -> io::Result<()> {
        let mut ancestors = self.ancestors.clone();
        ancestors.push(self.file.clone());
        let node = Directory {
            file: Arc::new(file),
            ancestors,
            writable: true,
        };
        on_pin(&node.file);
        let info = information(&node.file)?;
        if info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0
            && info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT == 0
        {
            for name in node.child_names()? {
                let child =
                    node.open_relative(&name, DELETE | FILE_GENERIC_READ, FILE_OPEN, 0, false)?;
                node.remove_open_tree(child, on_pin)?;
            }
        }
        let disposition = FILE_DISPOSITION_INFO_EX {
            Flags: FILE_DISPOSITION_FLAG_DELETE
                | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS
                | FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE,
        };
        // The same DELETE-capable handle pins this entry; do not modify shared hardlink attributes.
        if unsafe {
            SetFileInformationByHandle(
                node.file.as_raw_handle(),
                FileDispositionInfoEx,
                (&disposition as *const FILE_DISPOSITION_INFO_EX).cast(),
                size_of::<FILE_DISPOSITION_INFO_EX>() as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    fn child_names(&self) -> io::Result<Vec<String>> {
        // 1024 u64s = 8 KiB, aligned for FILE_ID_BOTH_DIR_INFO and larger than a maximal NTFS name.
        let mut storage = vec![0u64; 1024];
        let capacity = storage.len() * size_of::<u64>();
        let header = offset_of!(FILE_ID_BOTH_DIR_INFO, FileName);
        let mut names = Vec::new();
        let mut class = FileIdBothDirectoryRestartInfo;
        loop {
            // Enumeration stays bound to the directory object even if a reparse tag changes in place.
            if unsafe {
                GetFileInformationByHandleEx(
                    self.file.as_raw_handle(),
                    class,
                    storage.as_mut_ptr().cast(),
                    capacity as u32,
                )
            } == 0
            {
                let error = io::Error::last_os_error();
                return if error.raw_os_error() == Some(ERROR_NO_MORE_FILES as i32) {
                    Ok(names)
                } else {
                    Err(error)
                };
            }
            class = FileIdBothDirectoryInfo;
            // Storage is zero-initialized and the synchronous API only writes within its given capacity.
            let bytes =
                unsafe { std::slice::from_raw_parts(storage.as_ptr().cast::<u8>(), capacity) };
            let mut offset = 0;
            loop {
                if offset + header > capacity {
                    return Err(invalid());
                }
                let field = offset + offset_of!(FILE_ID_BOTH_DIR_INFO, FileNameLength);
                let length =
                    u32::from_ne_bytes(bytes[field..field + 4].try_into().map_err(|_| invalid())?)
                        as usize;
                let next = u32::from_ne_bytes(
                    bytes[offset..offset + 4]
                        .try_into()
                        .map_err(|_| invalid())?,
                ) as usize;
                let end = offset
                    .checked_add(header)
                    .and_then(|start| start.checked_add(length))
                    .ok_or_else(invalid)?;
                if length % 2 != 0
                    || end > capacity
                    || (next != 0 && (next < header + length || next > capacity - offset))
                {
                    return Err(invalid());
                }
                let units = bytes[offset + header..end]
                    .chunks_exact(2)
                    .map(|unit| u16::from_ne_bytes([unit[0], unit[1]]))
                    .collect::<Vec<_>>();
                let name = String::from_utf16(&units).map_err(|_| invalid())?;
                if name != "." && name != ".." {
                    super::validate_component(&name)?;
                    names.push(name);
                }
                if next == 0 {
                    break;
                }
                offset += next;
            }
        }
    }
}
