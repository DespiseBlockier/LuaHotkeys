use std::{
    collections::{hash_map::Entry, HashMap},
    error::Error,
    io,
    os::fd::{AsRawFd, RawFd},
};

use mlua::Lua;
use x11rb::{
    connection::Connection,
    protocol::{
        xinput::{self, ConnectionExt as _},
        xkb::{self, ConnectionExt as _},
        xproto::{
            ConnectionExt as _, GrabMode, Keycode, ModMask, BUTTON_PRESS_EVENT,
            BUTTON_RELEASE_EVENT, KEY_PRESS_EVENT, KEY_RELEASE_EVENT,
        },
        xtest::ConnectionExt as _,
        Event,
    },
    rust_connection::RustConnection,
};
use xkeysym::key as sym;

use crate::{
    macro_gui::Gui,
    runtime::{Binding, HotkeyTrigger, Output},
    shortcut::{
        Combination, Key, ModifierKey, MouseButton, NumpadOperator, ALT, CONTROL, NAMED_KEYS,
        SHIFT, SUPER,
    },
};

pub(super) fn symbol(key: Key) -> Option<u32> {
    Some(match key {
        Key::Letter(letter) => u32::from(letter),
        Key::Digit(digit) => u32::from(digit),
        Key::Function(n) => sym::F1 + u32::from(n) - 1,
        Key::Numpad(n) => sym::KP_0 + u32::from(n),
        Key::Symbol(symbol) => u32::from(symbol),
        Key::Mouse(_) => return None,
        Key::Modifier(key) => match key {
            ModifierKey::LeftCtrl => sym::Control_L,
            ModifierKey::RightCtrl => sym::Control_R,
            ModifierKey::LeftAlt => sym::Alt_L,
            ModifierKey::RightAlt => sym::Alt_R,
            ModifierKey::LeftShift => sym::Shift_L,
            ModifierKey::RightShift => sym::Shift_R,
            ModifierKey::LeftWin => sym::Super_L,
            ModifierKey::RightWin => sym::Super_R,
        },
        Key::NumpadOperator(key) => match key {
            NumpadOperator::Divide => sym::KP_Divide,
            NumpadOperator::Multiply => sym::KP_Multiply,
            NumpadOperator::Minus => sym::KP_Subtract,
            NumpadOperator::Plus => sym::KP_Add,
            NumpadOperator::Enter => sym::KP_Enter,
            NumpadOperator::Decimal => sym::KP_Decimal,
        },
        Key::Enter => sym::Return,
        Key::Escape => sym::Escape,
        Key::Space => sym::space,
        Key::Tab => sym::Tab,
        Key::Backspace => sym::BackSpace,
        Key::Delete => sym::Delete,
        Key::Insert => sym::Insert,
        Key::CapsLock => sym::Caps_Lock,
        Key::NumLock => sym::Num_Lock,
        Key::ScrollLock => sym::Scroll_Lock,
        Key::Pause => sym::Pause,
        Key::Menu => sym::Menu,
        Key::Home => sym::Home,
        Key::End => sym::End,
        Key::PageUp => sym::Page_Up,
        Key::PageDown => sym::Page_Down,
        Key::Up => sym::Up,
        Key::Down => sym::Down,
        Key::Left => sym::Left,
        Key::Right => sym::Right,
        Key::PrintScreen => sym::Print,
        Key::VolumeUp => sym::XF86_AudioRaiseVolume,
        Key::VolumeDown => sym::XF86_AudioLowerVolume,
        Key::VolumeMute => sym::XF86_AudioMute,
        Key::MediaPlayPause => sym::XF86_AudioPlay,
    })
}

pub(crate) struct Keyboard {
    first: Keycode,
    width: usize,
    symbols: Vec<u32>,
    modifier_keys: Vec<Keycode>,
}

impl Keyboard {
    pub(crate) fn for_capture() -> Result<Self, Box<dyn Error>> {
        let (connection, _) = x11rb::connect(None)?;
        Self::new(&connection)
    }

