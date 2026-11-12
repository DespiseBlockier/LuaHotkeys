use std::{
    cell::RefCell,
    ffi::OsString,
    fs,
    io::{self, BufRead, BufReader, Write},
    mem::size_of,
    os::windows::{ffi::OsStringExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    ptr::{null, null_mut},
    sync::mpsc::{self, Receiver, SyncSender},
    thread,
};

use base64::{engine::general_purpose::STANDARD, Engine};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, SYSTEMTIME, WPARAM},
    Graphics::Gdi::{CreateFontW, DeleteObject, GetStockObject, DEFAULT_GUI_FONT, HDC},
    System::{
        LibraryLoader::{GetModuleHandleW, LoadLibraryW},
        Ole::{OleInitialize, OleUninitialize},
        SystemInformation::GetLocalTime,
        Threading::{CreateEventW, WaitForSingleObject, CREATE_NO_WINDOW, INFINITE},
    },
    UI::{
        Controls::Dialogs::{
            GetOpenFileNameW, GetSaveFileNameW, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR,
            OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
        },
        Controls::{DRAWITEMSTRUCT, ODT_BUTTON},
        Input::KeyboardAndMouse::{
            EnableWindow, GetKeyState, SetFocus, VK_CONTROL, VK_DOWN, VK_END, VK_ESCAPE, VK_F5,
            VK_HOME, VK_LEFT, VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_SHIFT, VK_SPACE, VK_TAB,
            VK_UP,
        },
        Shell::{
            Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
            NOTIFYICONDATAW,
        },
        WindowsAndMessaging::*,
    },
};

use crate::{
    association,
    common::{app_folder, close_event, wide, Handle},
    docs, editor,
    suggestions::Popup,
    theme,
};

const RUN: u16 = 101;
const STOP: u16 = 102;
const NEW: u16 = 103;
const OPEN: u16 = 104;
const SAVE: u16 = 105;
const HIDE: u16 = 106;
const ASSOCIATE: u16 = 107;
const EDITOR: u16 = 108;
const SHOW_EDITOR: u16 = 109;
const EXIT: u16 = 110;
const DOCS: u16 = 111;
const UPDATE: u16 = 112;
const TITLE_LABEL: u16 = 121;
const FILE_LABEL: u16 = 122;
const ACTIVITY_LABEL: u16 = 123;
const STATUS_LABEL: u16 = 124;
const TRAY_MESSAGE: u32 = WM_APP + 1;
const WORKER_MESSAGE: u32 = WM_APP + 2;
const UPDATE_CLOSE: u32 = WM_APP + 3;
const SUGGEST: u32 = WM_APP + 4;
const COLOR_TIMER: usize = 1;
const EM_REPLACESEL: u32 = 0x00c2;
const EM_SETSEL: u32 = 0x00b1;
const EM_SCROLLCARET: u32 = 0x00b7;
const EM_EXLIMITTEXT: u32 = 0x0435;
pub const ERROR: u32 = MB_ICONERROR;

pub fn message(window: HWND, content: &str, title: &str, flags: u32) -> i32 {
    unsafe { MessageBoxW(window, wide(content).as_ptr(), wide(title).as_ptr(), flags) }
}

struct WorkerEvent {
    generation: u64,
    line: Option<String>,
}

struct App {
    window: HWND,
    title: HWND,
    buttons: Vec<HWND>,
    file_label: HWND,
    editor: HWND,
    activity_label: HWND,
    activity: HWND,
    status: HWND,
    fonts: Vec<*mut std::ffi::c_void>,
    docs: HWND,
    popup: Popup,
    path: Option<PathBuf>,
    saved: String,
    worker: Option<Child>,
    generation: u64,
    started_in_tray: bool,
    taskbar_created: u32,
    events: Receiver<WorkerEvent>,
    sender: SyncSender<WorkerEvent>,
}

impl App {
    fn new(started_in_tray: bool) -> Self {
        let (sender, events) = mpsc::sync_channel(96);
        Self {
            window: null_mut(),
            title: null_mut(),
            buttons: Vec::new(),
            file_label: null_mut(),
            editor: null_mut(),
            activity_label: null_mut(),
            activity: null_mut(),
            status: null_mut(),
            fonts: Vec::new(),
            docs: null_mut(),
            popup: Popup::new(),
            path: None,
            saved: String::new(),
            worker: None,
            generation: 0,
            started_in_tray,
            taskbar_created: 0,
            events,
            sender,
        }
    }

