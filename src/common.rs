use std::{
    ffi::OsStr,
    io,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0},
    System::Threading::{
        OpenMutexW, ReleaseMutex, WaitForSingleObject, MUTEX_MODIFY_STATE,
        SYNCHRONIZATION_SYNCHRONIZE,
    },
};

pub fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
    value.as_ref().encode_wide().chain(Some(0)).collect()
}

pub fn app_folder() -> io::Result<PathBuf> {
    let executable = std::env::current_exe()?;
    executable
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| io::Error::other("The application folder is unavailable."))
}

#[allow(dead_code)]
pub fn same_path(left: &Path, right: &Path) -> bool {
    folder_key(left) == folder_key(right)
}

fn folder_key(path: &Path) -> String {
    path.as_os_str()
        .to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_uppercase()
}

pub fn update_mutex(folder: &Path) -> Vec<u16> {
    let digest = Sha256::digest(folder_key(folder).as_bytes());
    wide(format!("Local\\LuaHotkeys.Update.{digest:X}"))
}

pub fn close_event(pid: u32) -> Vec<u16> {
    wide(format!("Local\\LuaHotkeys.CloseForUpdate.{pid}"))
}

pub struct Handle(pub HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CloseHandle(self.0) };
        }
    }
}

pub fn try_lock(handle: HANDLE) -> bool {
    matches!(
        unsafe { WaitForSingleObject(handle, 0) },
        WAIT_OBJECT_0 | WAIT_ABANDONED
    )
}

#[allow(dead_code)]
pub fn is_updating(folder: &Path) -> bool {
    let mutex = Handle(unsafe {
        OpenMutexW(
            SYNCHRONIZATION_SYNCHRONIZE | MUTEX_MODIFY_STATE,
            0,
            update_mutex(folder).as_ptr(),
        )
    });
    if mutex.0.is_null() {
        return false;
    }
    if !try_lock(mutex.0) {
        return true;
    }
    unsafe { ReleaseMutex(mutex.0) };
    false
}

pub fn app_icon(small: bool) -> windows_sys::Win32::UI::WindowsAndMessaging::HICON {
    use std::ptr::null;
    use windows_sys::Win32::{
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            GetSystemMetrics, LoadIconW, LoadImageW, IDI_APPLICATION, IMAGE_ICON, LR_SHARED,
            SM_CXSMICON, SM_CYSMICON,
        },
    };
    const RESOURCE: *const u16 = std::ptr::without_provenance(1);
    unsafe {
        let module = GetModuleHandleW(null());
        let icon = if small {
            LoadImageW(
                module,
                RESOURCE,
                IMAGE_ICON,
                GetSystemMetrics(SM_CXSMICON),
                GetSystemMetrics(SM_CYSMICON),
                LR_SHARED,
            )
        } else {
            LoadIconW(module, RESOURCE)
        };
        if icon.is_null() {
            LoadIconW(std::ptr::null_mut(), IDI_APPLICATION)
        } else {
            icon
        }
    }
}