    pub(crate) fn capture(&self, code: Keycode, state: u16, released: bool) -> Option<Combination> {
        let start = usize::from(code.checked_sub(self.first)?) * self.width;
        let key = self
            .symbols
            .get(start..start + self.width)?
            .iter()
            .find_map(|&code| match code {
                sym::a..=sym::z | sym::A..=sym::Z => {
                    Some(Key::Letter((code as u8).to_ascii_lowercase()))
                }
                sym::_0..=sym::_9 => Some(Key::Digit(code as u8)),
                sym::F1..=sym::F24 => Some(Key::Function((code - sym::F1 + 1) as u16)),
                sym::KP_0..=sym::KP_9 => Some(Key::Numpad((code - sym::KP_0) as u16)),
                _ => NAMED_KEYS
                    .iter()
                    .flat_map(|(_, keys)| *keys)
                    .find_map(|&(_, key)| (symbol(key) == Some(code)).then_some(key)),
            })?;
        let win = matches!(
            key,
            Key::Modifier(ModifierKey::LeftWin | ModifierKey::RightWin)
        );
        if released != win || matches!(key, Key::Modifier(_)) && !win {
            return None;
        }
        let mut modifiers = 0;
        for (flag, names, _) in MODIFIER_KEYSYMS {
            if self.modifier(names).is_some_and(|mask| state & mask != 0) {
                modifiers |= flag;
            }
        }
        if let Key::Modifier(key) = key {
            modifiers &= !key.flag();
        }
        Some(Combination { modifiers, key })
    }

    fn new(connection: &impl Connection) -> Result<Self, Box<dyn Error>> {
        let setup = connection.setup();
        let first = setup.min_keycode;
        let count = setup.max_keycode - first + 1;
        let map = connection.get_keyboard_mapping(first, count)?.reply()?;
        if map.keysyms_per_keycode == 0 {
            return Err(io::Error::other("X11 returned an empty keyboard map.").into());
        }
        let modifier_keys = connection.get_modifier_mapping()?.reply()?.keycodes;
        Ok(Self {
            first,
            width: usize::from(map.keysyms_per_keycode),
            symbols: map.keysyms,
            modifier_keys,
        })
    }

    fn code(&self, symbol: u32) -> Option<Keycode> {
        self.symbols
            .chunks_exact(self.width)
            .position(|row| row.contains(&symbol))
            .map(|offset| self.first + offset as Keycode)
    }

    fn modifier(&self, symbols: &[u32]) -> Option<u16> {
        let per_modifier = self.modifier_keys.len() / 8;
        if per_modifier == 0 {
            return None;
        }
        self.modifier_keys
            .chunks_exact(per_modifier)
            .position(|group| {
                group.iter().any(|&code| {
                    let Some(offset) = code.checked_sub(self.first) else {
                        return false;
                    };
                    let start = usize::from(offset) * self.width;
                    self.symbols
                        .get(start..start + self.width)
                        .is_some_and(|row| symbols.iter().any(|symbol| row.contains(symbol)))
                })
            })
            .map(|group| 1 << group)
    }

    fn binding(&self, combo: Combination) -> io::Result<(Keycode, u16)> {
        let resolved = if let Key::Symbol(symbol) = combo.key {
            self.symbol_binding(symbol)
        } else {
            symbol(combo.key)
                .and_then(|symbol| self.code(symbol))
                .map(|code| (code, 0))
        };
        let (code, mut mask) = resolved.ok_or_else(|| {
            io::Error::other("The key is absent from the current X11 keyboard map.")
        })?;
        for (flag, names, label) in MODIFIER_KEYSYMS {
            if combo.modifiers & flag != 0 {
                mask |= self.modifier(names).ok_or_else(|| {
                    io::Error::other(format!("{label} is absent from the X11 modifier map."))
                })?;
            }
        }
        Ok((code, mask))
    }

