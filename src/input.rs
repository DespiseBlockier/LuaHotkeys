use std::{
    cell::RefCell,
    io,
    mem::size_of,
    ptr::{null, null_mut},
    thread,
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::{LPARAM, LRESULT, POINT, WPARAM},
    System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
    UI::{
        Input::KeyboardAndMouse::*,
        WindowsAndMessaging::{
            CallNextHookEx, GetCursorPos, GetForegroundWindow, GetWindowThreadProcessId,
            PostThreadMessageW, SetCursorPos, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK,
            KBDLLHOOKSTRUCT, LLKHF_INJECTED, WH_KEYBOARD_LL, WM_HOTKEY, WM_KEYDOWN, WM_KEYUP,
            WM_SYSKEYDOWN, WM_SYSKEYUP,
        },
    },
};

use crate::{
    runtime::HotkeyTrigger,
    shortcut::{
        Combination, Key, ModifierKey, MouseButton, NumpadOperator, ALT, CONTROL, NAMED_KEYS,
        SHIFT, SUPER,
    },
};

fn virtual_key(key: Key) -> Option<u16> {
    Some(match key {
        Key::Letter(letter) => u16::from(letter.to_ascii_uppercase()),
        Key::Digit(digit) => u16::from(digit),
        Key::Function(number) => VK_F1 + number - 1,
        Key::Numpad(number) => VK_NUMPAD0 + number,
        Key::Symbol(_) | Key::Mouse(_) => return None,
        Key::Modifier(key) => match key {
            ModifierKey::LeftCtrl => VK_LCONTROL,
            ModifierKey::RightCtrl => VK_RCONTROL,
            ModifierKey::LeftAlt => VK_LMENU,
            ModifierKey::RightAlt => VK_RMENU,
            ModifierKey::LeftShift => VK_LSHIFT,
            ModifierKey::RightShift => VK_RSHIFT,
            ModifierKey::LeftWin => VK_LWIN,
            ModifierKey::RightWin => VK_RWIN,
        },
        Key::NumpadOperator(key) => match key {
            NumpadOperator::Divide => VK_DIVIDE,
            NumpadOperator::Multiply => VK_MULTIPLY,
            NumpadOperator::Minus => VK_SUBTRACT,
            NumpadOperator::Plus => VK_ADD,
            NumpadOperator::Enter => VK_RETURN,
            NumpadOperator::Decimal => VK_DECIMAL,
        },
        Key::Enter => VK_RETURN,
        Key::Escape => VK_ESCAPE,
        Key::Space => VK_SPACE,
        Key::Tab => VK_TAB,
        Key::Backspace => VK_BACK,
        Key::Delete => VK_DELETE,
        Key::Insert => VK_INSERT,
        Key::CapsLock => VK_CAPITAL,
        Key::NumLock => VK_NUMLOCK,
        Key::ScrollLock => VK_SCROLL,
        Key::Pause => VK_PAUSE,
        Key::Menu => VK_APPS,
        Key::Home => VK_HOME,
        Key::End => VK_END,
        Key::PageUp => VK_PRIOR,
        Key::PageDown => VK_NEXT,
        Key::Up => VK_UP,
        Key::Down => VK_DOWN,
        Key::Left => VK_LEFT,
        Key::Right => VK_RIGHT,
        Key::PrintScreen => VK_SNAPSHOT,
        Key::VolumeUp => VK_VOLUME_UP,
        Key::VolumeDown => VK_VOLUME_DOWN,
        Key::VolumeMute => VK_VOLUME_MUTE,
        Key::MediaPlayPause => VK_MEDIA_PLAY_PAUSE,
    })
}

fn keyboard_layout() -> HKL {
    unsafe { GetKeyboardLayout(GetWindowThreadProcessId(GetForegroundWindow(), null_mut())) }
}

fn resolve(mut combo: Combination) -> io::Result<(u16, Combination)> {
    if let Key::Symbol(symbol) = combo.key {
        let mapped = unsafe { VkKeyScanExW(u16::from(symbol), keyboard_layout()) };
        if mapped < 0 || mapped & !0x07ff != 0 {
            return Err(io::Error::other(format!(
                "'{}' has no single-key shortcut on the current keyboard layout.",
                char::from(symbol)
            )));
        }
        for (mask, flag) in [(0x100, SHIFT), (0x200, CONTROL), (0x400, ALT)] {
            if mapped & mask != 0 {
                combo.modifiers |= flag;
            }
        }
        return Ok(((mapped as u16) & 0xff, combo));
    }
    let key = virtual_key(combo.key).ok_or_else(|| io::Error::other("Expected a keyboard key."))?;
    Ok((key, combo))
}

