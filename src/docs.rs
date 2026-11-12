use crate::{
    common::{app_icon, wide},
    editor, theme,
};
use std::{
    io,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::{ERROR_CLASS_ALREADY_EXISTS, HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{GetStockObject, DEFAULT_GUI_FONT, HDC},
    System::LibraryLoader::GetModuleHandleW,
    UI::{Input::KeyboardAndMouse::SetFocus, WindowsAndMessaging::*},
};

const CLASS: &str = "LuaHotkeysDocsWindow";
const REFERENCE: &str = include_str!("editor-help.txt");
const CONTENT: i32 = 1;

pub fn open(owner: HWND, previous: HWND) -> io::Result<HWND> {
    unsafe {
        if IsWindow(previous) != 0 {
            ShowWindow(previous, SW_RESTORE);
            SetForegroundWindow(previous);
            return Ok(previous);
        }
        let instance = GetModuleHandleW(null());
        let class_name = wide(CLASS);
        let class = WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: instance,
            lpszClassName: class_name.as_ptr(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hIcon: app_icon(false),
            ..Default::default()
        };
        if RegisterClassW(&class) == 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(ERROR_CLASS_ALREADY_EXISTS as i32) {
                return Err(error);
            }
        }
        let mut bounds = RECT::default();
        GetWindowRect(owner, &mut bounds);
        let window = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class_name.as_ptr(),
            wide("LuaHotkeys Docs").as_ptr(),
            WS_OVERLAPPEDWINDOW,
            bounds.left + 32,
            bounds.top + 44,
            680,
            560,
            owner,
            null_mut(),
            instance,
            null(),
        );
        if window.is_null() {
            return Err(io::Error::last_os_error());
        }
        theme::title_bar(window);
        SendMessageW(
            window,
            WM_SETICON,
            ICON_SMALL as usize,
            app_icon(true) as isize,
        );
        ShowWindow(window, SW_SHOW);
        SetFocus(GetDlgItem(window, CONTENT));
        Ok(window)
    }
}

unsafe extern "system" fn procedure(window: HWND, id: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        match id {
            WM_CREATE => {
                let content = CreateWindowExW(
                    0,
                    wide("RICHEDIT50W").as_ptr(),
                    null(),
                    WS_CHILD
                        | WS_VISIBLE
                        | WS_TABSTOP
                        | WS_VSCROLL
                        | ES_MULTILINE as u32
                        | ES_READONLY as u32
                        | ES_AUTOVSCROLL as u32,
                    0,
                    0,
                    0,
                    0,
                    window,
                    CONTENT as usize as HMENU,
                    GetModuleHandleW(null()),
                    null(),
                );
                if content.is_null() {
                    return -1;
                }
                SendMessageW(
                    content,
                    WM_SETFONT,
                    GetStockObject(DEFAULT_GUI_FONT) as usize,
                    1,
                );
                SendMessageW(content, editor::EM_SETBKGNDCOLOR, 0, theme::EDITOR as isize);
                editor::default_color(content);
                SetWindowTextW(content, wide(REFERENCE).as_ptr());
                editor::color_all(content);
                theme::scrollbars(content);
                resize(window);
                0
            }
            WM_SIZE => {
                resize(window);
                0
            }
            WM_ERASEBKGND => {
                theme::paint_background(window, w as HDC);
                1
            }
            _ => DefWindowProcW(window, id, w, l),
        }
    }
}

fn resize(window: HWND) {
    unsafe {
        let mut client = RECT::default();
        GetClientRect(window, &mut client);
        let content = GetDlgItem(window, CONTENT);
        if !content.is_null() {
            MoveWindow(
                content,
                16,
                16,
                (client.right - 32).max(1),
                (client.bottom - 32).max(1),
                1,
            );
        }
    }
}