    fn symbol_binding(&self, symbol: u8) -> Option<(Keycode, u16)> {
        let shift = self.modifier(&[sym::Shift_L, sym::Shift_R]);
        let level_three = self.modifier(&[sym::ISO_Level3_Shift, sym::Mode_switch]);
        let levels = [
            Some(0),
            shift,
            level_three,
            shift.zip(level_three).map(|(shift, alt)| shift | alt),
        ];
        self.symbols
            .chunks_exact(self.width)
            .enumerate()
            .filter_map(|(offset, row)| {
                row.iter().enumerate().find_map(|(level, &key)| {
                    if key != u32::from(symbol) {
                        return None;
                    }
                    let index = if level < 4 {
                        level
                    } else if level < 6 {
                        level - 2
                    } else {
                        return None;
                    };
                    levels[index].map(|mask| (self.first + offset as Keycode, mask))
                })
            })
            .min_by_key(|&(_, mask)| mask.count_ones())
    }
}

const MODIFIER_KEYSYMS: [(u32, &[u32], &str); 4] = [
    (CONTROL, &[sym::Control_L, sym::Control_R], "Ctrl"),
    (ALT, &[sym::Alt_L, sym::Alt_R], "Alt"),
    (SHIFT, &[sym::Shift_L, sym::Shift_R], "Shift"),
    (SUPER, &[sym::Super_L, sym::Super_R], "Win"),
];

pub(super) fn click(button: MouseButton, modifiers: u32) -> Result<(), Box<dyn Error>> {
    let (connection, _) = x11rb::connect(None)?;
    let button = match button {
        MouseButton::Left => 1,
        MouseButton::Middle => 2,
        MouseButton::Right => 3,
    };
    let mut held = Vec::new();
    if modifiers != 0 {
        let keyboard = Keyboard::new(&connection)?;
        let pressed = connection.query_keymap()?.reply()?.keys;
        for (flag, names, label) in MODIFIER_KEYSYMS {
            if modifiers & flag == 0 {
                continue;
            }
            let mut codes = names.iter().filter_map(|&name| keyboard.code(name));
            let first = codes.next().ok_or_else(|| {
                io::Error::other(format!("{label} is absent from the X11 keyboard map."))
            })?;
            let is_down = |code: u8| pressed[usize::from(code / 8)] & (1 << (code % 8)) != 0;
            if !std::iter::once(first).chain(codes).any(is_down) {
                held.push(first);
            }
        }
    }
    let events = held
        .iter()
        .map(|&code| (KEY_PRESS_EVENT, code))
        .chain([(BUTTON_PRESS_EVENT, button), (BUTTON_RELEASE_EVENT, button)])
        .chain(held.iter().rev().map(|&code| (KEY_RELEASE_EVENT, code)));
    let pending = events
        .map(|(event, detail)| connection.xtest_fake_input(event, detail, 0, 0, 0, 0, 0))
        .collect::<Result<Vec<_>, _>>()?;
    for request in pending {
        request.check()?;
    }
    Ok(())
}

pub(super) struct Hotkeys<'a> {
    connection: RustConnection,
    root: u32,
    locks: u16,
    registered: bool,
    detectable: bool,
    active: HashMap<(Keycode, u16), &'a Binding>,
    held: HashMap<Keycode, &'a Binding>,
    taps: ModifierTaps<'a>,
}

#[derive(Default)]
struct ModifierTaps<'a> {
    bindings: HashMap<Keycode, &'a Binding>,
    modifiers: Vec<Keycode>,
    down: Vec<Keycode>,
    pending: Option<Keycode>,
}

impl<'a> ModifierTaps<'a> {
    fn press(&mut self, code: Keycode, recording: bool) -> Option<&'a Binding> {
        if self.down.contains(&code) {
            return None;
        }
        let binding = if !recording && self.down.is_empty() {
            self.bindings.get(&code).copied()
        } else {
            None
        };
        self.pending = binding
            .filter(|binding| binding.trigger == HotkeyTrigger::Release)
            .map(|_| code);
        if self.modifiers.contains(&code) {
            self.down.push(code);
        }
        binding.filter(|binding| binding.trigger == HotkeyTrigger::Press)
    }

    fn release(&mut self, code: Keycode) -> Option<&'a Binding> {
        self.down.retain(|&down| down != code);
        if self.pending == Some(code) {
            self.pending = None;
            return self.bindings.get(&code).copied();
        }
        None
    }
}