pub fn capture_key(code: u16, released: bool, extended: bool) -> Option<Combination> {
    if released && !matches!(code, VK_LWIN | VK_RWIN | VK_SNAPSHOT) {
        return None;
    }
    let key = match code {
        0x41..=0x5a => Key::Letter((code as u8).to_ascii_lowercase()),
        0x30..=0x39 => Key::Digit(code as u8),
        VK_F1..=VK_F24 => Key::Function(code - VK_F1 + 1),
        VK_NUMPAD0..=VK_NUMPAD9 => Key::Numpad(code - VK_NUMPAD0),
        VK_RETURN if extended => Key::NumpadOperator(NumpadOperator::Enter),
        _ => NAMED_KEYS
            .iter()
            .flat_map(|(_, keys)| *keys)
            .find_map(|&(_, key)| (virtual_key(key) == Some(code)).then_some(key))
            .or_else(|| {
                let symbol = unsafe {
                    MapVirtualKeyExW(u32::from(code), MAPVK_VK_TO_CHAR, keyboard_layout())
                };
                let symbol = u8::try_from(symbol).ok()?;
                symbol.is_ascii_punctuation().then_some(Key::Symbol(symbol))
            })?,
    };
    if matches!(key, Key::Modifier(_)) && !released {
        return None;
    }
    let mut modifiers = [
        (CONTROL, VK_CONTROL),
        (ALT, VK_MENU),
        (SHIFT, VK_SHIFT),
        (SUPER, VK_LWIN),
        (SUPER, VK_RWIN),
    ]
    .into_iter()
    .fold(0, |flags, (flag, key)| {
        if unsafe { GetKeyState(i32::from(key)) } < 0 {
            flags | flag
        } else {
            flags
        }
    });
    if let Key::Modifier(key) = key {
        modifiers &= !key.flag();
    }
    Some(Combination { modifiers, key })
}

fn modifiers(combo: Combination) -> impl DoubleEndedIterator<Item = u16> {
    [
        (MOD_CONTROL, VK_CONTROL),
        (MOD_ALT, VK_MENU),
        (MOD_SHIFT, VK_SHIFT),
        (MOD_WIN, VK_LWIN),
    ]
    .into_iter()
    .filter_map(move |(flag, key)| (combo.modifiers & flag != 0).then_some(key))
}

fn is_down(key: u16) -> bool {
    unsafe {
        GetAsyncKeyState(i32::from(key)) < 0
            || (key == VK_LWIN && GetAsyncKeyState(i32::from(VK_RWIN)) < 0)
    }
}

pub fn wait_for_release(combo: Combination) {
    let Ok((key, combo)) = resolve(combo) else {
        return;
    };
    while modifiers(combo).chain([key]).any(is_down) {
        thread::sleep(Duration::from_millis(10));
    }
}

