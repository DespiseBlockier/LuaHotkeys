use std::{
    io::{self, BufRead, Read, Write},
    ptr::{null, null_mut},
    thread,
};

use base64::{engine::general_purpose::STANDARD, Engine};
use mlua::{Error, Lua};
use windows_sys::Win32::UI::{
    Shell::ShellExecuteW,
    WindowsAndMessaging::{GetMessageW, PeekMessageW, MSG, PM_NOREMOVE, SW_SHOWNORMAL, WM_HOTKEY},
};

use crate::{
    common::wide,
    input,
    runtime::{self, Binding, Output},
    shortcut::{Combination, Key},
};

pub use crate::input::{click, mouse_pos, move_mouse, press, type_text};

fn message(kind: char, content: &str) {
    let end = content
        .char_indices()
        .nth(12000)
        .map_or(content.len(), |(index, _)| index);
    let mut stdout = io::stdout().lock();
    let _ = writeln!(
        stdout,
        "{kind} {}",
        STANDARD.encode(&content.as_bytes()[..end])
    );
    let _ = stdout.flush();
}

fn read_line(stdin: &mut impl BufRead) -> io::Result<String> {
    let mut line = String::new();
    if stdin.read_line(&mut line)? == 0 {
        return Err(io::Error::other("The script input closed."));
    }
    let bytes = STANDARD
        .decode(line.trim_end_matches(['\r', '\n']))
        .map_err(io::Error::other)?;
    String::from_utf8(bytes).map_err(io::Error::other)
}

pub fn run() -> i32 {
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let mut stdin = io::stdin().lock();
        let name = read_line(&mut stdin)?;
        let source = read_line(&mut stdin)?;
        drop(stdin);
        thread::spawn(|| {
            let _ = io::stdin().read(&mut [0_u8; 1]);
            std::process::exit(0);
        });
        runtime::run(&source, &name, message)?;
        Ok(())
    })();
    if let Err(error) = result {
        message('E', &error.to_string());
        return 1;
    }
    0
}

pub fn validate_hotkey(combo: Combination) -> Result<(), &'static str> {
    if combo.key == Key::Function(12) {
        return Err("Windows reserves F12. Choose a different key.");
    }
    Ok(())
}

pub fn open(program: &str, args: &[String]) -> io::Result<()> {
    let arguments = args
        .iter()
        .map(|arg| quote(arg))
        .collect::<Vec<_>>()
        .join(" ");
    let arguments = wide(&arguments);
    let result = unsafe {
        ShellExecuteW(
            null_mut(),
            wide("open").as_ptr(),
            wide(program).as_ptr(),
            if args.is_empty() {
                null()
            } else {
                arguments.as_ptr()
            },
            null(),
            SW_SHOWNORMAL,
        )
    };
    if result as usize <= 32 {
        return Err(io::Error::other(format!(
            "Windows could not open '{program}' (error {}).",
            result as usize
        )));
    }
    Ok(())
}

fn quote(value: &str) -> String {
    let mut quoted = String::from("\"");
    let mut backslashes = 0;
    for character in value.chars() {
        if character == '\\' {
            backslashes += 1;
        } else {
            let count = if character == '"' {
                backslashes * 2 + 1
            } else {
                backslashes
            };
            quoted.extend(std::iter::repeat_n('\\', count));
            quoted.push(character);
            backslashes = 0;
        }
    }
    quoted.extend(std::iter::repeat_n('\\', backslashes * 2));
    quoted.push('"');
    quoted
}

pub fn listen(lua: &Lua, bindings: &[Binding], output: Output) -> mlua::Result<()> {
    let mut windows_message = MSG::default();
    unsafe { PeekMessageW(&mut windows_message, null_mut(), 0, 0, PM_NOREMOVE) };
    let mut registered = Registered(0);
    for binding in bindings {
        input::register(registered.0 + 1, binding.combo).map_err(|error| {
            Error::runtime(format!("Couldn't register {}. {error}", binding.name))
        })?;
        registered.0 += 1;
    }
    for binding in bindings {
        output('L', &format!("Bound {}", binding.name));
    }
    output('R', &bindings.len().to_string());
    if bindings.is_empty() {
        return Ok(());
    }
    loop {
        match unsafe { GetMessageW(&mut windows_message, null_mut(), WM_HOTKEY, WM_HOTKEY) } {
            -1 => return Err(Error::external(io::Error::last_os_error())),
            0 => return Ok(()),
            _ => {
                if let Some(binding) = windows_message
                    .wParam
                    .checked_sub(1)
                    .and_then(|index| bindings.get(index))
                {
                    input::wait_for_release(binding.combo);
                    binding.activate(lua, output);
                }
            }
        }
    }
}

struct Registered(i32);

impl Drop for Registered {
    fn drop(&mut self) {
        for id in 1..=self.0 {
            input::unregister(id);
        }
    }
}