impl<'a> Hotkeys<'a> {
    pub(super) fn register(
        bindings: &'a [Binding],
        output: Output,
    ) -> Result<Self, Box<dyn Error>> {
        let (connection, screen) = x11rb::connect(None)?;
        let keyboard = Keyboard::new(&connection)?;
        let root = connection.setup().roots[screen].root;
        let locks = u16::from(ModMask::LOCK)
            | keyboard.modifier(&[sym::Num_Lock]).unwrap_or(0)
            | keyboard.modifier(&[sym::Scroll_Lock]).unwrap_or(0);
        let mut active = HashMap::new();
        let mut taps = ModifierTaps::default();
        for binding in bindings {
            let (code, mask) = keyboard.binding(binding.combo).map_err(|error| {
                io::Error::other(format!("Couldn't register {}. {error}", binding.name))
            })?;
            let duplicate = if mask == 0 && matches!(binding.combo.key, Key::Modifier(_)) {
                taps.bindings.insert(code, binding).is_some()
            } else {
                active.insert((code, mask), binding).is_some()
            };
            if duplicate {
                return Err(io::Error::other(format!(
                    "Duplicate X11 shortcut '{}'.",
                    binding.name
                ))
                .into());
            }
        }
        if !taps.bindings.is_empty() {
            // A passive grab on Win itself would steal Win chords from the desktop.
            connection.xinput_xi_query_version(2, 1)?.reply()?;
            connection
                .xinput_xi_select_events(
                    root,
                    &[xinput::EventMask {
                        deviceid: xinput::Device::ALL_MASTER.into(),
                        mask: vec![
                            xinput::XIEventMask::RAW_KEY_PRESS
                                | xinput::XIEventMask::RAW_KEY_RELEASE,
                        ],
                    }],
                )?
                .check()?;
            taps.modifiers = MODIFIER_KEYSYMS
                .iter()
                .flat_map(|&(_, symbols, _)| symbols)
                .filter_map(|&symbol| keyboard.code(symbol))
                .collect();
            let pressed = connection.query_keymap()?.reply()?.keys;
            taps.down
                .extend(taps.modifiers.iter().copied().filter(|&code| {
                    let code = usize::from(code);
                    pressed[code / 8] & (1 << (code % 8)) != 0
                }));
        }
        let mut hotkeys = Self {
            detectable: xkb_repeat(&connection),
            connection,
            root,
            locks,
            registered: false,
            active,
            held: HashMap::new(),
            taps,
        };
        hotkeys.grab()?;
        for binding in bindings {
            output('L', &format!("Bound {}", binding.name));
        }
        Ok(hotkeys)
    }

    fn grab(&mut self) -> Result<(), Box<dyn Error>> {
        for (&(code, mask), binding) in &self.active {
            // Each lock state needs a separate grab. A connection drop releases every grab.
            let ignored = self.locks & !mask;
            let mut subset = ignored;
            loop {
                self.connection
                    .grab_key(
                        false,
                        self.root,
                        (mask | subset).into(),
                        code,
                        GrabMode::ASYNC,
                        GrabMode::ASYNC,
                    )?
                    .check()
                    .map_err(|error| {
                        io::Error::other(format!("Couldn't register {}. {error}", binding.name))
                    })?;
                if subset == 0 {
                    break;
                }
                subset = (subset - 1) & ignored;
            }
        }
        self.registered = true;
        Ok(())
    }

