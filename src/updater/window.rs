use std::{
    io,
    mem::size_of,
    ptr::{null, null_mut},
    thread,
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    Graphics::Gdi::{GetStockObject, GetSysColorBrush, COLOR_WINDOW, DEFAULT_GUI_FONT},
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Controls::{
            InitCommonControlsEx, ICC_PROGRESS_CLASS, INITCOMMONCONTROLSEX, PBM_SETMARQUEE,
            PBS_MARQUEE,
        },
        WindowsAndMessaging::*,
    },
};

use crate::common::wide;

const TITLE: &str = "LuaHotkeys Updater";
const DONE: u32 = WM_APP + 1;

#[derive(Clone, Copy)]
pub struct Status {
    window: usize,
    label: usize,
}

impl Status {
    pub fn set(self, text: &str) {
        unsafe { SetWindowTextW(self.label as HWND, wide(text).as_ptr()) };
    }

    pub fn retry_close(self) -> bool {
        message(
            self.window as HWND,
            "Save your edits and exit LuaHotkeys from the system tray, then select Retry.",
            MB_RETRYCANCEL | MB_ICONINFORMATION,
        ) == IDRETRY
    }
}

pub fn message(owner: HWND, text: &str, flags: u32) -> i32 {
    unsafe { MessageBoxW(owner, wide(text).as_ptr(), wide(TITLE).as_ptr(), flags) }
}

pub fn run(action: impl FnOnce(Status) -> io::Result<String> + Send + 'static) -> io::Result<i32> {
    let class_name = wide("LuaHotkeysUpdaterWindow");
    let instance = unsafe { GetModuleHandleW(null()) };
    let class = WNDCLASSW {
        lpfnWndProc: Some(procedure),
        hInstance: instance,
        lpszClassName: class_name.as_ptr(),
        hCursor: unsafe { LoadCursorW(null_mut(), IDC_ARROW) },
        hIcon: crate::common::app_icon(false),
        hbrBackground: unsafe { GetSysColorBrush(COLOR_WINDOW) },
        ..Default::default()
    };
    let (window, label) = unsafe {
        let controls = INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_PROGRESS_CLASS,
        };
        InitCommonControlsEx(&controls);
        if RegisterClassW(&class) == 0 {
            return Err(io::Error::last_os_error());
        }
        let window = CreateWindowExW(
            0,
            class_name.as_ptr(),
            wide(TITLE).as_ptr(),
            WS_OVERLAPPED | WS_CAPTION,
            (GetSystemMetrics(SM_CXSCREEN) - 468).max(0) / 2,
            (GetSystemMetrics(SM_CYSCREEN) - 137).max(0) / 2,
            468,
            137,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if window.is_null() {
            return Err(io::Error::last_os_error());
        }
        let label = CreateWindowExW(
            0,
            wide("STATIC").as_ptr(),
            wide("Starting updater…").as_ptr(),
            WS_CHILD | WS_VISIBLE,
            18,
            16,
            410,
            28,
            window,
            null_mut(),
            instance,
            null(),
        );
        let bar = CreateWindowExW(
            0,
            wide("msctls_progress32").as_ptr(),
            null(),
            WS_CHILD | WS_VISIBLE | PBS_MARQUEE,
            18,
            53,
            410,
            18,
            window,
            null_mut(),
            instance,
            null(),
        );
        if label.is_null() || bar.is_null() {
            let error = io::Error::last_os_error();
            DestroyWindow(window);
            return Err(error);
        }
        SendMessageW(
            label,
            WM_SETFONT,
            GetStockObject(DEFAULT_GUI_FONT) as usize,
            1,
        );
        SendMessageW(bar, PBM_SETMARQUEE, 1, 35);
        SendMessageW(
            window,
            WM_SETICON,
            ICON_SMALL as usize,
            crate::common::app_icon(true) as isize,
        );
        ShowWindow(window, SW_SHOW);
        (window, label)
    };
    let status = Status {
        window: window as usize,
        label: label as usize,
    };
    let worker = thread::spawn(move || {
        let result = match action(status) {
            Ok(result) => {
                message(status.window as HWND, &result, MB_ICONINFORMATION);
                0
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => 2,
            Err(error) => {
                message(status.window as HWND, &error.to_string(), MB_ICONERROR);
                1
            }
        };
        unsafe { PostMessageW(status.window as HWND, DONE, 0, 0) };
        result
    });
    let mut msg = MSG::default();
    loop {
        match unsafe { GetMessageW(&mut msg, null_mut(), 0, 0) } {
            -1 => return Err(io::Error::last_os_error()),
            0 => break,
            _ => unsafe {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            },
        }
    }
    worker
        .join()
        .map_err(|_| io::Error::other("The updater stopped unexpectedly."))
}

unsafe extern "system" fn procedure(window: HWND, id: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match id {
        WM_CLOSE => 0,
        DONE => {
            unsafe { DestroyWindow(window) };
            0
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            0
        }
        _ => unsafe { DefWindowProcW(window, id, w, l) },
    }
}
