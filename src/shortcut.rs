#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Key {
    Letter(u8),
    Digit(u8),
    Function(u16),
    Numpad(u16),
    Enter,
    Escape,
    Space,
    Tab,
    Backspace,
    Delete,
    Insert,
    CapsLock,
    Home,
    End,
    PageUp,
    PageDown,
    Up,
    Down,
    Left,
    Right,
    PrintScreen,
    VolumeUp,
    VolumeDown,
    VolumeMute,
    MediaPlayPause,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Combination {
    pub modifiers: u32,
    pub key: Key,
}

pub type KeyGroup = (&'static str, &'static [(&'static str, Key)]);

pub const NAMED_KEYS: &[KeyGroup] = &[
    (
        "Typing and editing",
        &[
            ("Enter", Key::Enter),
            ("Esc", Key::Escape),
            ("Escape", Key::Escape),
            ("Space", Key::Space),
            ("Tab", Key::Tab),
            ("Backspace", Key::Backspace),
            ("Delete", Key::Delete),
            ("Insert", Key::Insert),
            ("CapsLock", Key::CapsLock),
        ],
    ),
    (
        "Navigation",
        &[
            ("Home", Key::Home),
            ("End", Key::End),
            ("PageUp", Key::PageUp),
            ("PageDown", Key::PageDown),
            ("Up", Key::Up),
            ("Down", Key::Down),
            ("Left", Key::Left),
            ("Right", Key::Right),
        ],
    ),
    (
        "System and media",
        &[
            ("PrintScreen", Key::PrintScreen),
            ("VolumeUp", Key::VolumeUp),
            ("VolumeDown", Key::VolumeDown),
            ("VolumeMute", Key::VolumeMute),
            ("MediaPlayPause", Key::MediaPlayPause),
        ],
    ),
];

// Keep the modifier bits stable for the Win32 RegisterHotKey interface.
pub const ALT: u32 = 1;
pub const CONTROL: u32 = 2;
pub const SHIFT: u32 = 4;
pub const SUPER: u32 = 8;

pub const MODIFIERS: &[(&str, &[&str], u32)] = &[
    ("Ctrl", &["Control"], CONTROL),
    ("Alt", &[], ALT),
    ("Shift", &[], SHIFT),
    ("Win", &["Windows"], SUPER),
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
            return key(part)
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

fn key(name: &str) -> Option<Key> {
    let name = name.to_ascii_lowercase();
    match name.as_bytes() {
        [letter @ b'a'..=b'z'] => return Some(Key::Letter(*letter)),
        [digit @ b'0'..=b'9'] => return Some(Key::Digit(*digit)),
        _ => {}
    }
    if let Some(number) = name
        .strip_prefix("numpad")
        .and_then(|part| part.parse::<u16>().ok())
    {
        return (number <= 9).then_some(Key::Numpad(number));
    }
    if let Some(number) = name
        .strip_prefix('f')
        .and_then(|part| part.parse::<u16>().ok())
    {
        return (1..=24).contains(&number).then_some(Key::Function(number));
    }
    NAMED_KEYS
        .iter()
        .flat_map(|(_, keys)| keys.iter())
        .find_map(|&(key, code)| key.eq_ignore_ascii_case(&name).then_some(code))
}
