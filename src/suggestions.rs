use std::{
    io,
    ptr::{null, null_mut},
};

use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, RECT},
    Graphics::Gdi::{GetStockObject, InvalidateRect, MapWindowPoints, DEFAULT_GUI_FONT},
    System::LibraryLoader::GetModuleHandleW,
    UI::{Controls::DRAWITEMSTRUCT, Input::KeyboardAndMouse::GetFocus, WindowsAndMessaging::*},
};

use crate::{
    common::wide,
    completion::{self, Suggestions},
    editor, theme,
};

pub const ID: u16 = 113;
const ROW_HEIGHT: i32 = 24;
const ROWS: usize = 7;

pub struct Popup {
    window: HWND,
    suggestions: Option<Suggestions>,
}

impl Popup {
    pub fn new() -> Self {
        Self {
            window: null_mut(),
            suggestions: None,
        }
    }

    pub fn is_open(&self) -> bool {
        self.suggestions.is_some()
    }

    pub fn is_window(&self, window: HWND) -> bool {
        self.is_open() && self.window == window
    }

    pub fn dismiss(&mut self) {
        if self.suggestions.take().is_some() {
            unsafe { ShowWindow(self.window, SW_HIDE) };
        }
    }

    pub fn show(&mut self, owner: HWND, editor: HWND, manual: bool) -> io::Result<()> {
        if unsafe { GetFocus() } != editor {
            self.dismiss();
            return Ok(());
        }
        let Some((source, caret, selection)) = current(editor) else {
            self.dismiss();
            return Ok(());
        };
        let Some(suggestions) = completion::suggest(&source, caret, manual) else {
            self.dismiss();
            return Ok(());
        };

        let mut point = editor::position(editor, selection);
        let mut bounds = RECT::default();
        unsafe { GetClientRect(editor, &mut bounds) };
        if point.x < 0 || point.x >= bounds.right || point.y < 0 || point.y >= bounds.bottom {
            self.dismiss();
            return Ok(());
        }
        unsafe {
            MapWindowPoints(editor, owner, &mut point, 1);
            GetClientRect(owner, &mut bounds);
        }
        if self.window.is_null() {
            self.create(owner)?;
        }
        let width = (bounds.right - 24).clamp(180, 332);
        let height = suggestions.items.len().min(ROWS) as i32 * ROW_HEIGHT + 2;
        let x = point.x.clamp(10, (bounds.right - width - 10).max(10));
        let y = if point.y + 22 + height <= bounds.bottom - 8 {
            point.y + 22
        } else {
            (point.y - height).max(8)
        };

        unsafe {
            SendMessageW(self.window, LB_RESETCONTENT, 0, 0);
            for item in &suggestions.items {
                let caption = wide(format!("{}   {}", item.name, item.detail));
                SendMessageW(self.window, LB_ADDSTRING, 0, caption.as_ptr() as isize);
            }
            SendMessageW(self.window, LB_SETCURSEL, 0, 0);
            self.suggestions = Some(suggestions);
            SetWindowPos(
                self.window,
                HWND_TOP,
                x,
                y,
                width,
                height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            InvalidateRect(self.window, null(), 0);
        }
        Ok(())
    }

    fn create(&mut self, owner: HWND) -> io::Result<()> {
        let window = unsafe {
            CreateWindowExW(
                0,
                wide("LISTBOX").as_ptr(),
                wide("").as_ptr(),
                WS_CHILD
                    | WS_BORDER
                    | WS_CLIPSIBLINGS
                    | WS_VSCROLL
                    | LBS_OWNERDRAWFIXED as u32
                    | LBS_HASSTRINGS as u32
                    | LBS_NOINTEGRALHEIGHT as u32,
                0,
                0,
                0,
                0,
                owner,
                ID as usize as HMENU,
                GetModuleHandleW(null()),
                null(),
            )
        };
        if window.is_null() {
            return Err(io::Error::last_os_error());
        }
        unsafe {
            SendMessageW(
                window,
                WM_SETFONT,
                GetStockObject(DEFAULT_GUI_FONT) as usize,
                1,
            );
            SendMessageW(window, LB_SETITEMHEIGHT, 0, ROW_HEIGHT as isize);
        }
        theme::scrollbars(window);
        self.window = window;
        Ok(())
    }

    pub fn paint(&self, item: &DRAWITEMSTRUCT) {
        if let Some(candidate) = self
            .suggestions
            .as_ref()
            .and_then(|list| list.items.get(item.itemID as usize))
        {
            theme::suggestion(item, candidate.name, candidate.detail);
        }
    }

    pub fn move_selection(&self, delta: isize) {
        let Some(suggestions) = &self.suggestions else {
            return;
        };
        let selected = unsafe { SendMessageW(self.window, LB_GETCURSEL, 0, 0) };
        let count = suggestions.items.len() as isize;
        let next = (selected.max(0) + delta).rem_euclid(count);
        unsafe {
            SendMessageW(self.window, LB_SETCURSEL, next as usize, 0);
            InvalidateRect(self.window, null(), 0);
        }
    }

    pub fn accept(&mut self, editor: HWND) -> bool {
        let index = unsafe { SendMessageW(self.window, LB_GETCURSEL, 0, 0) };
        self.apply(editor, index as usize)
    }

    pub fn click(&mut self, editor: HWND, position: LPARAM) -> bool {
        let hit = unsafe { SendMessageW(self.window, LB_ITEMFROMPOINT, 0, position) } as u32;
        if hit >> 16 != 0 {
            return false;
        }
        self.apply(editor, hit as u16 as usize)
    }

    fn apply(&mut self, editor: HWND, index: usize) -> bool {
        let Some(item) = self
            .suggestions
            .as_ref()
            .and_then(|list| list.items.get(index))
            .copied()
        else {
            return false;
        };
        self.dismiss();
        let Some((mut source, caret, _)) = current(editor) else {
            return false;
        };
        let Some(suggestions) = completion::suggest(&source, caret, true) else {
            return false;
        };
        if !suggestions.items.contains(&item) {
            return false;
        }
        let edit = suggestions.edit(&source, item);
        let start = source[..suggestions.range.start].encode_utf16().count();
        let end = start + source[suggestions.range.clone()].encode_utf16().count();
        source.replace_range(suggestions.range, &edit.replacement);
        let caret = source[..edit.caret].encode_utf16().count();
        editor::replace(editor, start..end, &edit.replacement, caret);
        true
    }
}

fn current(editor: HWND) -> Option<(String, usize, usize)> {
    let (start, end) = editor::selection(editor);
    if start != end {
        return None;
    }
    let source = editor::text(editor);
    let caret = editor::utf16_to_byte(&source, start)?;
    Some((source, caret, start))
}
