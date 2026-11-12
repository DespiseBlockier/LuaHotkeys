#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod association;
#[cfg(windows)]
mod common;
#[cfg(windows)]
mod completion;
#[cfg(windows)]
mod docs;
#[cfg(windows)]
mod editor;
#[cfg(windows)]
mod gui;
#[cfg(windows)]
mod highlight;
#[cfg(windows)]
mod input;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod lua_syntax;
mod macro_gui;
mod runtime;
mod script_types;
mod shortcut;
#[cfg(windows)]
mod suggestions;
#[cfg(windows)]
mod theme;
#[cfg(windows)]
mod worker;

#[cfg(windows)]
use std::{ffi::OsStr, path::PathBuf, ptr::null_mut};

#[cfg(target_os = "linux")]
fn main() -> std::process::ExitCode {
    linux::main()
}

#[cfg(windows)]
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && args[0] == "--worker" {
        std::process::exit(worker::run());
    }
    if args.len() == 1 && args[0] == "--refresh-associations" {
        association::refresh();
        return;
    }
    let result = (|| {
        if common::is_updating(&common::app_folder()?) {
            return Err(std::io::Error::other(
                "Wait for the LuaHotkeys updater to finish, then open the app again.",
            ));
        }
        let (file, run) = match args.as_slice() {
            [] => (None, false),
            [path] if !path.to_string_lossy().starts_with("--") => {
                let file = PathBuf::from(path);
                let run = association::is_script(&file);
                (Some(file), run)
            }
            [mode, path] if mode == OsStr::new("--edit") || mode == OsStr::new("--run") => {
                (Some(PathBuf::from(path)), mode == OsStr::new("--run"))
            }
            _ => {
                return Err(std::io::Error::other(
                    "Use LuaHotkeys.exe --run <file> or LuaHotkeys.exe --edit <file>.",
                ))
            }
        };
        gui::run(file, run)
    })();
    if let Err(error) = result {
        gui::message(null_mut(), &error.to_string(), "LuaHotkeys", gui::ERROR);
        std::process::exit(1);
    }
}
