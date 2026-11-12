use std::{mem::size_of, ptr::null};

use windows_sys::Win32::{
    Foundation::{HWND, RECT},
    Graphics::{
        Dwm::{
            DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
        },
        Gdi::{
            DrawTextW, FillRect, FrameRect, GetStockObject, SelectObject, SetBkColor, SetBkMode,
            SetDCBrushColor, SetTextColor, DC_BRUSH, DT_CENTER, DT_END_ELLIPSIS, DT_NOPREFIX,
            DT_RIGHT, DT_SINGLELINE, DT_VCENTER, HDC, TRANSPARENT,
        },
    },
    UI::{
        Controls::{
            SetWindowTheme, DRAWITEMSTRUCT, ODS_DISABLED, ODS_FOCUS, ODS_HOTLIGHT, ODS_SELECTED,
        },
        WindowsAndMessaging::{
            GetClientRect, GetWindowTextLengthW, GetWindowTextW, SendMessageW, WM_GETFONT,
        },
    },
};

use crate::{common::wide, editor::color};

pub const BACKGROUND: u32 = color(17, 23, 34);
pub const EDITOR: u32 = color(12, 19, 30);
pub const ACTIVITY: u32 = color(14, 21, 32);
pub const TEXT: u32 = color(234, 240, 252);
pub const MUTED: u32 = color(162, 177, 200);
pub const BORDER: u32 = color(48, 63, 85);
const BUTTON: u32 = color(28, 39, 57);
const HOVER: u32 = color(39, 54, 77);
const PRESSED: u32 = color(33, 48, 71);
const ACCENT: u32 = color(45, 80, 144);
const ACCENT_HOVER: u32 = color(53, 93, 165);
const FOCUS: u32 = color(126, 176, 248);
const DISABLED: u32 = color(119, 135, 158);

pub fn color_control(dc: HDC, text: u32, background: u32) -> isize {
    unsafe {
        let brush = GetStockObject(DC_BRUSH);
        SetDCBrushColor(dc, background);
        SetTextColor(dc, text);
        SetBkColor(dc, background);
        SetBkMode(dc, TRANSPARENT as i32);
        brush as isize
    }
}
pub fn title_bar(window: HWND) {
    let enabled = 1_i32;
    unsafe {
        DwmSetWindowAttribute(
            window,
            DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
            (&enabled as *const i32).cast(),
            size_of::<i32>() as u32,
        );
        for (attribute, value) in [
            (DWMWA_CAPTION_COLOR, BACKGROUND),
            (DWMWA_TEXT_COLOR, TEXT),
            (DWMWA_BORDER_COLOR, BORDER),
        ] {
            DwmSetWindowAttribute(
                window,
                attribute as u32,
                (&value as *const u32).cast(),
                size_of::<u32>() as u32,
            );
        }
    }
}

pub fn scrollbars(window: HWND) {
    unsafe { SetWindowTheme(window, wide("DarkMode_Explorer").as_ptr(), null()) };
}

pub fn paint_background(window: HWND, dc: HDC) {
    let mut rectangle = RECT::default();
    unsafe { GetClientRect(window, &mut rectangle) };
    fill(dc, &rectangle, BACKGROUND);
}

pub fn fill(dc: HDC, rectangle: &RECT, value: u32) {
    unsafe {
        let brush = GetStockObject(DC_BRUSH);
        SetDCBrushColor(dc, value);
        FillRect(dc, rectangle, brush);
    }
}

pub fn button(item: &DRAWITEMSTRUCT, primary: bool) {
    let disabled = item.itemState & ODS_DISABLED != 0;
    let selected = item.itemState & ODS_SELECTED != 0;
    let hovered = item.itemState & ODS_HOTLIGHT != 0;
    let focused = item.itemState & ODS_FOCUS != 0;
    let background = match (disabled, primary, selected, hovered) {
        (true, _, _, _) => BACKGROUND,
        (_, _, true, _) => PRESSED,
        (_, true, _, true) => ACCENT_HOVER,
        (_, true, _, _) => ACCENT,
        (_, _, _, true) => HOVER,
        _ => BUTTON,
    };
    fill(item.hDC, &item.rcItem, background);
    unsafe {
        let brush = GetStockObject(DC_BRUSH);
        SetDCBrushColor(item.hDC, if focused { FOCUS } else { BORDER });
        FrameRect(item.hDC, &item.rcItem, brush);
    }

    let length = unsafe { GetWindowTextLengthW(item.hwndItem) }.max(0);
    let mut caption = vec![0_u16; length as usize + 1];
    unsafe { GetWindowTextW(item.hwndItem, caption.as_mut_ptr(), length + 1) };
    draw_text(
        item,
        item.rcItem,
        &caption[..length as usize],
        if disabled { DISABLED } else { TEXT },
        DT_CENTER | DT_END_ELLIPSIS,
    );
}

pub fn suggestion(item: &DRAWITEMSTRUCT, name: &str, detail: &str) {
    let active = item.itemState & ODS_SELECTED != 0;
    fill(
        item.hDC,
        &item.rcItem,
        if active { ACCENT } else { ACTIVITY },
    );
    let mut row = item.rcItem;
    row.left += 8;
    row.right -= 9;
    let mut label = row;
    label.right = (label.left + 136).min(row.right);
    draw_text(
        item,
        label,
        &wide(name)[..name.encode_utf16().count()],
        TEXT,
        DT_END_ELLIPSIS,
    );
    row.left = label.right + 3;
    if row.left < row.right {
        draw_text(
            item,
            row,
            &wide(detail)[..detail.encode_utf16().count()],
            if active { TEXT } else { MUTED },
            DT_RIGHT | DT_END_ELLIPSIS,
        );
    }
}

fn draw_text(
    item: &DRAWITEMSTRUCT,
    mut rectangle: RECT,
    caption: &[u16],
    color: u32,
    alignment: u32,
) {
    unsafe {
        let font = SendMessageW(item.hwndItem, WM_GETFONT, 0, 0) as _;
        let old = SelectObject(item.hDC, font);
        SetBkMode(item.hDC, TRANSPARENT as i32);
        SetTextColor(item.hDC, color);
        DrawTextW(
            item.hDC,
            caption.as_ptr(),
            caption.len() as i32,
            &mut rectangle,
            alignment | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
        );
        if !old.is_null() {
            SelectObject(item.hDC, old);
        }
    }
}