    pub(super) fn update_recording(&mut self, recording: bool) -> Result<bool, Box<dyn Error>> {
        if recording {
            self.taps.pending = None;
            if self.registered {
                // Let the focused picker receive keys that this client normally grabs.
                self.connection
                    .ungrab_key(0, self.root, ModMask::ANY)?
                    .check()?;
                self.held.clear();
                self.registered = false;
            }
            return Ok(false);
        }
        if !self.registered {
            let pressed = self.connection.query_keymap()?.reply()?.keys;
            let awaiting_release = self.active.keys().any(|&(code, _)| {
                let code = usize::from(code);
                pressed[code / 8] & (1 << (code % 8)) != 0
            });
            if awaiting_release {
                return Ok(true);
            }
            self.grab()?;
        }
        Ok(false)
    }

    pub(super) fn fd(&self) -> RawFd {
        self.connection.stream().as_raw_fd()
    }

    pub(super) fn drain(
        &mut self,
        lua: &Lua,
        output: Output,
        gui: &Gui,
    ) -> Result<(), Box<dyn Error>> {
        while let Some(event) = self.connection.poll_for_event()? {
            self.dispatch(event, lua, output, gui)?;
        }
        Ok(())
    }

    pub(super) fn wait(
        &mut self,
        lua: &Lua,
        output: Output,
        gui: &Gui,
    ) -> Result<(), Box<dyn Error>> {
        self.dispatch(self.connection.wait_for_event()?, lua, output, gui)
    }

    fn dispatch(
        &mut self,
        event: Event,
        lua: &Lua,
        output: Output,
        gui: &Gui,
    ) -> Result<(), Box<dyn Error>> {
        match event {
            Event::XinputRawKeyPress(event) => {
                if u32::from(event.flags & xinput::KeyEventFlags::KEY_REPEAT) == 0 {
                    if let Ok(code) = Keycode::try_from(event.detail) {
                        if let Some(binding) = self.taps.press(code, !self.registered) {
                            if !gui.record_key(binding.combo) {
                                binding.activate(lua, output);
                            }
                        }
                    }
                }
            }
            Event::XinputRawKeyRelease(event) => {
                if let Ok(code) = Keycode::try_from(event.detail) {
                    if let Some(binding) = self.taps.release(code) {
                        if self.registered && !gui.record_key(binding.combo) {
                            binding.activate(lua, output);
                        }
                    }
                }
            }
            Event::KeyPress(event) if self.registered => {
                self.held.retain(|&code, binding| {
                    code == event.detail || !matches!(binding.combo.key, Key::Modifier(_))
                });
                let mask = u16::from(event.state) & 0xff & !self.locks;
                if let (Some(&binding), Entry::Vacant(held)) = (
                    self.active.get(&(event.detail, mask)),
                    self.held.entry(event.detail),
                ) {
                    held.insert(binding);
                    if binding.trigger == HotkeyTrigger::Press && !gui.record_key(binding.combo) {
                        binding.activate(lua, output);
                    }
                }
            }
            Event::KeyRelease(event) => {
                let code = usize::from(event.detail);
                if !self.detectable
                    && self.connection.query_keymap()?.reply()?.keys[code / 8] & (1 << (code % 8))
                        != 0
                {
                    return Ok(());
                }
                if let Some(binding) = self.held.remove(&event.detail) {
                    if binding.trigger == HotkeyTrigger::Release && !gui.record_key(binding.combo) {
                        binding.activate(lua, output);
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

fn xkb_repeat(connection: &impl Connection) -> bool {
    let use_xkb = connection
        .xkb_use_extension(1, 0)
        .ok()
        .and_then(|cookie| cookie.reply().ok());
    if !use_xkb.is_some_and(|reply| reply.supported) {
        return false;
    }
    let flag = xkb::PerClientFlag::DETECTABLE_AUTO_REPEAT;
    let none = xkb::BoolCtrl::from(0_u32);
    connection
        .xkb_per_client_flags(xkb::ID::USE_CORE_KBD.into(), flag, flag, none, none, none)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .is_some_and(|reply| u32::from(reply.value) & u32::from(flag) != 0)
}