    fn initialize(&mut self) -> io::Result<()> {
        self.title = child(self.window, "STATIC", "LuaHotkeys", 0, 0, TITLE_LABEL)?;
        for (id, caption) in [
            (RUN, "Run"),
            (STOP, "Stop"),
            (NEW, "New"),
            (OPEN, "Open…"),
            (SAVE, "Save"),
            (HIDE, "Hide to tray"),
            (DOCS, "Docs"),
            (UPDATE, "Update"),
        ] {
            self.buttons.push(child(
                self.window,
                "BUTTON",
                caption,
                WS_TABSTOP | BS_OWNERDRAW as u32,
                0,
                id,
            )?);
        }
        self.file_label = child(
            self.window,
            "STATIC",
            "",
            SS_PATHELLIPSIS_STYLE,
            0,
            FILE_LABEL,
        )?;
        self.editor = child(
            self.window,
            "RICHEDIT50W",
            "",
            WS_TABSTOP
                | WS_VSCROLL
                | WS_HSCROLL
                | ES_MULTILINE as u32
                | ES_AUTOVSCROLL as u32
                | ES_AUTOHSCROLL as u32
                | ES_WANTRETURN as u32,
            0,
            EDITOR,
        )?;
        self.activity_label = child(self.window, "STATIC", "Activity", 0, 0, ACTIVITY_LABEL)?;
        self.activity = child(
            self.window,
            "RICHEDIT50W",
            "",
            WS_VSCROLL | ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32 | ES_READONLY as u32,
            0,
            0,
        )?;
        self.status = child(self.window, "STATIC", "", 0, 0, STATUS_LABEL)?;

        let title_font = font("Segoe UI", -30, 700);
        let editor_font = font("Consolas", -16, 400);
        let log_font = font("Consolas", -14, 400);
        for (control, font) in [
            (self.title, title_font),
            (self.editor, editor_font),
            (self.activity, log_font),
        ] {
            if !font.is_null() {
                unsafe { SendMessageW(control, WM_SETFONT, font as usize, 1) };
                self.fonts.push(font);
            }
        }
        unsafe {
            SendMessageW(
                self.editor,
                editor::EM_SETBKGNDCOLOR,
                0,
                theme::EDITOR as isize,
            );
            SendMessageW(
                self.activity,
                editor::EM_SETBKGNDCOLOR,
                0,
                theme::ACTIVITY as isize,
            );
            SendMessageW(self.editor, editor::EM_SETEVENTMASK, 0, 1);
            for control in [self.editor, self.activity] {
                SendMessageW(control, 0x00d3, 3, 10 | (10 << 16)); // EM_SETMARGINS
            }
            SendMessageW(self.editor, EM_EXLIMITTEXT, 0, i32::MAX as isize);
        }
        editor::default_color(self.editor);
        editor::default_color(self.activity);
        theme::scrollbars(self.editor);
        theme::scrollbars(self.activity);
        self.taskbar_created = unsafe { RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()) };
        self.set_script(include_str!("../example.lua"), None);
        self.append("Run the example, then press Ctrl+Alt+J in a text editor.");
        self.set_status("Stopped", false);
        self.tray(NIM_ADD);
        self.layout();
        Ok(())
    }

    fn layout(&self) {
        if self.editor.is_null() {
            return;
        }
        let mut client = RECT::default();
        unsafe { GetClientRect(self.window, &mut client) };
        let width = (client.right - 36).max(1);
        let height = client.bottom;
        let mut x = 18;
        place(self.title, x, 13, width, 37);
        for (&button, button_width) in self.buttons.iter().zip([82, 72, 65, 73, 67, 104, 70, 82]) {
            place(button, x, 61, button_width, 32);
            x += button_width + 8;
        }
        place(self.file_label, 18, 107, width, 22);
        let available = (height - 201).max(20);
        let editor_height = available * 72 / 100;
        let log_top = 134 + editor_height + 30;
        place(self.editor, 18, 134, width, editor_height);
        place(self.activity_label, 18, log_top - 24, width, 20);
        place(self.activity, 18, log_top, width, available - editor_height);
        place(self.status, 18, height - 27, width, 20);
    }

    fn set_script(&mut self, source: &str, path: Option<PathBuf>) {
        self.popup.dismiss();
        self.path = path;
        set_text(self.editor, source);
        self.saved = editor::text(self.editor);
        self.update_label();
        self.tray(NIM_MODIFY);
        editor::highlight(self.editor);
        unsafe { SendMessageW(self.editor, 0x00cd, 0, 0) }; // EM_EMPTYUNDOBUFFER
    }

    fn load(&mut self, path: &Path) -> io::Result<()> {
        let path = std::path::absolute(path)?;
        let bytes = fs::read(&path)?;
        let script = decode_script(&bytes)?;
        if self.stop_worker() {
            self.set_script(&script, Some(path));
        }
        Ok(())
    }

    fn file_name(&self) -> String {
        self.path.as_deref().and_then(Path::file_name).map_or_else(
            || "Untitled.lhk".into(),
            |name| name.to_string_lossy().into_owned(),
        )
    }

    fn update_label(&self) {
        let mut label = self.file_name();
        if editor::text(self.editor) != self.saved {
            label.push_str("  •  Unsaved");
        }
        set_text(self.file_label, &label);
    }

    fn save(&mut self) -> bool {
        let Some(path) = self.path.clone().or_else(|| choose_file(self.window, true)) else {
            return false;
        };
        let script = editor::text(self.editor);
        if let Err(error) = fs::write(&path, &script) {
            self.error("Could not save script", &error);
            return false;
        }
        self.path = Some(path);
        self.saved = script;
        self.update_label();
        self.tray(NIM_MODIFY);
        self.append(&format!("Saved {}", self.file_name()));
        true
    }

    fn can_discard(&mut self) -> bool {
        if editor::text(self.editor) == self.saved {
            return true;
        }
        if unsafe { IsWindowVisible(self.window) } == 0 {
            self.show();
        }
        match message(
            self.window,
            "Save your changes to the Lua script?",
            "LuaHotkeys",
            MB_YESNOCANCEL | MB_ICONQUESTION,
        ) {
            IDNO => true,
            IDYES => self.save(),
            _ => false,
        }
    }

    fn command(&mut self, id: u16) {
        self.popup.dismiss();
        match id {
            RUN => {
                if let Err(error) = self.start_worker() {
                    self.error("Could not run script", &error);
                }
            }
            STOP => {
                if self.worker.is_some() && self.stop_worker() {
                    self.append("Stopped.");
                }
            }
            NEW => {
                if self.can_discard() && self.stop_worker() {
                    self.set_script("", None);
                    unsafe { SetFocus(self.editor) };
                }
            }
            OPEN => {
                if self.can_discard() {
                    if let Some(path) = choose_file(self.window, false) {
                        if let Err(error) = self.load(&path) {
                            self.error("Could not open script", &error);
                        }
                    }
                }
            }
            SAVE => {
                self.save();
            }
            HIDE => {
                unsafe { ShowWindow(self.window, SW_HIDE) };
            }
            DOCS => match docs::open(self.window, self.docs) {
                Ok(window) => self.docs = window,
                Err(error) => self.error("Could not open Docs", &error),
            },
            UPDATE => {
                let launch = app_folder().and_then(|folder| {
                    Command::new(folder.join("LuaHotkeysUpdater.exe"))
                        .current_dir(&folder)
                        .creation_flags(CREATE_NO_WINDOW)
                        .spawn()
                });
                if let Err(error) = launch {
                    self.error("Could not start updater", &error);
                }
            }
            ASSOCIATE => {
                match std::env::current_exe().and_then(|executable| association::register(&executable)) {
                    Ok(()) => self.append("Double-click a script to run it. Right-click for Edit in LuaHotkeys or New → LuaHotkeys Script."),
                    Err(error) => self.error("Could not associate files", &error),
                }
            }
            SHOW_EDITOR => self.show(),
            EXIT => {
                unsafe { PostMessageW(self.window, WM_CLOSE, 0, 0) };
            }
            _ => {}
        }
    }

    fn completion_key(&mut self, key: u16, ctrl: bool, shift: bool) -> bool {
        if key == VK_SPACE && ctrl && !shift {
            let _ = self.popup.show(self.window, self.editor, true);
            return true;
        }
        if !self.popup.is_open() {
            return false;
        }
        match (key, ctrl, shift) {
            (VK_UP, false, false) => self.popup.move_selection(-1),
            (VK_DOWN, false, false) => self.popup.move_selection(1),
            (VK_RETURN | VK_TAB, false, false) => {
                if !self.popup.accept(self.editor) {
                    return false;
                }
                self.after_completion();
            }
            (VK_ESCAPE, _, _) => self.popup.dismiss(),
            (
                VK_UP | VK_DOWN | VK_LEFT | VK_RIGHT | VK_HOME | VK_END | VK_PRIOR | VK_NEXT,
                _,
                _,
            ) => {
                self.popup.dismiss();
                return false;
            }
            _ => return false,
        }
        true
    }

    fn after_completion(&self) {
        unsafe { SetTimer(self.window, COLOR_TIMER, 110, None) };
    }

    fn start_worker(&mut self) -> io::Result<()> {
        if !self.stop_worker() {
            return Ok(());
        }
        let folder = match self.path.as_deref().and_then(Path::parent) {
            Some(folder) => folder.to_path_buf(),
            None => app_folder()?,
        };
        let mut worker = Command::new(std::env::current_exe()?)
            .arg("--worker")
            .current_dir(folder)
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        let (Some(mut input), Some(output)) = (worker.stdin.take(), worker.stdout.take()) else {
            let _ = worker.kill();
            let _ = worker.wait();
            return Err(io::Error::other("Could not open the script process pipes."));
        };
        self.worker = Some(worker);
        self.generation += 1;
        let generation = self.generation;
        let name = self.file_name();
        let source = editor::text(self.editor);
        self.set_status("Starting…", true);
        self.append(&format!("Running {name}"));
        let sender = self.sender.clone();
        let window = self.window as usize;
        thread::spawn(move || {
            let notify = |line| {
                if sender.send(WorkerEvent { generation, line }).is_err() {
                    return false;
                }
                unsafe { PostMessageW(window as HWND, WORKER_MESSAGE, 0, 0) != 0 }
            };
            if writeln!(input, "{}", STANDARD.encode(name.as_bytes()))
                .and_then(|_| writeln!(input, "{}", STANDARD.encode(source.as_bytes())))
                .and_then(|_| input.flush())
                .is_ok()
            {
                for line in BufReader::new(output).lines().map_while(Result::ok) {
                    if !notify(Some(line)) {
                        break;
                    }
                }
            }
            drop(input);
            notify(None);
        });
        Ok(())
    }

    fn stop_worker(&mut self) -> bool {
        if let Some(mut child) = self.worker.take() {
            if let Err(error) = child.kill().and_then(|_| child.wait()) {
                if !matches!(child.try_wait(), Ok(Some(_))) {
                    self.append(&error.to_string());
                    self.worker = Some(child);
                    return false;
                }
            }
        }
        self.set_status("Stopped", false);
        true
    }

    fn receive_worker(&mut self) {
        let Ok(event) = self.events.try_recv() else {
            return;
        };
        if event.generation != self.generation || self.worker.is_none() {
            return;
        }
        let Some(line) = event.line else {
            let status = self.worker.take().and_then(|mut child| child.wait().ok());
            let success = status.is_some_and(|status| status.success());
            if !success {
                self.append("The script process exited with an error.");
            }
            self.set_status("Stopped", false);
            if self.started_in_tray && unsafe { IsWindowVisible(self.window) } == 0 {
                if success {
                    unsafe { PostMessageW(self.window, WM_CLOSE, 0, 0) };
                } else {
                    self.show();
                }
            }
            return;
        };
        if let Some((kind, content)) = line.split_once(' ') {
            if let Ok(content) = STANDARD.decode(content) {
                let text = String::from_utf8_lossy(&content);
                match kind {
                    "L" => self.append(&text),
                    "E" => self.append(&format!("Error  {text}")),
                    "R" => {
                        if let Ok(count) = text.parse::<usize>() {
                            match count {
                                0 => self.append("Finished. No global hotkeys were declared."),
                                1 => self.set_status("Running  •  1 hotkey", true),
                                _ => self.set_status(&format!("Running  •  {count} hotkeys"), true),
                            }
                        }
                    }
                    _ => self.append(&line),
                }
                return;
            }
        }
        self.append(&line);
    }

    fn set_status(&self, caption: &str, active: bool) {
        set_text(self.status, caption);
        if self.buttons.len() >= 2 {
            set_text(self.buttons[0], if active { "Restart" } else { "Run" });
            unsafe { EnableWindow(self.buttons[1], active as i32) };
        }
    }

    fn append(&self, text: &str) {
        let mut time = SYSTEMTIME::default();
        unsafe { GetLocalTime(&mut time) };
        let line = wide(format!(
            "{:02}:{:02}:{:02}  {}\r\n",
            time.wHour,
            time.wMinute,
            time.wSecond,
            text.trim_end()
        ));
        unsafe {
            if GetWindowTextLengthW(self.activity) > 48_000 {
                SendMessageW(self.activity, EM_SETSEL, 0, 24_000);
                SendMessageW(self.activity, EM_REPLACESEL, 0, wide("").as_ptr() as isize);
            }
            SendMessageW(self.activity, EM_SETSEL, usize::MAX, -1);
            SendMessageW(self.activity, EM_REPLACESEL, 0, line.as_ptr() as isize);
            SendMessageW(self.activity, EM_SCROLLCARET, 0, 0);
        }
    }

    fn error(&self, title: &str, error: &io::Error) {
        message(self.window, &error.to_string(), title, ERROR);
    }

    fn show(&self) {
        unsafe {
            ShowWindow(
                self.window,
                if IsIconic(self.window) != 0 {
                    SW_RESTORE
                } else {
                    SW_SHOW
                },
            );
            SetForegroundWindow(self.window);
            SetFocus(self.editor);
        }
    }

    fn tray(&self, operation: u32) {
        let mut tip = [0_u16; 128];
        for (slot, value) in tip[..63]
            .iter_mut()
            .zip(format!("LuaHotkeys · {}", self.file_name()).encode_utf16())
        {
            *slot = value;
        }
        let data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.window,
            uID: 1,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: TRAY_MESSAGE,
            hIcon: crate::common::app_icon(true),
            szTip: tip,
            ..Default::default()
        };
        unsafe { Shell_NotifyIconW(operation, &data) };
    }

    fn tray_menu(&mut self) {
        unsafe {
            let menu = CreatePopupMenu();
            if menu.is_null() {
                return;
            }
            AppendMenuW(
                menu,
                MF_STRING,
                SHOW_EDITOR as usize,
                wide("Edit script").as_ptr(),
            );
            AppendMenuW(
                menu,
                MF_STRING | if self.worker.is_some() { 0 } else { MF_GRAYED },
                STOP as usize,
                wide("Stop script").as_ptr(),
            );
            AppendMenuW(
                menu,
                MF_STRING,
                ASSOCIATE as usize,
                wide("Associate files").as_ptr(),
            );
            AppendMenuW(menu, MF_SEPARATOR, 0, null());
            AppendMenuW(menu, MF_STRING, EXIT as usize, wide("Exit").as_ptr());
            let mut position = windows_sys::Win32::Foundation::POINT::default();
            GetCursorPos(&mut position);
            SetForegroundWindow(self.window);
            let selected = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                position.x,
                position.y,
                0,
                self.window,
                null(),
            );
            DestroyMenu(menu);
            PostMessageW(self.window, WM_NULL, 0, 0);
            self.command(selected as u16);
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        if let Some(mut child) = self.worker.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        for &font in &self.fonts {
            unsafe { DeleteObject(font) };
        }
    }
}

