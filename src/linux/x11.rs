use std::{collections::HashMap, error::Error, io};

use mlua::Lua;
use x11rb::{
    connection::Connection,
    protocol::{
        xkb::{self, ConnectionExt as _},
        xproto::{ConnectionExt as _, GrabMode, Keycode, ModMask},
        Event,
    },
};
use xkeysym::key as sym;

use crate::{
    runtime::{Binding, Output},
    shortcut::{Combination, Key, ALT, CONTROL, SHIFT, SUPER},
};

pub(super) fn symbol(key: Key) -> u32 {
    match key {
        Key::Letter(letter) => u32::from(letter),
        Key::Digit(digit) => u32::from(digit),
        Key::Function(n) => sym::F1 + u32::from(n) - 1,
        Key::Numpad(n) => sym::KP_0 + u32::from(n),
        Key::Enter => sym::Return,
        Key::Escape => sym::Escape,
        Key::Space => sym::space,
        Key::Tab => sym::Tab,
        Key::Backspace => sym::BackSpace,
        Key::Delete => sym::Delete,
        Key::Insert => sym::Insert,
        Key::CapsLock => sym::Caps_Lock,
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
    }
}

struct Keyboard {
    first: Keycode,
    width: usize,
    symbols: Vec<u32>,
    modifier_keys: Vec<Keycode>,
}

impl Keyboard {
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
        let code = self.code(symbol(combo.key)).ok_or_else(|| {
            io::Error::other("The key is absent from the current X11 keyboard map.")
        })?;
        let mut mask = 0;
        for (flag, names, label) in [
            (CONTROL, &[sym::Control_L, sym::Control_R], "Ctrl"),
            (ALT, &[sym::Alt_L, sym::Alt_R], "Alt"),
            (SHIFT, &[sym::Shift_L, sym::Shift_R], "Shift"),
            (SUPER, &[sym::Super_L, sym::Super_R], "Win"),
        ] {
            if combo.modifiers & flag != 0 {
                mask |= self.modifier(names).ok_or_else(|| {
                    io::Error::other(format!("{label} is absent from the X11 modifier map."))
                })?;
            }
        }
        Ok((code, mask))
    }
}

pub(super) fn listen(
    lua: &Lua,
    bindings: &[Binding],
    output: Output,
) -> Result<(), Box<dyn Error>> {
    let (connection, screen) = x11rb::connect(None)?;
    let keyboard = Keyboard::new(&connection)?;
    let root = connection.setup().roots[screen].root;
    let locks = u16::from(ModMask::LOCK)
        | keyboard.modifier(&[sym::Num_Lock]).unwrap_or(0)
        | keyboard.modifier(&[sym::Scroll_Lock]).unwrap_or(0);
    let mut active = HashMap::new();
    for binding in bindings {
        let (code, mask) = keyboard.binding(binding.combo).map_err(|error| {
            io::Error::other(format!("Couldn't register {}. {error}", binding.name))
        })?;
        if active.insert((code, mask), binding).is_some() {
            return Err(
                io::Error::other(format!("Duplicate X11 shortcut '{}'.", binding.name)).into(),
            );
        }

        // Each lock state needs a separate grab. A connection drop releases every grab.
        let ignored = locks & !mask;
        let mut subset = ignored;
        loop {
            connection
                .grab_key(
                    false,
                    root,
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
    for binding in bindings {
        output('L', &format!("Bound {}", binding.name));
    }

    let detectable = xkb_repeat(&connection);
    let mut held = HashMap::new();
    loop {
        match connection.wait_for_event()? {
            Event::KeyPress(event) => {
                let mask = u16::from(event.state) & 0xff & !locks;
                if let Some(binding) = active.get(&(event.detail, mask)) {
                    held.entry(event.detail).or_insert(*binding);
                }
            }
            Event::KeyRelease(event) => {
                let code = usize::from(event.detail);
                if !detectable
                    && connection.query_keymap()?.reply()?.keys[code / 8] & (1 << (code % 8)) != 0
                {
                    continue;
                }
                if let Some(binding) = held.remove(&event.detail) {
                    binding.activate(lua, output);
                }
            }
            _ => {}
        }
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
