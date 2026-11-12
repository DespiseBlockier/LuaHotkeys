use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
    io::{self, BufRead, Read, Write},
    ptr::{null, null_mut},
    rc::Rc,
    thread,
    time::Duration,
};

use base64::{engine::general_purpose::STANDARD, Engine};
use mlua::{Error, Function, Lua, RegistryKey, String as LuaString, Value, Variadic};
use windows_sys::Win32::UI::{
    Input::KeyboardAndMouse::VK_F12,
    Shell::ShellExecuteW,
    WindowsAndMessaging::{GetMessageW, PeekMessageW, MSG, PM_NOREMOVE, SW_SHOWNORMAL, WM_HOTKEY},
};

use crate::{common::wide, input};

struct Binding {
    name: String,
    combo: input::Combination,
    callback: RegistryKey,
}

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
        run_script(&source, &name)?;
        Ok(())
    })();
    if let Err(error) = result {
        message('E', &error.to_string());
        return 1;
    }
    0
}

fn strict_text(value: Value) -> mlua::Result<LuaString> {
    match value {
        Value::String(text) => Ok(text),
        _ => Err(Error::runtime("Expected a string.")),
    }
}

fn integer(value: Value) -> mlua::Result<i32> {
    let value = match value {
        Value::Integer(value) => value as f64,
        Value::Number(value) => value,
        _ => return Err(Error::runtime("Expected an integer.")),
    };
    if value >= f64::from(i32::MIN) && value <= f64::from(i32::MAX) && value == value.trunc() {
        Ok(value as i32)
    } else {
        Err(Error::runtime("Expected a 32-bit integer."))
    }
}

fn run_script(source: &str, name: &str) -> mlua::Result<()> {
    // Macros are local scripts and retain the full Lua 5.1 library in their own process.
    let lua = unsafe { Lua::unsafe_new() };
    let globals = lua.globals();
    let bindings = Rc::new(RefCell::new(Vec::<Binding>::new()));
    let unique = Rc::new(RefCell::new(HashSet::new()));
    let loading = Rc::new(Cell::new(true));
    let (registered, seen, accepting) = (bindings.clone(), unique, loading.clone());
    globals.set(
        "hotkey",
        lua.create_function(move |lua, (name, callback): (Value, Function)| {
            if !accepting.get() {
                return Err(Error::runtime("Declare hotkeys when the script starts."));
            }
            let name = strict_text(name)?.to_str()?.to_owned();
            let combo = input::parse(&name).map_err(Error::runtime)?;
            if combo.key == VK_F12 {
                return Err(Error::runtime(
                    "Windows reserves F12. Choose a different key.",
                ));
            }
            if !seen.borrow_mut().insert(combo) {
                return Err(Error::runtime(format!("Duplicate hotkey '{name}'.")));
            }
            registered.borrow_mut().push(Binding {
                name,
                combo,
                callback: lua.create_registry_value(callback)?,
            });
            Ok(())
        })?,
    )?;

    globals.set(
        "type_text",
        lua.create_function(|_, value: Value| {
            input::type_text(&strict_text(value)?.to_string_lossy()).map_err(Error::external)
        })?,
    )?;
    globals.set(
        "press",
        lua.create_function(|_, value: Value| {
            let combo = input::parse(&strict_text(value)?.to_str()?).map_err(Error::runtime)?;
            input::press(combo).map_err(Error::external)
        })?,
    )?;
    globals.set(
        "click",
        lua.create_function(|_, value: Value| {
            let button = match value {
                Value::Nil => "left".to_owned(),
                value => strict_text(value)?.to_str()?.to_owned(),
            };
            input::click(&button).map_err(Error::external)
        })?,
    )?;
    globals.set(
        "move_mouse",
        lua.create_function(|_, (x, y): (Value, Value)| {
            input::move_mouse(integer(x)?, integer(y)?).map_err(Error::external)
        })?,
    )?;
    globals.set(
        "mouse_pos",
        lua.create_function(|_, ()| input::mouse_pos().map_err(Error::external))?,
    )?;
    globals.set(
        "sleep",
        lua.create_function(|_, value: Value| {
            let milliseconds = u64::try_from(integer(value)?)
                .map_err(|_| Error::runtime("Sleep requires a nonnegative duration."))?;
            thread::sleep(Duration::from_millis(milliseconds));
            Ok(())
        })?,
    )?;
    globals.set(
        "run",
        lua.create_function(|_, (program, args): (Value, Variadic<Value>)| {
            let program = strict_text(program)?.to_str()?.to_owned();
            let arguments = args
                .into_iter()
                .map(|value| Ok(quote(&strict_text(value)?.to_str()?)))
                .collect::<mlua::Result<Vec<String>>>()?
                .join(" ");
            let arguments = wide(&arguments);
            let result = unsafe {
                ShellExecuteW(
                    null_mut(),
                    wide("open").as_ptr(),
                    wide(&program).as_ptr(),
                    if arguments.len() == 1 {
                        null()
                    } else {
                        arguments.as_ptr()
                    },
                    null(),
                    SW_SHOWNORMAL,
                )
            };
            if result as usize <= 32 {
                return Err(Error::runtime(format!(
                    "Windows could not open '{program}' (error {}).",
                    result as usize
                )));
            }
            Ok(())
        })?,
    )?;

    let write = lua.create_function(|_, text: LuaString| {
        message('L', &text.to_string_lossy());
        Ok(())
    })?;
    lua.load(
        r#"
        local write = ...
        local protected, raise, stringify = pcall, error, tostring
        for _, name in ipairs({
            'hotkey', 'type_text', 'press', 'click', 'move_mouse',
            'mouse_pos', 'sleep', 'run'
        }) do
            local callback = _G[name]
            _G[name] = function(...)
                local ok, first, second = protected(callback, ...)
                if not ok then raise(stringify(first), 2) end
                return first, second
            end
        end
        local to_string, concat, count = tostring, table.concat, select
        function log(...)
            local values = {}
            for i = 1, count('#', ...) do
                values[i] = to_string(count(i, ...))
            end
            write(concat(values, '\t'))
        end
        print = log
    "#,
    )
    .set_name("=(LuaHotkeys)")
    .call::<()>(write)?;
    lua.load(source).set_name(format!("@{name}")).exec()?;
    loading.set(false);
    let bindings = bindings.borrow();
    listen(&lua, &bindings)
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

fn listen(lua: &Lua, bindings: &[Binding]) -> mlua::Result<()> {
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
        message('L', &format!("Bound {}", binding.name));
    }
    message('R', &bindings.len().to_string());
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
                    let result = lua
                        .registry_value::<Function>(&binding.callback)
                        .and_then(|callback| callback.call::<()>(()));
                    if let Err(error) = result {
                        message('E', &format!("{} — {error}", binding.name));
                    }
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
