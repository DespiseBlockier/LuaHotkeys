#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Key {
    Letter(u8),
    Digit(u8),
    Function(u16),
    Numpad(u16),
    Symbol(u8),
    Modifier(ModifierKey),
    NumpadOperator(NumpadOperator),
    Enter,
    Escape,
    Space,
    Tab,
    Backspace,
    Delete,
    Insert,
    CapsLock,
    NumLock,
    ScrollLock,
    Pause,
    Menu,
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
pub enum ModifierKey {
    LeftCtrl,
    RightCtrl,
    LeftAlt,
    RightAlt,
    LeftShift,
    RightShift,
    LeftWin,
    RightWin,
}

impl ModifierKey {
    pub fn flag(self) -> u32 {
        match self {
            Self::LeftCtrl | Self::RightCtrl => CONTROL,
            Self::LeftAlt | Self::RightAlt => ALT,
            Self::LeftShift | Self::RightShift => SHIFT,
            Self::LeftWin | Self::RightWin => SUPER,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum NumpadOperator {
    Divide,
    Multiply,
    Minus,
    Plus,
    Enter,
    Decimal,
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
            ("NumLock", Key::NumLock),
            ("ScrollLock", Key::ScrollLock),
        ],
    ),
    (
        "Standalone modifier keys",
        &[
            ("Win", Key::Modifier(ModifierKey::LeftWin)),
            ("LeftWin", Key::Modifier(ModifierKey::LeftWin)),
            ("RightWin", Key::Modifier(ModifierKey::RightWin)),
            ("LeftCtrl", Key::Modifier(ModifierKey::LeftCtrl)),
            ("RightCtrl", Key::Modifier(ModifierKey::RightCtrl)),
            ("LeftAlt", Key::Modifier(ModifierKey::LeftAlt)),
            ("RightAlt", Key::Modifier(ModifierKey::RightAlt)),
            ("LeftShift", Key::Modifier(ModifierKey::LeftShift)),
            ("RightShift", Key::Modifier(ModifierKey::RightShift)),
        ],
    ),
    (
        "Punctuation",
        &[
            ("Backslash", Key::Symbol(b'\\')),
            ("Slash", Key::Symbol(b'/')),
            ("Backtick", Key::Symbol(b'\x60')),
            ("Minus", Key::Symbol(b'-')),
            ("Equals", Key::Symbol(b'=')),
            ("LeftBracket", Key::Symbol(b'[')),
            ("RightBracket", Key::Symbol(b']')),
            ("Semicolon", Key::Symbol(b';')),
            ("Apostrophe", Key::Symbol(b'\'')),
            ("Comma", Key::Symbol(b',')),
            ("Period", Key::Symbol(b'.')),
            ("Tilde", Key::Symbol(b'~')),
            ("Exclamation", Key::Symbol(b'!')),
            ("At", Key::Symbol(b'@')),
            ("Hash", Key::Symbol(b'#')),
            ("Dollar", Key::Symbol(b'$')),
            ("Percent", Key::Symbol(b'%')),
            ("Caret", Key::Symbol(b'^')),
            ("Ampersand", Key::Symbol(b'&')),
            ("Asterisk", Key::Symbol(b'*')),
            ("LeftParen", Key::Symbol(b'(')),
            ("RightParen", Key::Symbol(b')')),
            ("Underscore", Key::Symbol(b'_')),
            ("Plus", Key::Symbol(b'+')),
            ("LeftBrace", Key::Symbol(b'{')),
            ("RightBrace", Key::Symbol(b'}')),
            ("Pipe", Key::Symbol(b'|')),
            ("Colon", Key::Symbol(b':')),
            ("Quote", Key::Symbol(b'"')),
            ("LessThan", Key::Symbol(b'<')),
            ("GreaterThan", Key::Symbol(b'>')),
            ("QuestionMark", Key::Symbol(b'?')),
        ],
    ),
    (
        "Numpad operators",
        &[
            ("NumPadDivide", Key::NumpadOperator(NumpadOperator::Divide)),
            (
                "NumPadMultiply",
                Key::NumpadOperator(NumpadOperator::Multiply),
            ),
            ("NumPadMinus", Key::NumpadOperator(NumpadOperator::Minus)),
            ("NumPadPlus", Key::NumpadOperator(NumpadOperator::Plus)),
            ("NumPadEnter", Key::NumpadOperator(NumpadOperator::Enter)),
            (
                "NumPadDecimal",
                Key::NumpadOperator(NumpadOperator::Decimal),
            ),
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
            ("Pause", Key::Pause),
            ("Menu", Key::Menu),
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
    let text = text.trim();
    if text == "+" {
        return Ok(Combination {
            modifiers: 0,
            key: Key::Symbol(b'+'),
        });
    }
    if let Some(modifiers) = text
        .strip_suffix('+')
        .map(str::trim_end)
        .and_then(|text| text.strip_suffix('+'))
    {
        return parse(&format!("{modifiers}+Plus"));
    }
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

impl std::fmt::Display for Combination {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for &(name, _, flag) in MODIFIERS {
            if self.modifiers & flag != 0 {
                write!(f, "{name}+")?;
            }
        }
        match self.key {
            Key::Letter(letter) => write!(f, "{}", char::from(letter.to_ascii_uppercase())),
            Key::Digit(digit) => write!(f, "{}", char::from(digit)),
            Key::Function(number) => write!(f, "F{number}"),
            Key::Numpad(number) => write!(f, "NumPad{number}"),
            Key::Escape => f.write_str("Escape"),
            key => f.write_str(
                NAMED_KEYS
                    .iter()
                    .flat_map(|(_, keys)| *keys)
                    .find_map(|&(name, candidate)| (candidate == key).then_some(name))
                    .expect("named shortcut key"),
            ),
        }
    }
}

fn key(name: &str) -> Option<Key> {
    let name = name.to_ascii_lowercase();
    match name.as_bytes() {
        [letter @ b'a'..=b'z'] => return Some(Key::Letter(*letter)),
        [digit @ b'0'..=b'9'] => return Some(Key::Digit(*digit)),
        [symbol] if symbol.is_ascii_punctuation() => return Some(Key::Symbol(*symbol)),
        _ => {}
    }
    match name.as_str() {
        "windows" | "super" => return Some(Key::Modifier(ModifierKey::LeftWin)),
        "ctrl" | "control" => return Some(Key::Modifier(ModifierKey::LeftCtrl)),
        "alt" => return Some(Key::Modifier(ModifierKey::LeftAlt)),
        "shift" => return Some(Key::Modifier(ModifierKey::LeftShift)),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_full_size_keyboard_names_and_punctuation() {
        assert_eq!(
            parse("Win").unwrap().key,
            Key::Modifier(ModifierKey::LeftWin)
        );
        assert_eq!(parse("Win+J").unwrap().modifiers, SUPER);
        assert_eq!(parse("Ctrl++"), parse("Ctrl+Plus"));
        assert_eq!(parse("Ctrl+#"), parse("Ctrl+Hash"));
        assert_eq!(parse(r"\"), parse("Backslash"));
        for (_, keys) in NAMED_KEYS {
            for (name, expected) in *keys {
                let combo = parse(name).unwrap();
                assert_eq!(combo.key, *expected, "{name}");
                assert_eq!(parse(&combo.to_string()), Ok(combo));
            }
        }
    }
}
