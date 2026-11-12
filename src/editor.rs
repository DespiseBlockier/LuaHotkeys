use std::{
    ffi::c_void,
    mem::{size_of, transmute},
    ptr::null_mut,
};
use windows_sys::{
    core::GUID,
    Win32::{
        Foundation::{HWND, POINT},
        Graphics::Gdi::InvalidateRect,
        UI::WindowsAndMessaging::{IsWindowVisible, SendMessageW, WM_SETREDRAW},
    },
};

use crate::{common::wide, highlight, theme};

mod indent;

const EM_GETOLEINTERFACE: u32 = 0x043c;
const EM_GETTEXTEX: u32 = 0x045e;
const EM_GETTEXTLENGTHEX: u32 = 0x045f;
const EM_EXGETSEL: u32 = 0x0434;
const EM_EXSETSEL: u32 = 0x0437;
const EM_SETCHARFORMAT: u32 = 0x0444;
pub const EM_GETSCROLLPOS: u32 = 0x04dd;
pub const EM_SETSCROLLPOS: u32 = 0x04de;
pub const EM_SETBKGNDCOLOR: u32 = 0x0443;
pub const EM_SETEVENTMASK: u32 = 0x0445;

pub const fn color(r: u8, g: u8, b: u8) -> u32 {
    r as u32 | (g as u32) << 8 | (b as u32) << 16
}

#[repr(C)]
struct TextLength {
    flags: u32,
    code_page: u32,
}

#[repr(C)]
struct TextRequest {
    bytes: u32,
    flags: u32,
    code_page: u32,
    default_character: *mut c_void,
    used_default: *mut i32,
}

#[repr(C)]
#[derive(Default)]
struct Range {
    start: i32,
    end: i32,
}

#[repr(C)]
struct CharacterFormat {
    size: u32,
    mask: u32,
    effects: u32,
    height: i32,
    offset: i32,
    text_color: u32,
    charset: u8,
    pitch: u8,
    face: [u16; 32],
}

pub fn text(hwnd: HWND) -> String {
    read_text(hwnd).replace('\r', "\n")
}

fn read_text(hwnd: HWND) -> String {
    text_with_flags(hwnd, 0)
}

#[cfg(test)]
pub fn raw_text(hwnd: HWND) -> String {
    text_with_flags(hwnd, 4) // GT_RAWTEXT retains the table delimiters used in selection offsets.
}

fn text_with_flags(hwnd: HWND, flags: u32) -> String {
    // Rich Edit returns one UTF-16 code unit for each native line break without GT_USECRLF.
    let length = TextLength {
        flags: 2 | 8,
        code_page: 1200,
    };
    let units = unsafe { SendMessageW(hwnd, EM_GETTEXTLENGTHEX, &length as *const _ as usize, 0) };
    if units <= 0 {
        return String::new();
    }
    let mut buffer = vec![0_u16; units as usize + 1];
    let request = TextRequest {
        bytes: (buffer.len() * 2) as u32,
        flags,
        code_page: 1200,
        default_character: null_mut(),
        used_default: null_mut(),
    };
    let written = unsafe {
        SendMessageW(
            hwnd,
            EM_GETTEXTEX,
            &request as *const _ as usize,
            buffer.as_mut_ptr() as isize,
        )
    };
    String::from_utf16_lossy(&buffer[..(written.max(0) as usize).min(buffer.len() - 1)])
}

pub fn default_color(hwnd: HWND) {
    set_color(hwnd, 0, theme::TEXT);
}

fn color_all(hwnd: HWND) {
    set_color(hwnd, 4, theme::TEXT);
}

pub fn selection(hwnd: HWND) -> (usize, usize) {
    let mut range = Range::default();
    unsafe { SendMessageW(hwnd, EM_EXGETSEL, 0, &mut range as *mut _ as isize) };
    (range.start.max(0) as usize, range.end.max(0) as usize)
}

pub fn utf16_to_byte(text: &str, position: usize) -> Option<usize> {
    let mut units = 0;
    for (index, character) in text.char_indices() {
        if units == position {
            return Some(index);
        }
        units += character.len_utf16();
        if units > position {
            return None;
        }
    }
    (units == position).then_some(text.len())
}

pub fn position(hwnd: HWND, caret: usize) -> POINT {
    let mut point = POINT {
        x: i32::MIN,
        y: i32::MIN,
    };
    unsafe {
        SendMessageW(hwnd, 0x0426, &mut point as *mut _ as usize, caret as isize); // EM_POSFROMCHAR
        if point.x == i32::MIN {
            SendMessageW(hwnd, 0x00d6, &mut point as *mut _ as usize, caret as isize);
        }
    }
    point
}

