use std::{
    cell::{Cell, RefCell},
    collections::{HashSet, VecDeque},
    io,
    mem::size_of,
    ptr::{null, null_mut},
};

use windows_sys::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{GetStockObject, COLOR_WINDOW, DEFAULT_GUI_FONT},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Controls::{
                InitCommonControlsEx, SetScrollInfo, ICC_BAR_CLASSES, INITCOMMONCONTROLSEX,
                TBM_SETPOS, TBM_SETRANGEMAX, TBS_NOTICKS, TRACKBAR_CLASSW, WC_BUTTONW, WC_EDITW,
                WC_STATICW, WM_MOUSELEAVE,
            },
            Input::KeyboardAndMouse::{SetFocus, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT},
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::*,
        },
    },
};

use super::{Control, ControlValue, Event, Gui, Kind, Update, Window};
use crate::{common::wide, input};

const CLASS: PCWSTR = w!("LuaHotkeysMacroWindow");
const FIRST_ID: usize = 100;
const TBM_GETPOS: u32 = WM_USER;
const MARGIN: i32 = 14;
const ROW_GAP: i32 = 8;

struct NativeControl {
    handle: HWND,
    caption: HWND,
    kind: Kind,
    label: String,
    top: i32,
    height: i32,
}

#[derive(Default)]
struct State {
    events: RefCell<VecDeque<Event>>,
    controls: RefCell<Vec<NativeControl>>,
    tracking: RefCell<HashSet<usize>>,
    updating: Cell<bool>,
    height: Cell<i32>,
    scroll: Cell<i32>,
}

pub struct NativeWindow {
    handle: Cell<HWND>,
    state: Box<State>,
}

