mod content;

use self::content::{Page, CATEGORIES};
use crate::{
    common::{app_icon, wide},
    editor, theme,
};
use std::{
    io,
    mem::size_of,
    ptr::{self, null, null_mut},
};
use windows_sys::Win32::{
    Foundation::{GlobalFree, ERROR_CLASS_ALREADY_EXISTS, HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{GetStockObject, DEFAULT_GUI_FONT, HDC},
    System::{
        DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData},
        LibraryLoader::GetModuleHandleW,
        Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE},
        Ole::CF_UNICODETEXT,
    },
    UI::{Controls::*, Input::KeyboardAndMouse::SetFocus, WindowsAndMessaging::*},
};

const CLASS: &str = "LuaHotkeysDocsWindow";
const CONTENT: i32 = 1;
const NAV: i32 = 2;
const COPY: i32 = 3;
const NAV_LABEL: i32 = 4;
const EM_STREAMIN: u32 = 0x0449;
const SF_RTF: usize = 2;

pub fn open(owner: HWND, previous: HWND) -> io::Result<HWND> {
    unsafe {
        if belongs_to(previous, owner) {
            ShowWindow(previous, SW_RESTORE);
            SetForegroundWindow(previous);
            SetFocus(GetDlgItem(previous, NAV));
            return Ok(previous);
        }
        let controls = INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_TREEVIEW_CLASSES,
        };
        if InitCommonControlsEx(&controls) == 0 {
            return Err(io::Error::last_os_error());
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
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            bounds.left + 24,
            bounds.top + 30,
            900,
            620,
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
        SetFocus(GetDlgItem(window, NAV));
        Ok(window)
    }
}

pub fn handle_tab(window: HWND, owner: HWND, message: &MSG) -> bool {
    belongs_to(window, owner)
        && unsafe { IsChild(window, message.hwnd) != 0 && IsDialogMessageW(window, message) != 0 }
}

fn belongs_to(window: HWND, owner: HWND) -> bool {
    let mut name = [0_u16; 64];
    unsafe {
        if window.is_null() || GetWindow(window, GW_OWNER) != owner {
            return false;
        }
        let written = GetClassNameW(window, name.as_mut_ptr(), name.len() as i32).max(0);
        name[..written as usize]
            .iter()
            .copied()
            .eq(CLASS.encode_utf16())
    }
}

unsafe extern "system" fn procedure(window: HWND, id: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        match id {
            WM_CREATE => {
                if initialize(window) {
                    0
                } else {
                    -1
                }
            }
            WM_SIZE => {
                layout(window);
                0
            }
            WM_GETMINMAXINFO => {
                let limits = &mut *(l as *mut MINMAXINFO);
                limits.ptMinTrackSize.x = 650;
                limits.ptMinTrackSize.y = 420;
                0
            }
            WM_NOTIFY => {
                let notification = &*(l as *const NMHDR);
                if notification.idFrom == NAV as usize && notification.code == TVN_SELCHANGEDW {
                    let changed = &*(l as *const NMTREEVIEWW);
                    if let Some(page) = Page::from_id(changed.itemNew.lParam) {
                        show_page(window, page);
                    }
                }
                0
            }
            WM_COMMAND if w as u16 == COPY as u16 => {
                if let Some(example) = selected_page(window).and_then(Page::example) {
                    let caption = if copy_text(window, example) {
                        "Copied"
                    } else {
                        "Try again"
                    };
                    SetWindowTextW(GetDlgItem(window, COPY), wide(caption).as_ptr());
                }
                0
            }
            WM_DRAWITEM if w == COPY as usize => {
                theme::button(&*(l as *const DRAWITEMSTRUCT), true);
                1
            }
            WM_CTLCOLORSTATIC if GetDlgCtrlID(l as HWND) == NAV_LABEL => {
                theme::color_control(w as HDC, theme::MUTED, theme::BACKGROUND)
            }
            WM_ERASEBKGND => {
                theme::paint_background(window, w as HDC);
                1
            }
            _ => DefWindowProcW(window, id, w, l),
        }
    }
}