pub fn replace(hwnd: HWND, range: std::ops::Range<usize>, value: &str, caret: usize) {
    replace_and_select(hwnd, range, value, caret..caret);
}

fn replace_and_select(
    hwnd: HWND,
    range: std::ops::Range<usize>,
    value: &str,
    selected: std::ops::Range<usize>,
) {
    let range = Range {
        start: range.start as i32,
        end: range.end as i32,
    };
    let cursor = Range {
        start: selected.start as i32,
        end: selected.end as i32,
    };
    let replacement = wide(value.replace('\n', "\r"));
    unsafe {
        SendMessageW(hwnd, 0x0458, 0, 0); // EM_STOPGROUPTYPING
        SendMessageW(hwnd, EM_EXSETSEL, 0, &range as *const _ as isize);
        SendMessageW(hwnd, 0x00c2, 1, replacement.as_ptr() as isize); // EM_REPLACESEL
        SendMessageW(hwnd, EM_EXSETSEL, 0, &cursor as *const _ as isize);
        SendMessageW(hwnd, 0x0458, 0, 0);
        SendMessageW(hwnd, 0x00b7, 0, 0); // EM_SCROLLCARET
    }
}

pub fn indent_key(hwnd: HWND, tab: bool, reverse: bool) -> bool {
    let source = text(hwnd);
    let (start, end) = selection(hwnd);
    let (Some(start), Some(end)) = (utf16_to_byte(&source, start), utf16_to_byte(&source, end))
    else {
        return false;
    };
    let edit = if tab {
        indent::tab(&source, start..end, reverse)
    } else {
        indent::enter(&source, start..end)
    };
    if source[edit.range.clone()] == edit.text {
        return true;
    }
    let offset = source[..edit.range.start].encode_utf16().count();
    let end = offset + source[edit.range.clone()].encode_utf16().count();
    let selected = offset + edit.text[..edit.selected.start].encode_utf16().count()
        ..offset + edit.text[..edit.selected.end].encode_utf16().count();
    replace_and_select(hwnd, offset..end, &edit.text, selected);
    true
}

pub fn highlight(hwnd: HWND) {
    let Some(_undo) = UndoSuspended::new(hwnd) else {
        return;
    };
    let source = read_text(hwnd);
    let visible = unsafe { IsWindowVisible(hwnd) != 0 };
    let mut selection = Range::default();
    let mut scroll = POINT::default();
    unsafe {
        SendMessageW(hwnd, EM_EXGETSEL, 0, &mut selection as *mut _ as isize);
        SendMessageW(hwnd, EM_GETSCROLLPOS, 0, &mut scroll as *mut _ as isize);
        if visible {
            SendMessageW(hwnd, WM_SETREDRAW, 0, 0);
        }
    }

    color_all(hwnd);
    let mut previous_byte = 0;
    let mut previous_unit = 0;
    for (start, end, text_color) in highlight::tokens(&source) {
        let first = previous_unit + source[previous_byte..start].encode_utf16().count();
        let last = first + source[start..end].encode_utf16().count();
        let range = Range {
            start: first as i32,
            end: last as i32,
        };
        unsafe { SendMessageW(hwnd, EM_EXSETSEL, 0, &range as *const _ as isize) };
        set_color(hwnd, 1, text_color);
        previous_byte = end;
        previous_unit = last;
    }
    unsafe {
        SendMessageW(hwnd, EM_EXSETSEL, 0, &selection as *const _ as isize);
        SendMessageW(hwnd, EM_SETSCROLLPOS, 0, &scroll as *const _ as isize);
        if visible {
            SendMessageW(hwnd, WM_SETREDRAW, 1, 0);
            InvalidateRect(hwnd, null_mut(), 0);
        }
    }
}

fn set_color(hwnd: HWND, scope: usize, text_color: u32) {
    let format = CharacterFormat {
        size: size_of::<CharacterFormat>() as u32,
        mask: 0x4000_0000,
        effects: 0,
        height: 0,
        offset: 0,
        text_color,
        charset: 0,
        pitch: 0,
        face: [0; 32],
    };
    unsafe { SendMessageW(hwnd, EM_SETCHARFORMAT, scope, &format as *const _ as isize) };
}

type Release = unsafe extern "system" fn(*mut c_void) -> u32;
type Undo = unsafe extern "system" fn(*mut c_void, i32, *mut i32) -> i32;

struct UndoSuspended {
    document: *mut c_void,
    undo: Undo,
    release: Release,
}