const SS_PATHELLIPSIS_STYLE: u32 = 0x8000;

fn child(
    parent: HWND,
    class: &str,
    caption: &str,
    style: u32,
    extra: u32,
    id: u16,
) -> io::Result<HWND> {
    let window = unsafe {
        CreateWindowExW(
            extra,
            wide(class).as_ptr(),
            wide(caption).as_ptr(),
            WS_CHILD | WS_VISIBLE | style,
            0,
            0,
            0,
            0,
            parent,
            id as usize as HMENU,
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
        )
    };
    Ok(window)
}

fn set_text(window: HWND, value: &str) {
    unsafe { SetWindowTextW(window, wide(value).as_ptr()) };
}

fn place(window: HWND, x: i32, y: i32, width: i32, height: i32) {
    unsafe { MoveWindow(window, x, y, width.max(1), height.max(1), 1) };
}

fn font(family: &str, height: i32, weight: i32) -> *mut std::ffi::c_void {
    unsafe {
        CreateFontW(
            height,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            1,
            0,
            0,
            5,
            0,
            wide(family).as_ptr(),
        )
    }
}

fn decode_script(bytes: &[u8]) -> io::Result<String> {
    for (prefix, big_endian) in [([0xff, 0xfe, 0, 0], false), ([0, 0, 0xfe, 0xff], true)] {
        if let Some(bytes) = bytes.strip_prefix(&prefix) {
            if bytes.len() % 4 != 0 {
                return Err(io::Error::other("Invalid UTF-32 script."));
            }
            return bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|bytes| {
                    let bytes = *bytes;
                    let unit = if big_endian {
                        u32::from_be_bytes(bytes)
                    } else {
                        u32::from_le_bytes(bytes)
                    };
                    char::from_u32(unit).ok_or_else(|| io::Error::other("Invalid UTF-32 script."))
                })
                .collect();
        }
    }
    for (prefix, big_endian) in [([0xff, 0xfe], false), ([0xfe, 0xff], true)] {
        if let Some(bytes) = bytes.strip_prefix(&prefix) {
            if bytes.len() % 2 != 0 {
                return Err(io::Error::other("Invalid UTF-16 script."));
            }
            let units: Vec<_> = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|bytes| {
                    if big_endian {
                        u16::from_be_bytes([bytes[0], bytes[1]])
                    } else {
                        u16::from_le_bytes([bytes[0], bytes[1]])
                    }
                })
                .collect();
            return String::from_utf16(&units).map_err(io::Error::other);
        }
    }
    std::str::from_utf8(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes))
        .map(str::to_owned)
        .map_err(io::Error::other)
}