impl NativeWindow {
    pub fn new(window: &Window) -> io::Result<Self> {
        let state = Box::<State>::default();
        let instance = unsafe { GetModuleHandleW(null()) };
        let common = INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_BAR_CLASSES,
        };
        if unsafe { InitCommonControlsEx(&common) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            hCursor: unsafe { LoadCursorW(null_mut(), IDC_ARROW) },
            hbrBackground: (COLOR_WINDOW + 1) as _,
            lpszClassName: CLASS,
            ..Default::default()
        };
        if unsafe { RegisterClassW(&class) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let height = window.controls.iter().fold(MARGIN * 2, |height, control| {
            height + row_height(control.kind) + ROW_GAP
        });
        state.height.set(height);
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 460,
            bottom: height.clamp(120, 600),
        };
        let style = WS_OVERLAPPEDWINDOW | WS_VSCROLL | WS_CLIPCHILDREN;
        unsafe { AdjustWindowRectEx(&mut rect, style, 0, WS_EX_CONTROLPARENT) };
        let handle = unsafe {
            CreateWindowExW(
                WS_EX_CONTROLPARENT,
                CLASS,
                wide(&window.title).as_ptr(),
                style,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                rect.right - rect.left,
                rect.bottom - rect.top,
                null_mut(),
                null_mut(),
                instance,
                (&*state as *const State).cast(),
            )
        };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let native = Self {
            handle: Cell::new(handle),
            state,
        };
        let mut top = MARGIN;
        for (id, control) in window.controls.iter().enumerate() {
            native.add(id, top, control)?;
            top += row_height(control.kind) + ROW_GAP;
        }
        native.state.layout(handle, 0);
        if window.visible {
            unsafe { ShowWindow(handle, SW_SHOW) };
        }
        Ok(native)
    }

    fn add(&self, id: usize, top: i32, control: &Control) -> io::Result<()> {
        let (class, style, extended) = match control.kind {
            Kind::Label => (WC_STATICW, 0, 0),
            Kind::Button | Kind::Keybind => (WC_BUTTONW, WS_TABSTOP | BS_PUSHBUTTON as u32, 0),
            Kind::Slider { .. } => (TRACKBAR_CLASSW, WS_TABSTOP | TBS_NOTICKS, 0),
            Kind::Toggle => (WC_BUTTONW, WS_TABSTOP | BS_AUTOCHECKBOX as u32, 0),
            Kind::Input => (
                WC_EDITW,
                WS_TABSTOP | ES_AUTOHSCROLL as u32,
                WS_EX_CLIENTEDGE,
            ),
        };
        let id = id + FIRST_ID;
        if id > usize::from(u16::MAX) {
            return Err(io::Error::other("Too many controls in this GUI window."));
        }
        let caption = if has_caption(control.kind) {
            self.child(WC_STATICW, &control.label, 0, 0, 0)?
        } else {
            null_mut()
        };
        let label = if control.kind == Kind::Keybind {
            control.keybind_caption(false)
        } else {
            control.label.clone()
        };
        let handle = self.child(class, &label, style, extended, id)?;
        if matches!(control.kind, Kind::Button | Kind::Keybind) {
            let procedure = if control.kind == Kind::Button {
                button_proc
            } else {
                keybind_proc
            };
            if unsafe {
                SetWindowSubclass(
                    handle,
                    Some(procedure),
                    id - FIRST_ID,
                    (&*self.state as *const State) as usize,
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        if let Kind::Slider { min, max } = control.kind {
            unsafe {
                SendMessageW(handle, TBM_SETRANGEMAX, 0, (max - min) as isize);
            }
        }
        let native = NativeControl {
            handle,
            caption,
            kind: control.kind,
            label: control.label.clone(),
            top,
            height: row_height(control.kind),
        };
        if control.kind != Kind::Keybind {
            native.set(&control.value);
        }
        self.state.controls.borrow_mut().push(native);
        Ok(())
    }

    fn child(
        &self,
        class: PCWSTR,
        text: &str,
        style: u32,
        extended: u32,
        id: usize,
    ) -> io::Result<HWND> {
        let handle = unsafe {
            CreateWindowExW(
                extended,
                class,
                wide(text).as_ptr(),
                WS_CHILD | WS_VISIBLE | style,
                0,
                0,
                0,
                0,
                self.handle.get(),
                id as _,
                GetModuleHandleW(null()),
                null(),
            )
        };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        unsafe {
            SendMessageW(
                handle,
                WM_SETFONT,
                GetStockObject(DEFAULT_GUI_FONT) as usize,
                1,
            )
        };
        Ok(handle)
    }

    pub fn handle(&self) -> HWND {
        self.handle.get()
    }

    pub fn events(&self) -> VecDeque<Event> {
        std::mem::take(&mut *self.state.events.borrow_mut())
    }

    pub fn capture_key(&self, message: &MSG, gui: &Gui) -> bool {
        let Some(id) = gui.recording() else {
            return false;
        };
        let handle = self.state.controls.borrow()[id].handle;
        if message.hwnd != handle
            || !matches!(
                message.message,
                WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP
            )
        {
            return false;
        }
        let released = matches!(message.message, WM_KEYUP | WM_SYSKEYUP);
        if released || message.lParam & (1 << 30) == 0 {
            if let Some(combo) = input::capture_key(
                message.wParam as u16,
                released,
                message.lParam & (1 << 24) != 0,
            ) {
                gui.event(Event::Key(id, combo));
                return true;
            }
        }
        !released
    }

    pub fn sync(&self, gui: &Gui) {
        let updates = gui.updates();
        let window = gui.window();
        let window = window.as_ref().expect("native GUI");
        self.state.updating.set(true);
        for update in updates {
            let handle = self.handle.get();
            if handle.is_null() {
                break;
            }
            match update {
                Update::Close => self.destroy(),
                Update::Visibility(visible) => unsafe {
                    if !visible {
                        self.state.tracking.borrow_mut().clear();
                    }
                    ShowWindow(handle, if visible { SW_SHOW } else { SW_HIDE });
                },
                Update::Control(id) => {
                    let controls = self.state.controls.borrow();
                    let native = &controls[id];
                    let control = &window.controls[id];
                    if control.kind == Kind::Keybind {
                        let recording = window.recording == Some(id);
                        unsafe {
                            SetWindowTextW(
                                native.handle,
                                wide(&control.keybind_caption(recording)).as_ptr(),
                            );
                            if recording {
                                SetFocus(native.handle);
                            }
                        }
                    } else {
                        native.set(&control.value);
                    }
                }
            }
        }
        self.state.updating.set(false);
    }

    fn destroy(&self) {
        let handle = self.handle.replace(null_mut());
        if !handle.is_null() {
            unsafe { DestroyWindow(handle) };
        }
    }
}

impl Drop for NativeWindow {
    fn drop(&mut self) {
        self.destroy();
    }
}

fn has_caption(kind: Kind) -> bool {
    matches!(kind, Kind::Slider { .. } | Kind::Input)
}

fn row_height(kind: Kind) -> i32 {
    match kind {
        Kind::Label => 22,
        Kind::Button | Kind::Toggle | Kind::Keybind => 31,
        Kind::Slider { .. } | Kind::Input => 48,
    }
}

impl NativeControl {
    fn set(&self, value: &ControlValue) {
        unsafe {
            match value {
                ControlValue::Text(text) => {
                    SetWindowTextW(self.handle, wide(text).as_ptr());
                }
                ControlValue::Number(value) => {
                    if let Kind::Slider { min, .. } = self.kind {
                        SendMessageW(self.handle, TBM_SETPOS, 1, (*value - min) as isize);
                        self.slider_caption(*value);
                    }
                }
                ControlValue::Boolean(value) => {
                    SendMessageW(self.handle, BM_SETCHECK, usize::from(*value), 0);
                }
                ControlValue::None => {}
            }
        }
    }

    fn slider_caption(&self, value: i32) {
        let text = wide(&format!("{} ({value})", self.label));
        unsafe { SetWindowTextW(self.caption, text.as_ptr()) };
    }
}

impl State {
    fn emit(&self, window: HWND, event: Event) {
        let mut events = self.events.borrow_mut();
        let wake = events.is_empty();
        events.push_back(event);
        if wake {
            // Accessibility tools can send notifications while GetMessage is blocked.
            unsafe { PostMessageW(window, WM_NULL, 0, 0) };
        }
    }

    fn layout(&self, handle: HWND, position: i32) {
        let mut rect = RECT::default();
        unsafe { GetClientRect(handle, &mut rect) };
        let height = (rect.bottom - rect.top).max(0);
        let limit = (self.height.get() - height).max(0);
        let position = position.clamp(0, limit);
        self.scroll.set(position);
        let info = SCROLLINFO {
            cbSize: size_of::<SCROLLINFO>() as u32,
            fMask: SIF_RANGE | SIF_PAGE | SIF_POS,
            nMax: self.height.get() - 1,
            nPage: height as u32,
            nPos: position,
            ..Default::default()
        };
        self.scroll
            .set(unsafe { SetScrollInfo(handle, SB_VERT, &info, 1) });
        unsafe { GetClientRect(handle, &mut rect) };
        let width = (rect.right - rect.left - MARGIN * 2).max(0);
        for control in self.controls.borrow().iter() {
            let top = control.top - self.scroll.get();
            let caption = if has_caption(control.kind) { 20 } else { 0 };
            unsafe {
                if caption != 0 {
                    MoveWindow(control.caption, MARGIN, top, width, caption, 1);
                }
                MoveWindow(
                    control.handle,
                    MARGIN,
                    top + caption,
                    width,
                    control.height - caption,
                    1,
                );
            }
        }
    }

    fn scroll(&self, handle: HWND, command: i32) {
        let position = self.scroll.get();
        let mut info = SCROLLINFO {
            cbSize: size_of::<SCROLLINFO>() as u32,
            fMask: SIF_TRACKPOS | SIF_PAGE,
            ..Default::default()
        };
        unsafe { GetScrollInfo(handle, SB_VERT, &mut info) };
        let next = match command {
            SB_LINEUP => position - 30,
            SB_LINEDOWN => position + 30,
            SB_PAGEUP => position - info.nPage as i32,
            SB_PAGEDOWN => position + info.nPage as i32,
            SB_THUMBTRACK | SB_THUMBPOSITION => info.nTrackPos,
            SB_TOP => 0,
            SB_BOTTOM => self.height.get(),
            _ => return,
        };
        self.layout(handle, next);
    }

    fn command(&self, child: HWND, notification: u32) {
        if child.is_null() || self.updating.get() {
            return;
        }
        let Some(id) = (unsafe { GetDlgCtrlID(child) } as usize).checked_sub(FIRST_ID) else {
            return;
        };
        let controls = self.controls.borrow();
        let Some(control) = controls.get(id) else {
            return;
        };
        let event = match (control.kind, notification) {
            (Kind::Button, BN_CLICKED) => Event::Press(id),
            (Kind::Keybind, BN_CLICKED) => Event::Record(id),
            (Kind::Toggle, BN_CLICKED) => {
                let checked = unsafe { SendMessageW(child, BM_GETCHECK, 0, 0) } != 0;
                Event::Change(id, ControlValue::Boolean(checked))
            }
            (Kind::Input, EN_CHANGE) => {
                let length = unsafe { GetWindowTextLengthW(child) } + 1;
                let mut text = vec![0; length as usize];
                let length = unsafe { GetWindowTextW(child, text.as_mut_ptr(), length) };
                Event::Change(
                    id,
                    ControlValue::Text(String::from_utf16_lossy(&text[..length as usize])),
                )
            }
            _ => return,
        };
        self.emit(unsafe { GetParent(child) }, event);
    }

    fn slider(&self, child: HWND) {
        if child.is_null() || self.updating.get() {
            return;
        }
        let Some(id) = (unsafe { GetDlgCtrlID(child) } as usize).checked_sub(FIRST_ID) else {
            return;
        };
        let controls = self.controls.borrow();
        let Some(control) = controls.get(id) else {
            return;
        };
        let Kind::Slider { min, .. } = control.kind else {
            return;
        };
        let value = (unsafe { SendMessageW(child, TBM_GETPOS, 0, 0) } as i32) + min;
        control.slider_caption(value);
        self.emit(
            unsafe { GetParent(child) },
            Event::Change(id, ControlValue::Number(value)),
        );
    }
}

unsafe extern "system" fn window_proc(
    handle: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = &*(lparam as *const CREATESTRUCTW);
        SetWindowLongPtrW(handle, GWLP_USERDATA, create.lpCreateParams as _);
    }
    if let Some(state) = (GetWindowLongPtrW(handle, GWLP_USERDATA) as *const State).as_ref() {
        match message {
            WM_CLOSE => {
                state.emit(handle, Event::Close);
                return 0;
            }
            WM_COMMAND => state.command(lparam as HWND, (wparam >> 16) as u32 & 0xffff),
            WM_HSCROLL => state.slider(lparam as HWND),
            WM_SIZE => state.layout(handle, state.scroll.get()),
            WM_VSCROLL => state.scroll(handle, (wparam & 0xffff) as i32),
            WM_MOUSEWHEEL => {
                let steps = ((wparam >> 16) as i16 as i32) / 120;
                state.layout(handle, state.scroll.get() - steps * 60);
            }
            _ => {}
        }
    }
    DefWindowProcW(handle, message, wparam, lparam)
}

unsafe extern "system" fn button_proc(
    handle: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    context: usize,
) -> LRESULT {
    let state = &*(context as *const State);
    match message {
        WM_MOUSEMOVE if !state.tracking.borrow().contains(&id) => {
            let mut track = TRACKMOUSEEVENT {
                cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: handle,
                dwHoverTime: 0,
            };
            if TrackMouseEvent(&mut track) != 0 {
                state.tracking.borrow_mut().insert(id);
                state.emit(GetParent(handle), Event::Hover(id, true));
            }
        }
        WM_MOUSELEAVE if state.tracking.borrow_mut().remove(&id) => {
            state.emit(GetParent(handle), Event::Hover(id, false));
        }
        WM_NCDESTROY => {
            RemoveWindowSubclass(handle, Some(button_proc), id);
        }
        _ => {}
    }
    DefSubclassProc(handle, message, wparam, lparam)
}

unsafe extern "system" fn keybind_proc(
    handle: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    context: usize,
) -> LRESULT {
    let state = &*(context as *const State);
    match message {
        WM_KILLFOCUS => state.emit(GetParent(handle), Event::CancelRecording(id)),
        WM_NCDESTROY => {
            RemoveWindowSubclass(handle, Some(keybind_proc), id);
        }
        _ => {}
    }
    DefSubclassProc(handle, message, wparam, lparam)
}