impl UndoSuspended {
    fn new(hwnd: HWND) -> Option<Self> {
        let mut ole: *mut c_void = null_mut();
        let supported =
            unsafe { SendMessageW(hwnd, EM_GETOLEINTERFACE, 0, &mut ole as *mut _ as isize) };
        if supported == 0 || ole.is_null() {
            return None;
        }
        let interface = GUID {
            data1: 0x8cc497c0,
            data2: 0xa1df,
            data3: 0x11ce,
            data4: [0x80, 0x98, 0x00, 0xaa, 0x00, 0x47, 0xbe, 0x5d],
        };
        let mut document: *mut c_void = null_mut();
        unsafe {
            let table = *ole.cast::<*const usize>();
            let query: unsafe extern "system" fn(
                *mut c_void,
                *const GUID,
                *mut *mut c_void,
            ) -> i32 = transmute(*table);
            let release: Release = transmute(*table.add(2));
            let result = query(ole, &interface, &mut document);
            release(ole);
            if result < 0 || document.is_null() {
                return None;
            }
            let table = *document.cast::<*const usize>();
            // ITextDocument::Undo occupies entry 22 in the native TOM vtable.
            let undo: Undo = transmute(*table.add(22));
            let release: Release = transmute(*table.add(2));
            if undo(document, -9999995, null_mut()) < 0 {
                release(document);
                return None;
            }
            Some(Self {
                document,
                undo,
                release,
            })
        }
    }
}

impl Drop for UndoSuspended {
    fn drop(&mut self) {
        unsafe {
            (self.undo)(self.document, -9999994, null_mut());
            (self.release)(self.document);
        }
    }
}

#[cfg(test)]
pub(crate) fn foreground(hwnd: HWND, start: i32, end: i32) -> u32 {
    let range = Range { start, end };
    let mut format = CharacterFormat {
        size: size_of::<CharacterFormat>() as u32,
        mask: 0,
        effects: 0,
        height: 0,
        offset: 0,
        text_color: 0,
        charset: 0,
        pitch: 0,
        face: [0; 32],
    };
    unsafe {
        SendMessageW(hwnd, EM_EXSETSEL, 0, &range as *const _ as isize);
        SendMessageW(hwnd, 0x043a, 1, &mut format as *mut _ as isize); // EM_GETCHARFORMAT
    }
    format.text_color
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::ptr::null;
    use windows_sys::Win32::{
        System::{
            LibraryLoader::{GetModuleHandleW, LoadLibraryW},
            Ole::{OleInitialize, OleUninitialize},
        },
        UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, SetWindowTextW, ES_MULTILINE, WS_POPUP,
        },
    };

    #[test]
    fn syntax_colors_preserve_unicode_selection_and_undo() {
        unsafe {
            assert!(OleInitialize(null()) >= 0);
            assert!(!LoadLibraryW(wide("msftedit.dll").as_ptr()).is_null());
            let editor = CreateWindowExW(
                0,
                wide("RICHEDIT50W").as_ptr(),
                null(),
                WS_POPUP | ES_MULTILINE as u32,
                0,
                0,
                320,
                160,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            assert!(!editor.is_null());
            let source = "local text = 'é😀'\n--[=[comment\nend]=]\nlog(0xFF)";
            SetWindowTextW(editor, wide(source).as_ptr());
            assert_eq!(text(editor), source);
            let units = source.encode_utf16().count();
            SendMessageW(editor, 0x00b1, units, units as isize);
            SendMessageW(editor, 0x00c2, 1, wide("\r-- undo me").as_ptr() as isize);
            assert_ne!(SendMessageW(editor, 0x00c6, 0, 0), 0);
            let mut before = Range::default();
            SendMessageW(editor, EM_EXGETSEL, 0, &mut before as *mut _ as isize);
            highlight(editor);
            let mut after = Range::default();
            SendMessageW(editor, EM_EXGETSEL, 0, &mut after as *mut _ as isize);
            assert_eq!((before.start, before.end), (after.start, after.end));
            for (start, end, expected) in [
                (0, 5, color(186, 166, 255)),
                (
                    source.split_once("--[=[").unwrap().0.encode_utf16().count() as i32,
                    source.split_once("--[=[").unwrap().0.encode_utf16().count() as i32 + 2,
                    color(200, 213, 236),
                ),
            ] {
                assert_eq!(foreground(editor, start, end), expected);
            }
            SendMessageW(editor, 0x00c7, 0, 0);
            assert_eq!(text(editor), source);
            DestroyWindow(editor);
            OleUninitialize();
        }
    }
}