fn choose_file(owner: HWND, save: bool) -> Option<PathBuf> {
    let mut buffer = vec![0_u16; 32768];
    if save {
        buffer[..12].copy_from_slice(&wide("hotkeys.lhk"));
    }
    let open_filter = "LuaHotkeys and Lua scripts\0*.lhk;*.luahotkeys;*.lua\0All files\0*.*\0";
    let save_filter = "LuaHotkeys script (*.lhk)\0*.lhk\0LuaHotkeys script (*.luahotkeys)\0*.luahotkeys\0Lua script (*.lua)\0*.lua\0";
    let filters = wide(if save { save_filter } else { open_filter });
    let default = wide("lhk");
    let mut dialog = OPENFILENAMEW {
        lStructSize: size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner,
        lpstrFilter: filters.as_ptr(),
        nFilterIndex: 1,
        lpstrFile: buffer.as_mut_ptr(),
        nMaxFile: buffer.len() as u32,
        lpstrDefExt: if save { default.as_ptr() } else { null() },
        Flags: OFN_NOCHANGEDIR
            | OFN_PATHMUSTEXIST
            | if save {
                OFN_OVERWRITEPROMPT
            } else {
                OFN_FILEMUSTEXIST
            },
        ..Default::default()
    };
    let accepted = unsafe {
        if save {
            GetSaveFileNameW(&mut dialog)
        } else {
            GetOpenFileNameW(&mut dialog)
        }
    };
    (accepted != 0).then(|| {
        PathBuf::from(OsString::from_wide(
            &buffer[..buffer
                .iter()
                .position(|&unit| unit == 0)
                .unwrap_or(buffer.len())],
        ))
    })
}

