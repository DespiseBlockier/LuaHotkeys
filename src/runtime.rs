use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
    rc::Rc,
    thread,
    time::Duration,
};

use mlua::{Error, Function, Lua, RegistryKey, String as LuaString, Value, Variadic};

#[cfg(target_os = "linux")]
use crate::linux as platform;
#[cfg(windows)]
use crate::worker as platform;
use crate::{macro_gui::Gui, script_types, shortcut};

pub type Output = fn(char, &str);

pub struct Binding {
    pub name: String,
    pub combo: shortcut::Combination,
    callback: RegistryKey,
}

impl Binding {
    pub fn activate(&self, lua: &Lua, output: Output) {
        let result = lua
            .registry_value::<Function>(&self.callback)
            .and_then(|callback| callback.call::<()>(()));
        if let Err(error) = result {
            output('E', &format!("{} — {error}", self.name));
        }
    }
}

pub(crate) fn strict_text(value: Value) -> mlua::Result<LuaString> {
    match value {
        Value::String(text) => Ok(text),
        _ => Err(Error::runtime("Expected a string.")),
    }
}

pub(crate) fn integer(value: Value) -> mlua::Result<i32> {
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

pub fn run(source: &str, name: &str, output: Output) -> mlua::Result<()> {
    // Macros are local scripts and retain the full Lua 5.1 library in their own process.
    let lua = unsafe { Lua::unsafe_new() };
    let gui = Gui::default();
    gui.install(&lua)?;
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
            let combo = shortcut::parse(&name).map_err(Error::runtime)?;
            platform::validate_hotkey(combo).map_err(Error::runtime)?;
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
            platform::type_text(&strict_text(value)?.to_string_lossy()).map_err(Error::external)
        })?,
    )?;
    globals.set(
        "press",
        lua.create_function(|_, value: Value| {
            let combo = shortcut::parse(&strict_text(value)?.to_str()?).map_err(Error::runtime)?;
            platform::press(combo).map_err(Error::external)
        })?,
    )?;
    globals.set(
        "click",
        lua.create_function(|_, value: Value| {
            let button = match value {
                Value::Nil => "left".to_owned(),
                value => strict_text(value)?.to_str()?.to_owned(),
            };
            platform::click(&button).map_err(Error::external)
        })?,
    )?;
    globals.set(
        "move_mouse",
        lua.create_function(|_, (x, y): (Value, Value)| {
            platform::move_mouse(integer(x)?, integer(y)?).map_err(Error::external)
        })?,
    )?;
    globals.set(
        "mouse_pos",
        lua.create_function(|_, ()| platform::mouse_pos().map_err(Error::external))?,
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
            let args = args
                .into_iter()
                .map(|value| Ok(strict_text(value)?.to_str()?.to_owned()))
                .collect::<mlua::Result<Vec<String>>>()?;
            platform::open(&program, &args).map_err(Error::external)
        })?,
    )?;

    let write = lua.create_function(move |_, text: LuaString| {
        output('L', &text.to_string_lossy());
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
    script_types::compile(&lua, source, name)?.call::<()>(())?;
    loading.set(false);
    gui.start();
    gui.dispatch(&lua, output);
    let bindings = bindings.borrow();
    platform::listen(&lua, &bindings, output, &gui)
}