pub fn register(id: i32, combo: Combination, trigger: HotkeyTrigger) -> io::Result<()> {
    let (code, combo) = resolve(combo)?;
    if let Key::Modifier(key) = combo.key {
        return register_modifier(id, code, key, combo.modifiers, trigger);
    }
    if unsafe {
        RegisterHotKey(
            null_mut(),
            id,
            combo.modifiers | MOD_NOREPEAT,
            u32::from(code),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub fn unregister(id: i32) {
    if MODIFIER_HOTKEYS.with(|state| {
        let mut state = state.borrow_mut();
        let before = state.bindings.len();
        state.bindings.retain(|binding| binding.id != id);
        if before == state.bindings.len() {
            return false;
        }
        if state.bindings.is_empty() {
            unsafe { UnhookWindowsHookEx(state.hook) };
            state.hook = null_mut();
        }
        true
    }) {
        return;
    }
    unsafe { UnregisterHotKey(null_mut(), id) };
}

#[derive(Default)]
struct ModifierHotkeys {
    hook: HHOOK,
    bindings: Vec<ModifierBinding>,
}

struct ModifierBinding {
    id: i32,
    code: u16,
    flag: u32,
    modifiers: u32,
    down: bool,
    pending: bool,
    trigger: HotkeyTrigger,
}

thread_local! {
    static MODIFIER_HOTKEYS: RefCell<ModifierHotkeys> = RefCell::new(ModifierHotkeys::default());
}

fn register_modifier(
    id: i32,
    code: u16,
    key: ModifierKey,
    modifiers: u32,
    trigger: HotkeyTrigger,
) -> io::Result<()> {
    if modifiers & key.flag() != 0 {
        return Err(io::Error::other(
            "The main key already supplies that modifier.",
        ));
    }
    MODIFIER_HOTKEYS.with(|state| {
        let mut state = state.borrow_mut();
        if state.hook.is_null() {
            state.hook = unsafe {
                SetWindowsHookExW(
                    WH_KEYBOARD_LL,
                    Some(modifier_hook),
                    GetModuleHandleW(null()),
                    0,
                )
            };
            if state.hook.is_null() {
                return Err(io::Error::last_os_error());
            }
        }
        state.bindings.push(ModifierBinding {
            id,
            code,
            flag: key.flag(),
            modifiers,
            down: false,
            pending: false,
            trigger,
        });
        Ok(())
    })
}

unsafe extern "system" fn modifier_hook(action: i32, message: WPARAM, data: LPARAM) -> LRESULT {
    if action == 0 {
        let event = &*(data as *const KBDLLHOOKSTRUCT);
        if event.flags & LLKHF_INJECTED == 0 {
            let code = event.vkCode as u16;
            let message = message as u32;
            MODIFIER_HOTKEYS.with(|state| {
                let mut state = state.borrow_mut();
                for binding in &mut state.bindings {
                    match message {
                        WM_KEYDOWN | WM_SYSKEYDOWN if code != binding.code => {
                            binding.pending = false
                        }
                        WM_KEYDOWN | WM_SYSKEYDOWN if !binding.down => {
                            binding.down = true;
                            let held = [
                                (CONTROL, VK_CONTROL),
                                (ALT, VK_MENU),
                                (SHIFT, VK_SHIFT),
                                (SUPER, VK_LWIN),
                            ]
                            .into_iter()
                            .filter(|&(_, key)| is_down(key))
                            .fold(0, |mask, (flag, _)| mask | flag);
                            if held & !binding.flag == binding.modifiers {
                                if binding.trigger == HotkeyTrigger::Press {
                                    post_hotkey(binding.id);
                                } else {
                                    binding.pending = true;
                                }
                            }
                        }
                        WM_KEYUP | WM_SYSKEYUP if code == binding.code => {
                            binding.down = false;
                            if std::mem::take(&mut binding.pending) {
                                post_hotkey(binding.id);
                            }
                        }
                        _ => {}
                    }
                }
            });
        }
    }
    CallNextHookEx(null_mut(), action, message, data)
}

unsafe fn post_hotkey(id: i32) {
    PostThreadMessageW(GetCurrentThreadId(), WM_HOTKEY, id as usize, 0);
}

fn keyboard(key: u16, scan: u16, mut flags: u32) -> INPUT {
    if matches!(
        key,
        VK_PRIOR
            | VK_NEXT
            | VK_END
            | VK_HOME
            | VK_LEFT
            | VK_UP
            | VK_RIGHT
            | VK_DOWN
            | VK_SNAPSHOT
            | VK_INSERT
            | VK_DELETE
            | VK_LWIN
            | VK_RWIN
            | VK_RCONTROL
            | VK_RMENU
            | VK_APPS
            | VK_DIVIDE
            | VK_NUMLOCK
            | VK_VOLUME_UP
            | VK_VOLUME_DOWN
            | VK_VOLUME_MUTE
            | VK_MEDIA_PLAY_PAUSE
    ) {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn send(events: &[INPUT]) -> io::Result<()> {
    if !events.is_empty()
        && unsafe {
            SendInput(
                events.len() as u32,
                events.as_ptr(),
                size_of::<INPUT>() as i32,
            )
        } != events.len() as u32
    {
        return Err(io::Error::other(
            "Windows rejected the input. Check the focused window and its privilege level.",
        ));
    }
    Ok(())
}

pub fn press(combo: Combination) -> io::Result<()> {
    let (combo, action) = if let Key::Mouse(button) = combo.key {
        (combo, mouse_click(button))
    } else {
        let (key, combo) = resolve(combo)?;
        let flags = keypad_enter_flag(combo.key);
        (
            combo,
            [
                keyboard(key, 0, flags),
                keyboard(key, 0, flags | KEYEVENTF_KEYUP),
            ],
        )
    };
    let held: Vec<_> = modifiers(combo).filter(|key| !is_down(*key)).collect();
    let mut events = Vec::with_capacity(2 + held.len() * 2);
    events.extend(held.iter().map(|&key| keyboard(key, 0, 0)));
    events.extend(action);
    events.extend(
        held.iter()
            .rev()
            .map(|&key| keyboard(key, 0, KEYEVENTF_KEYUP)),
    );
    send(&events)
}

fn keypad_enter_flag(key: Key) -> u32 {
    if key == Key::NumpadOperator(NumpadOperator::Enter) {
        KEYEVENTF_EXTENDEDKEY
    } else {
        0
    }
}

pub fn type_text(text: &str) -> io::Result<()> {
    let mut units = text.encode_utf16().peekable();
    let mut events = Vec::with_capacity(text.len() * 2);
    while let Some(unit) = units.next() {
        if unit == 13 && units.peek() == Some(&10) {
            continue;
        }
        let (key, scan, flags) = if unit == 13 || unit == 10 {
            (VK_RETURN, 0, 0)
        } else {
            (0, unit, KEYEVENTF_UNICODE)
        };
        events.extend([
            keyboard(key, scan, flags),
            keyboard(key, scan, flags | KEYEVENTF_KEYUP),
        ]);
    }
    send(&events)
}

fn mouse_click(button: MouseButton) -> [INPUT; 2] {
    let (down, up) = match button {
        MouseButton::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
        MouseButton::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
        MouseButton::Middle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
    };
    let mouse = |flags| INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    [mouse(down), mouse(up)]
}

pub fn move_mouse(x: i32, y: i32) -> io::Result<()> {
    if unsafe { SetCursorPos(x, y) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub fn mouse_pos() -> io::Result<(i32, i32)> {
    let mut point = POINT::default();
    if unsafe { GetCursorPos(&mut point) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((point.x, point.y))
}