fn initialize(window: HWND) -> bool {
    let nav = child(
        window,
        "SysTreeView32",
        "",
        NAV,
        WS_TABSTOP | WS_VSCROLL | TVS_HASBUTTONS | TVS_SHOWSELALWAYS,
    );
    let content = child(
        window,
        "RICHEDIT50W",
        "",
        CONTENT,
        WS_TABSTOP | WS_VSCROLL | ES_MULTILINE as u32 | ES_READONLY as u32 | ES_AUTOVSCROLL as u32,
    );
    let copy = child(
        window,
        "BUTTON",
        "Copy example",
        COPY,
        WS_TABSTOP | BS_OWNERDRAW as u32,
    );
    let label = child(window, "STATIC", "Reference", NAV_LABEL, 0);
    if [nav, content, copy, label]
        .iter()
        .any(|window| window.is_null())
    {
        return false;
    }
    theme::scrollbars(nav);
    theme::scrollbars(content);
    unsafe {
        SendMessageW(nav, TVM_SETUNICODEFORMAT, 1, 0);
        SendMessageW(nav, TVM_SETBKCOLOR, 0, theme::ACTIVITY as isize);
        SendMessageW(nav, TVM_SETTEXTCOLOR, 0, theme::TEXT as isize);
        SendMessageW(nav, TVM_SETITEMHEIGHT, 25, 0);
        SendMessageW(content, editor::EM_SETBKGNDCOLOR, 0, theme::EDITOR as isize);
    }

    let start = insert_item(nav, TVI_ROOT, "Getting started", Page::Start);
    if start == 0 {
        return false;
    }
    for (group, category) in CATEGORIES.iter().enumerate() {
        let node = insert_item(nav, TVI_ROOT, category.name, Page::Category(group));
        if node == 0 {
            return false;
        }
        for (item, function) in category.functions.iter().enumerate() {
            if insert_item(nav, node, function.name, Page::Function(group, item)) == 0 {
                return false;
            }
        }
        unsafe { SendMessageW(nav, TVM_EXPAND, TVE_EXPAND as usize, node) };
    }
    unsafe { SendMessageW(nav, TVM_SELECTITEM, TVGN_CARET as usize, start) };
    layout(window);
    true
}

fn child(owner: HWND, class: &str, text: &str, id: i32, style: u32) -> HWND {
    unsafe {
        let child = CreateWindowExW(
            0,
            wide(class).as_ptr(),
            wide(text).as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS | style,
            0,
            0,
            0,
            0,
            owner,
            id as usize as HMENU,
            GetModuleHandleW(null()),
            null(),
        );
        if !child.is_null() {
            SendMessageW(
                child,
                WM_SETFONT,
                GetStockObject(DEFAULT_GUI_FONT) as usize,
                1,
            );
        }
        child
    }
}

fn layout(window: HWND) {
    unsafe {
        let mut client = RECT::default();
        GetClientRect(window, &mut client);
        let navigation = (client.right / 4).clamp(160, 218);
        let right = 18 + navigation + 16;
        let height = (client.bottom - 70).max(1);
        for (id, x, y, width, height) in [
            (NAV_LABEL, 18, 23, navigation, 20),
            (NAV, 18, 52, navigation, height),
            (
                CONTENT,
                right,
                52,
                (client.right - right - 18).max(1),
                height,
            ),
            (COPY, (client.right - 152).max(right), 13, 134, 30),
        ] {
            let child = GetDlgItem(window, id);
            if !child.is_null() {
                MoveWindow(child, x, y, width, height, 1);
            }
        }
    }
}

fn insert_item(nav: HWND, parent: HTREEITEM, title: &str, page: Page) -> HTREEITEM {
    let mut text = wide(title);
    let item = TVINSERTSTRUCTW {
        hParent: parent,
        hInsertAfter: TVI_LAST,
        Anonymous: TVINSERTSTRUCTW_0 {
            item: TVITEMW {
                mask: TVIF_TEXT | TVIF_PARAM,
                pszText: text.as_mut_ptr(),
                lParam: page.id(),
                ..Default::default()
            },
        },
    };
    unsafe { SendMessageW(nav, TVM_INSERTITEMW, 0, &item as *const _ as isize) }
}

fn selected_page(window: HWND) -> Option<Page> {
    unsafe {
        let nav = GetDlgItem(window, NAV);
        let selected = SendMessageW(nav, TVM_GETNEXTITEM, TVGN_CARET as usize, 0);
        if selected == 0 {
            return None;
        }
        let mut item = TVITEMW {
            mask: TVIF_PARAM,
            hItem: selected,
            ..Default::default()
        };
        if SendMessageW(nav, TVM_GETITEMW, 0, &mut item as *mut _ as isize) == 0 {
            return None;
        }
        Page::from_id(item.lParam)
    }
}

