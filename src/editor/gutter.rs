use std::{io, ptr::null};

use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{
        DrawTextW, InvalidateRect, SelectObject, SetBkMode, SetTextColor, DT_NOPREFIX, DT_RIGHT,
        DT_SINGLELINE, TRANSPARENT,
    },
    UI::{
        Controls::DRAWITEMSTRUCT,
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            SendMessageW, WM_CHAR, WM_GETFONT, WM_KEYDOWN, WM_MOUSEWHEEL, WM_NCDESTROY, WM_PAINT,
            WM_SETTEXT, WM_SIZE, WM_VSCROLL,
        },
    },
};

use crate::theme;

const EM_GETLINECOUNT: u32 = 0x00ba;
const EM_LINEINDEX: u32 = 0x00bb;
const EM_GETFIRSTVISIBLELINE: u32 = 0x00ce;
const EM_LINESCROLL: u32 = 0x00b6;
const EM_SCROLLCARET: u32 = 0x00b7;
const EM_REPLACESEL: u32 = 0x00c2;
const SUBCLASS_ID: usize = 1;

fn count(editor: HWND) -> usize {
    unsafe { SendMessageW(editor, EM_GETLINECOUNT, 0, 0).max(1) as usize }
}

pub fn width(editor: HWND) -> i32 {
    let digits = count(editor).ilog10() as i32 + 1;
    18 + digits.max(3) * 9
}

pub fn attach(editor: HWND, gutter: HWND) -> io::Result<()> {
    if unsafe { SetWindowSubclass(editor, Some(sync), SUBCLASS_ID, gutter as usize) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

unsafe extern "system" fn sync(
    editor: HWND,
    message: u32,
    w: WPARAM,
    l: LPARAM,
    id: usize,
    gutter: usize,
) -> LRESULT {
    if message == WM_NCDESTROY {
        unsafe { RemoveWindowSubclass(editor, Some(sync), id) };
    }
    let result = unsafe { DefSubclassProc(editor, message, w, l) };
    if matches!(
        message,
        WM_PAINT
            | WM_SIZE
            | WM_SETTEXT
            | WM_VSCROLL
            | WM_MOUSEWHEEL
            | WM_KEYDOWN
            | WM_CHAR
            | EM_LINESCROLL
            | EM_SCROLLCARET
            | EM_REPLACESEL
            | super::EM_SETSCROLLPOS
    ) {
        unsafe { InvalidateRect(gutter as HWND, null(), 0) };
    }
    result
}

pub fn paint(editor: HWND, item: &DRAWITEMSTRUCT) {
    let area = item.rcItem;
    let dc = item.hDC;
    theme::fill(dc, &area, theme::EDITOR);
    theme::fill(
        dc,
        &RECT {
            left: area.right - 1,
            ..area
        },
        theme::BORDER,
    );
    let font = unsafe { SendMessageW(editor, WM_GETFONT, 0, 0) } as _;
    let previous = unsafe { SelectObject(dc, font) };
    unsafe {
        SetBkMode(dc, TRANSPARENT as i32);
        SetTextColor(dc, theme::MUTED);
    }
    for (line, top) in rows(editor, area.bottom) {
        let label: Vec<_> = line.to_string().encode_utf16().collect();
        let mut row = RECT {
            top,
            right: area.right - 8,
            ..area
        };
        unsafe {
            DrawTextW(
                dc,
                label.as_ptr(),
                label.len() as i32,
                &mut row,
                DT_RIGHT | DT_SINGLELINE | DT_NOPREFIX,
            );
        }
    }
    if !previous.is_null() {
        unsafe { SelectObject(dc, previous) };
    }
}

pub fn rows(editor: HWND, bottom: i32) -> impl Iterator<Item = (usize, i32)> {
    let first = unsafe { SendMessageW(editor, EM_GETFIRSTVISIBLELINE, 0, 0) }.max(0) as usize;
    (first..count(editor)).map_while(move |line| {
        let offset = unsafe { SendMessageW(editor, EM_LINEINDEX, line, 0) };
        if offset < 0 {
            return None;
        }
        let top = super::position(editor, offset as usize).y;
        (top != i32::MIN && top < bottom).then_some((line + 1, top))
    })
}