unsafe extern "system" fn window_proc(hwnd: HWND, id: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if id == WM_NCCREATE {
        let create = unsafe { &*(l as *const CREATESTRUCTW) };
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as _) };
    }
    if id == WM_GETMINMAXINFO {
        let limits = unsafe { &mut *(l as *mut MINMAXINFO) };
        limits.ptMinTrackSize.x = 755;
        limits.ptMinTrackSize.y = 475;
        return 0;
    }
    match id {
        WM_ERASEBKGND => {
            theme::paint_background(hwnd, w as HDC);
            return 1;
        }
        WM_CTLCOLORSTATIC => match unsafe { GetDlgCtrlID(l as HWND) } as u16 {
            FILE_LABEL | STATUS_LABEL => {
                return theme::color_control(w as HDC, theme::MUTED, theme::BACKGROUND)
            }
            TITLE_LABEL | ACTIVITY_LABEL => {
                return theme::color_control(w as HDC, theme::TEXT, theme::BACKGROUND)
            }
            _ => {}
        },
        WM_CTLCOLORLISTBOX
            if unsafe { GetDlgCtrlID(l as HWND) } as u16 == crate::suggestions::ID =>
        {
            return theme::color_control(w as HDC, theme::TEXT, theme::ACTIVITY);
        }
        WM_DRAWITEM => {
            let item = unsafe { &*(l as *const DRAWITEMSTRUCT) };
            if item.CtlType == ODT_BUTTON {
                theme::button(item, item.CtlID == u32::from(RUN));
                return 1;
            }
        }
        _ => {}
    }
    let state = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<App> };
    if !state.is_null() {
        if let Ok(mut app) = unsafe { &*state }.try_borrow_mut() {
            if id == app.taskbar_created && id != 0 {
                app.tray(NIM_ADD);
                return 0;
            }
            match id {
                WM_CREATE => {
                    app.window = hwnd;
                    return if app.initialize().is_ok() { 0 } else { -1 };
                }
                WM_DRAWITEM => {
                    let item = unsafe { &*(l as *const DRAWITEMSTRUCT) };
                    if item.CtlID == u32::from(crate::suggestions::ID) {
                        app.popup.paint(item);
                        return 1;
                    }
                }
                WM_SIZE => {
                    app.popup.dismiss();
                    app.layout();
                    return 0;
                }
                WM_COMMAND => {
                    if w as u16 == EDITOR {
                        match (w >> 16) as u16 {
                            0x0300 => unsafe {
                                // EN_CHANGE
                                SetTimer(hwnd, COLOR_TIMER, 110, None);
                                PostMessageW(hwnd, SUGGEST, 0, 0);
                            },
                            0x0200 => app.popup.dismiss(), // EN_KILLFOCUS
                            _ => {}
                        }
                    } else {
                        app.command(w as u16);
                    }
                    return 0;
                }
                SUGGEST => {
                    let editor = app.editor;
                    let _ = app.popup.show(hwnd, editor, false);
                    return 0;
                }
                WM_TIMER if w == COLOR_TIMER => {
                    unsafe { KillTimer(hwnd, COLOR_TIMER) };
                    app.update_label();
                    editor::highlight(app.editor);
                    return 0;
                }
                WM_CLOSE | UPDATE_CLOSE => {
                    let close = app.can_discard() && app.stop_worker();
                    drop(app);
                    if close {
                        unsafe { DestroyWindow(hwnd) };
                    }
                    return 0;
                }
                WM_DESTROY => {
                    app.tray(NIM_DELETE);
                    unsafe { PostQuitMessage(0) };
                    return 0;
                }
                WORKER_MESSAGE => {
                    app.receive_worker();
                    return 0;
                }
                TRAY_MESSAGE => {
                    if l as u32 == WM_RBUTTONUP {
                        app.tray_menu();
                    }
                    if l as u32 == WM_LBUTTONDBLCLK {
                        app.show();
                    }
                    return 0;
                }
                _ => {}
            }
        }
    }
    unsafe { DefWindowProcW(hwnd, id, w, l) }
}

