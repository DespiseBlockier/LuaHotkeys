use std::{io, mem::size_of, ptr::null_mut, thread, time::Duration};
use windows_sys::Win32::{
    Foundation::POINT,
    UI::{
        Input::KeyboardAndMouse::*,
        WindowsAndMessaging::{GetCursorPos, SetCursorPos},
    },
};

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct Combination {
    pub modifiers: u32,
    pub key: u16,
}

pub const NAMED_KEYS: &[(&str, &[(&str, u16)])] = &[
    (
        "Typing and editing",
        &[
            ("Enter", VK_RETURN),
            ("Esc", VK_ESCAPE),
            ("Escape", VK_ESCAPE),
            ("Space", VK_SPACE),
            ("Tab", VK_TAB),
            ("Backspace", VK_BACK),
            ("Delete", VK_DELETE),
            ("Insert", VK_INSERT),
            ("CapsLock", VK_CAPITAL),
        ],
    ),
    (
        "Navigation",
        &[
            ("Home", VK_HOME),
            ("End", VK_END),
            ("PageUp", VK_PRIOR),
            ("PageDown", VK_NEXT),
            ("Up", VK_UP),
            ("Down", VK_DOWN),
            ("Left", VK_LEFT),
            ("Right", VK_RIGHT),
        ],
    ),
    (
        "System and media",
        &[
            ("PrintScreen", VK_SNAPSHOT),
            ("VolumeUp", VK_VOLUME_UP),
            ("VolumeDown", VK_VOLUME_DOWN),
            ("VolumeMute", VK_VOLUME_MUTE),
            ("MediaPlayPause", VK_MEDIA_PLAY_PAUSE),
        ],
    ),
];

pub const MODIFIERS: &[(&str, &[&str], u32)] = &[
    ("Ctrl", &["Control"], MOD_CONTROL),
    ("Alt", &[], MOD_ALT),
    ("Shift", &[], MOD_SHIFT),
    ("Win", &["Windows"], MOD_WIN),
];

pub fn modifier_flag(part: &str) -> Option<u32> {
    MODIFIERS.iter().find_map(|&(name, aliases, flag)| {
        (name.eq_ignore_ascii_case(part)
            || aliases.iter().any(|alias| alias.eq_ignore_ascii_case(part)))
        .then_some(flag)
    })
}

pub fn parse(text: &str) -> Result<Combination, String> {
    let mut parts = text.split('+').map(str::trim).peekable();
    let mut modifiers = 0;
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            return key_code(part)
                .map(|key| Combination { modifiers, key })
                .ok_or_else(|| format!("Unknown key in '{text}'."));
        }
        let flag = modifier_flag(part).unwrap_or(0);
        if flag == 0 || modifiers & flag != 0 {
            return Err(format!(
                "Invalid modifier in '{text}'. Use Ctrl, Alt, Shift or Win."
            ));
        }
        modifiers |= flag;
    }
    Err(format!("Unknown key in '{text}'."))
}

fn key_code(name: &str) -> Option<u16> {
    let name = name.to_ascii_lowercase();
    let bytes = name.as_bytes();
    if bytes.len() == 1 && bytes[0].is_ascii_alphanumeric() {
        return Some(u16::from(bytes[0].to_ascii_uppercase()));
    }
    if let Some(number) = name
        .strip_prefix("numpad")
        .and_then(|part| part.parse::<u16>().ok())
    {
        return (number <= 9).then(|| VK_NUMPAD0 + number);
    }
    if let Some(number) = name
        .strip_prefix('f')
        .and_then(|part| part.parse::<u16>().ok())
    {
        return (1..=24).contains(&number).then(|| VK_F1 + number - 1);
    }
    NAMED_KEYS
        .iter()
        .flat_map(|(_, keys)| keys.iter())
        .find_map(|&(key, code)| key.eq_ignore_ascii_case(&name).then_some(code))
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
    while modifiers(combo).chain([combo.key]).any(is_down) {
        thread::sleep(Duration::from_millis(10));
    }
}

pub fn register(id: i32, combo: Combination) -> io::Result<()> {
    if unsafe {
        RegisterHotKey(
            null_mut(),
            id,
            combo.modifiers | MOD_NOREPEAT,
            u32::from(combo.key),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub fn unregister(id: i32) {
    unsafe { UnregisterHotKey(null_mut(), id) };
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
    let held: Vec<_> = modifiers(combo).filter(|key| !is_down(*key)).collect();
    let mut events = Vec::with_capacity(2 + held.len() * 2);
    events.extend(held.iter().map(|&key| keyboard(key, 0, 0)));
    events.extend([
        keyboard(combo.key, 0, 0),
        keyboard(combo.key, 0, KEYEVENTF_KEYUP),
    ]);
    events.extend(
        held.iter()
            .rev()
            .map(|&key| keyboard(key, 0, KEYEVENTF_KEYUP)),
    );
    send(&events)
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

pub fn click(button: &str) -> io::Result<()> {
    let down = match button.to_ascii_lowercase().as_str() {
        "left" => MOUSEEVENTF_LEFTDOWN,
        "right" => MOUSEEVENTF_RIGHTDOWN,
        "middle" => MOUSEEVENTF_MIDDLEDOWN,
        _ => {
            return Err(io::Error::other(
                "Mouse button must be left, right or middle.",
            ))
        }
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
    send(&[mouse(down), mouse(down << 1)])
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