#[repr(C, packed(4))]
struct EditStream {
    cookie: usize,
    error: u32,
    feed: unsafe extern "system" fn(usize, *mut u8, i32, *mut i32) -> u32,
}

unsafe extern "system" fn feed(
    cookie: usize,
    output: *mut u8,
    capacity: i32,
    written: *mut i32,
) -> u32 {
    unsafe {
        let remaining = &mut *(cookie as *mut &[u8]);
        let count = remaining.len().min(capacity.max(0) as usize);
        if count != 0 {
            ptr::copy_nonoverlapping(remaining.as_ptr(), output, count);
        }
        *remaining = &remaining[count..];
        *written = count as i32;
    }
    0
}

fn show_page(window: HWND, page: Page) {
    let document = page.rtf();
    let mut bytes = document.as_bytes();
    let mut stream = EditStream {
        cookie: &mut bytes as *mut _ as usize,
        error: 0,
        feed,
    };
    unsafe {
        let content = GetDlgItem(window, CONTENT);
        SendMessageW(content, EM_STREAMIN, SF_RTF, &mut stream as *mut _ as isize);
        SendMessageW(content, 0x00b1, 0, 0); // EM_SETSEL
        SendMessageW(content, 0x00b7, 0, 0); // EM_SCROLLCARET
        let copy = GetDlgItem(window, COPY);
        SetWindowTextW(copy, wide("Copy example").as_ptr());
        ShowWindow(
            copy,
            if page.example().is_some() {
                SW_SHOW
            } else {
                SW_HIDE
            },
        );
    }
}

fn copy_text(window: HWND, text: &str) -> bool {
    let utf16 = wide(text.replace('\n', "\r\n"));
    unsafe {
        let memory = GlobalAlloc(GMEM_MOVEABLE, utf16.len() * size_of::<u16>());
        if memory.is_null() {
            return false;
        }
        let output = GlobalLock(memory).cast::<u16>();
        if !output.is_null() {
            ptr::copy_nonoverlapping(utf16.as_ptr(), output, utf16.len());
            GlobalUnlock(memory);
            if OpenClipboard(window) != 0 {
                let copied = EmptyClipboard() != 0
                    && !SetClipboardData(CF_UNICODETEXT as u32, memory).is_null();
                CloseClipboard();
                if copied {
                    return true;
                }
            }
        }
        GlobalFree(memory);
        false
    }
}

#[cfg(test)]
pub(crate) fn verify_reference(window: HWND) {
    unsafe {
        let nav = GetDlgItem(window, NAV);
        let content = GetDlgItem(window, CONTENT);
        let copy = GetDlgItem(window, COPY);
        let start = editor::text(content);
        assert!(start.starts_with("Getting started"));
        assert!(start.contains("hotkey(\"Ctrl+Alt+J\", function()"));
        assert_ne!(IsWindowVisible(copy), 0);
        let mut item = SendMessageW(nav, TVM_GETNEXTITEM, TVGN_ROOT as usize, 0);
        for _ in 0..4 {
            item = SendMessageW(nav, TVM_GETNEXTITEM, TVGN_NEXT as usize, item);
            assert_ne!(item, 0);
        }
        assert_ne!(
            SendMessageW(nav, TVM_SELECTITEM, TVGN_CARET as usize, item),
            0
        );
        assert!(editor::text(content).starts_with("Timing"));
        assert!(editor::text(content).contains("sleep(milliseconds)"));
        assert_eq!(IsWindowVisible(copy), 0);

        item = SendMessageW(nav, TVM_GETNEXTITEM, TVGN_CHILD as usize, item);
        assert_ne!(item, 0);
        assert_ne!(
            SendMessageW(nav, TVM_SELECTITEM, TVGN_CARET as usize, item),
            0
        );
        let page = editor::text(content);
        assert!(page.contains("Parameters"));
        assert!(page.contains("Example"));
        assert_ne!(IsWindowVisible(copy), 0);
        let (prefix, _) = page.split_once("math.floor").unwrap();
        let math = prefix.encode_utf16().count() as i32;
        let colors = editor::foreground(content, math, math + 4);
        assert_eq!(colors, crate::highlight::tokens("math").next().unwrap().2);
        assert_ne!(colors, theme::TEXT);
    }
}