fn listen_for_updates(window: HWND) {
    let name = close_event(std::process::id());
    let event = unsafe { CreateEventW(null(), 0, 0, name.as_ptr()) } as usize;
    if event == 0 {
        return;
    }
    let window = window as usize;
    thread::spawn(move || {
        let event = Handle(event as _);
        while unsafe { WaitForSingleObject(event.0, INFINITE) } == 0 {
            if unsafe { PostMessageW(window as HWND, UPDATE_CLOSE, 0, 0) } == 0 {
                break;
            }
        }
    });
}

pub fn run(file: Option<PathBuf>, start_hidden: bool) -> io::Result<()> {
    let _ole = Ole::start()?;
    if unsafe { LoadLibraryW(wide("msftedit.dll").as_ptr()) }.is_null() {
        return Err(io::Error::last_os_error());
    }
    let class_name = wide("LuaHotkeysWindow");
    let instance = unsafe { GetModuleHandleW(null()) };
    let class = WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        hInstance: instance,
        lpszClassName: class_name.as_ptr(),
        hCursor: unsafe { LoadCursorW(null_mut(), IDC_ARROW) },
        hIcon: crate::common::app_icon(false),

        ..Default::default()
    };
    if unsafe { RegisterClassW(&class) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let app = Box::new(RefCell::new(App::new(start_hidden)));
    let window = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            wide("LuaHotkeys").as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            (GetSystemMetrics(SM_CXSCREEN) - 880).max(0) / 2,
            (GetSystemMetrics(SM_CYSCREEN) - 640).max(0) / 2,
            880,
            640,
            null_mut(),
            null_mut(),
            instance,
            (&*app as *const RefCell<App>).cast(),
        )
    };
    if window.is_null() {
        return Err(io::Error::last_os_error());
    }
    unsafe {
        SendMessageW(
            window,
            WM_SETICON,
            ICON_SMALL as usize,
            crate::common::app_icon(true) as isize,
        )
    };
    theme::title_bar(window);
    let initial = (|| {
        let mut app = app.borrow_mut();
        if let Some(file) = file {
            app.load(&file)?;
        }
        if start_hidden {
            app.start_worker()?;
        } else {
            app.show();
        }
        Ok::<_, io::Error>(())
    })();
    if let Err(error) = initial {
        unsafe { DestroyWindow(window) };
        return Err(error);
    }
    listen_for_updates(window);

    let mut msg = MSG::default();
    loop {
        match unsafe { GetMessageW(&mut msg, null_mut(), 0, 0) } {
            -1 => {
                unsafe { DestroyWindow(window) };
                return Err(io::Error::last_os_error());
            }
            0 => return Ok(()),
            _ => {
                if matches!(msg.message, WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MOUSEWHEEL) {
                    let mut state = app.borrow_mut();
                    if state.popup.is_window(msg.hwnd) {
                        if msg.message == WM_LBUTTONDOWN {
                            let editor = state.editor;
                            if state.popup.click(editor, msg.lParam) {
                                state.after_completion();
                            }
                            continue;
                        }
                    } else {
                        state.popup.dismiss();
                    }
                }
                if msg.message == WM_KEYDOWN
                    && (msg.hwnd == window || unsafe { IsChild(window, msg.hwnd) } != 0)
                {
                    let ctrl = unsafe { GetKeyState(i32::from(VK_CONTROL)) } < 0;
                    let shift = unsafe { GetKeyState(i32::from(VK_SHIFT)) } < 0;
                    {
                        let mut state = app.borrow_mut();
                        if msg.hwnd == state.editor
                            && state.completion_key(msg.wParam as u16, ctrl, shift)
                        {
                            continue;
                        }
                    }
                    let action = match (msg.wParam as u16, ctrl, shift) {
                        (VK_F5, false, false) => RUN,
                        (VK_F5, false, true) => STOP,
                        (0x4e, true, false) => NEW,
                        (0x4f, true, false) => OPEN,
                        (0x53, true, false) => SAVE,
                        _ => 0,
                    };
                    if action != 0 {
                        app.borrow_mut().command(action);
                        continue;
                    }
                }
                unsafe {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
    }
}

struct Ole;
impl Ole {
    fn start() -> io::Result<Self> {
        if unsafe { OleInitialize(null()) } < 0 {
            return Err(io::Error::other(
                "Could not initialize Windows text controls.",
            ));
        }
        Ok(Self)
    }
}
impl Drop for Ole {
    fn drop(&mut self) {
        unsafe { OleUninitialize() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Graphics::Gdi::{GetBkColor, GetDC, GetTextColor, ReleaseDC};

    #[test]
    fn editor_toolbar_theme_and_completion_work_together() {
        let _ole = Ole::start().unwrap();
        let state = Box::new(RefCell::new(App::new(false)));
        let name = wide("LuaHotkeysEditorTest");
        let owner = unsafe {
            assert!(!LoadLibraryW(wide("msftedit.dll").as_ptr()).is_null());
            let instance = GetModuleHandleW(null());
            let class = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance,
                lpszClassName: name.as_ptr(),
                ..Default::default()
            };
            assert_ne!(RegisterClassW(&class), 0);
            CreateWindowExW(
                0,
                name.as_ptr(),
                wide("Editor test").as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
                0,
                0,
                880,
                640,
                null_mut(),
                null_mut(),
                instance,
                (&*state as *const RefCell<App>).cast(),
            )
        };
        assert!(!owner.is_null());
        struct TestWindow(HWND);
        impl Drop for TestWindow {
            fn drop(&mut self) {
                unsafe { DestroyWindow(self.0) };
            }
        }
        let _window = TestWindow(owner);

        unsafe {
            assert!(!GetDlgItem(owner, i32::from(DOCS)).is_null());
            assert!(!GetDlgItem(owner, i32::from(UPDATE)).is_null());
            assert!(GetDlgItem(owner, i32::from(ASSOCIATE)).is_null());
            ShowWindow(owner, SW_SHOW);
        }
        let editor = state.borrow().editor;
        unsafe {
            SetFocus(editor);
            let dc = GetDC(owner);
            assert_ne!(
                SendMessageW(
                    owner,
                    WM_CTLCOLORSTATIC,
                    dc as usize,
                    state.borrow().title as isize
                ),
                0
            );
            assert_eq!(GetTextColor(dc), theme::TEXT);
            assert_eq!(GetBkColor(dc), theme::BACKGROUND);
            ReleaseDC(owner, dc);
        }

        set_text(editor, "");
        unsafe {
            for character in "hotkey".encode_utf16() {
                SendMessageW(editor, WM_CHAR, character as usize, 1);
            }
            let mut message = MSG::default();
            while PeekMessageW(&mut message, owner, SUGGEST, SUGGEST, PM_REMOVE) != 0 {
                DispatchMessageW(&message);
            }
        }
        assert_eq!(editor::text(editor), "hotkey");
        let mut app = state.borrow_mut();
        assert!(app.popup.is_open());
        assert!(app.completion_key(VK_TAB, false, false));
        assert_eq!(editor::text(editor), "hotkey()");
        assert_eq!(editor::selection(editor), (7, 7));
        unsafe { SendMessageW(editor, 0x00c7, 0, 0) }; // EM_UNDO
        assert_eq!(editor::text(editor), "hotkey");
        editor::replace(editor, 6..6, "", 6);
        assert!(app.completion_key(VK_SPACE, true, false));
        assert!(app.popup.is_open());
        assert!(app.completion_key(VK_ESCAPE, false, false));
        assert!(!app.popup.is_open());
        drop(app);

        unsafe { SendMessageW(owner, WM_COMMAND, DOCS as usize, 0) };
        let docs = state.borrow().docs;
        assert!(!docs.is_null());
        let reference = unsafe { GetWindow(docs, GW_CHILD) };
        assert!(editor::text(reference).contains("hotkey(combo, callback)"));
    }
}
