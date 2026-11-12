#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
include!("windows.rs");

#[cfg(not(windows))]
fn main() -> std::process::ExitCode {
    eprintln!("Use your package manager to update LuaHotkeys on Linux.");
    std::process::ExitCode::FAILURE
}
