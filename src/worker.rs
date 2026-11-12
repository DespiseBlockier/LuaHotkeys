use std::{
    cell::RefCell,
    io::{self, BufRead, Read, Write},
    ptr::{null, null_mut},
    rc::Rc,
    thread,
};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use mlua::{Error, Lua};
use windows_sys::Win32::UI::{
    Shell::ShellExecuteW,
    WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, IsDialogMessageW, PeekMessageW, TranslateMessage, MSG,
        PM_NOREMOVE, SW_SHOWNORMAL, WM_HOTKEY,
    },
};
use winit::{event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows};

use crate::{
    common::wide,
    input,
    macro_gui::{immediate, windows::NativeWindow, Engine, Gui},
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

pub fn listen(lua: &Lua, bindings: &[Binding], output: Output, gui: &Gui) -> mlua::Result<()> {
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
    gui.updates();
    let window = match gui.engine() {
        Some(Engine::Basic) => Some(
            NativeWindow::new(gui.window().as_ref().expect("Basic GUI"))
                .map_err(Error::external)?,
        ),
        Some(Engine::Imgui) => {
            let pending = Rc::new(RefCell::new(Vec::new()));
            let messages = pending.clone();
            let mut builder = EventLoop::builder();
            builder.with_msg_hook(move |message| {
                let message = unsafe { &*message.cast::<MSG>() };
                if message.message != WM_HOTKEY {
                    return false;
                }
                messages.borrow_mut().push(message.wParam);
                true
            });
            immediate::run(lua, gui, output, builder, || {
                for id in std::mem::take(&mut *pending.borrow_mut()) {
                    activate(lua, bindings, output, id, gui);
                }
                Ok(())
            })
            .map_err(|error| Error::runtime(error.to_string()))?;
            None
        }
        None => None,
    };
    loop {
        let handle = window.as_ref().map_or(null_mut(), NativeWindow::handle);
        if handle.is_null() && bindings.is_empty() {
            return Ok(());
        }
        let filter = if handle.is_null() { WM_HOTKEY } else { 0 };
        match unsafe { GetMessageW(&mut windows_message, null_mut(), filter, filter) } {
            -1 => return Err(Error::external(io::Error::last_os_error())),
            0 => return Ok(()),
            _ => {
                if windows_message.message == WM_HOTKEY {
                    activate(lua, bindings, output, windows_message.wParam, gui);
                } else if !window
                    .as_ref()
                    .is_some_and(|window| window.capture_key(&windows_message, gui))
                    && unsafe { IsDialogMessageW(handle, &windows_message) } == 0
                {
                    unsafe {
                        TranslateMessage(&windows_message);
                        DispatchMessageW(&windows_message);
                    }
                }
                if let Some(window) = &window {
                    for event in window.events() {
                        gui.event(event);
                        gui.dispatch(lua, output);
                        window.sync(gui);
                    }
                    gui.dispatch(lua, output);
                    window.sync(gui);
                }
            }
        }
    }
}

fn activate(lua: &Lua, bindings: &[Binding], output: Output, id: usize, gui: &Gui) {
    if let Some(binding) = id.checked_sub(1).and_then(|index| bindings.get(index)) {
        input::wait_for_release(binding.combo);
        if !gui.record_key(binding.combo) {
            binding.activate(lua, output);
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
