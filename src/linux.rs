mod x11;

use std::{
    env, fs, io,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, ExitCode, Output as ProcessOutput, Stdio},
    thread,
};

use mlua::Lua;

use crate::{
    runtime::{self, Binding, Output},
    shortcut::{Combination, ALT, CONTROL, MODIFIERS, NAMED_KEYS, SHIFT, SUPER},
};

const HELP: &str = "Usage: luahotkeys [--run] <script.lhk>\n       luahotkeys --keys\n       luahotkeys --version\n\nRun a LuaHotkeys script on X11. Press Ctrl+C to stop a running script.";

pub fn main() -> ExitCode {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let path = match args.as_slice() {
        [] => {
            println!("{HELP}");
            return ExitCode::SUCCESS;
        }
        [arg] if arg == "--help" || arg == "-h" => {
            println!("{HELP}");
            return ExitCode::SUCCESS;
        }
        [arg] if arg == "--version" => {
            println!("LuaHotkeys {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        [arg] if arg == "--keys" => {
            print_keys();
            return ExitCode::SUCCESS;
        }
        [arg] if !arg.to_string_lossy().starts_with('-') => PathBuf::from(arg),
        [mode, arg] if mode == "--run" || mode == "--" => PathBuf::from(arg),
        _ => {
            eprintln!("{HELP}");
            return ExitCode::from(2);
        }
    };
    match run(&path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("LuaHotkeys: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let path = fs::canonicalize(path)?;
    let source = fs::read_to_string(&path)?;
    if let Some(folder) = path.parent() {
        env::set_current_dir(folder)?;
    }
    runtime::run(
        source.strip_prefix('\u{feff}').unwrap_or(&source),
        &path.to_string_lossy(),
        |kind, text| match kind {
            'L' => println!("{text}"),
            'E' => eprintln!("{text}"),
            _ => {}
        },
    )?;
    Ok(())
}

fn print_keys() {
    let modifiers = MODIFIERS
        .iter()
        .map(|(name, _, _)| *name)
        .collect::<Vec<_>>();
    println!(
        "Modifiers: {} (Control and Windows are aliases)",
        modifiers.join(", ")
    );
    println!("Separate optional modifiers and one main key with +. Example: Ctrl+Alt+J");
    println!("Keys: A–Z, 0–9, F1–F24, NumPad0–NumPad9");
    for (group, keys) in NAMED_KEYS {
        let names = keys.iter().map(|(name, _)| *name).collect::<Vec<_>>();
        println!("{group}: {}", names.join(", "));
    }
}

pub fn validate_hotkey(_combo: Combination) -> Result<(), &'static str> {
    Ok(())
}

pub fn listen(lua: &Lua, bindings: &[Binding], output: Output) -> mlua::Result<()> {
    if bindings.is_empty() {
        return Ok(());
    }
    require_x11().map_err(mlua::Error::external)?;
    x11::listen(lua, bindings, output).map_err(|error| mlua::Error::runtime(error.to_string()))
}

fn require_x11() -> io::Result<()> {
    let wayland = env::var_os("WAYLAND_DISPLAY").is_some_and(|name| !name.is_empty())
        || env::var_os("XDG_SESSION_TYPE").is_some_and(|kind| kind == "wayland");
    if wayland {
        return Err(io::Error::other(
            "Global hotkeys and input automation require an X11 desktop session in this Linux build.",
        ));
    }
    if env::var_os("DISPLAY").is_none_or(|display| display.is_empty()) {
        return Err(io::Error::other(
            "DISPLAY is unset. Run this script from an X11 desktop session.",
        ));
    }
    Ok(())
}

fn xdotool(args: &[&str]) -> io::Result<ProcessOutput> {
    require_x11()?;
    checked(Command::new("xdotool").args(args).output()?)
}

fn checked(output: ProcessOutput) -> io::Result<ProcessOutput> {
    if output.status.success() {
        return Ok(output);
    }
    let message = String::from_utf8_lossy(&output.stderr);
    Err(io::Error::other(format!(
        "xdotool exited with {}. {}",
        output.status,
        message.trim()
    )))
}

pub fn press(combo: Combination) -> io::Result<()> {
    let mut keys = String::new();
    for (flag, name) in [
        (CONTROL, "ctrl"),
        (ALT, "alt"),
        (SHIFT, "shift"),
        (SUPER, "super"),
    ] {
        if combo.modifiers & flag != 0 {
            keys.push_str(name);
            keys.push('+');
        }
    }
    let symbol = xkeysym::Keysym::new(x11::symbol(combo.key));
    let name = symbol
        .name()
        .ok_or_else(|| io::Error::other("No X11 name for that key."))?;
    if let Some(name) = name.strip_prefix("XF86XK_") {
        keys.push_str("XF86");
        keys.push_str(name);
    } else {
        keys.push_str(name.strip_prefix("XK_").unwrap_or(name));
    }
    xdotool(&["key", "--clearmodifiers", &keys]).map(|_| ())
}

pub fn type_text(text: &str) -> io::Result<()> {
    require_x11()?;
    if text.contains('\0') {
        return Err(io::Error::other("Text to type contains a NUL character."));
    }
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut process = Command::new("xdotool")
        .args(["type", "--clearmodifiers", "--file", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    let result = process
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(text.as_bytes());
    checked(process.wait_with_output()?)?;
    result
}

pub fn click(button: &str) -> io::Result<()> {
    let number = match button.to_ascii_lowercase().as_str() {
        "left" => "1",
        "middle" => "2",
        "right" => "3",
        _ => {
            return Err(io::Error::other(
                "Mouse button must be left, right or middle.",
            ))
        }
    };
    xdotool(&["click", number]).map(|_| ())
}

pub fn move_mouse(x: i32, y: i32) -> io::Result<()> {
    xdotool(&["mousemove", "--", &x.to_string(), &y.to_string()]).map(|_| ())
}

pub fn mouse_pos() -> io::Result<(i32, i32)> {
    let output = xdotool(&["getmouselocation", "--shell"])?;
    let output = String::from_utf8(output.stdout).map_err(io::Error::other)?;
    let coordinate = |prefix| {
        output
            .lines()
            .find_map(|line| line.strip_prefix(prefix))?
            .parse::<i32>()
            .ok()
    };
    coordinate("X=")
        .zip(coordinate("Y="))
        .ok_or_else(|| io::Error::other("Could not read pointer coordinates from xdotool."))
}

pub fn open(program: &str, args: &[String]) -> io::Result<()> {
    let path = fs::canonicalize(program).ok();
    let is_url = program.contains("://") || program.starts_with("mailto:");
    let child = if is_url && args.is_empty() {
        Command::new("xdg-open").arg(program).spawn()?
    } else if let Some(path) = path {
        match Command::new(&path).args(args).spawn() {
            Err(error)
                if args.is_empty()
                    && (path.is_dir() || error.kind() == io::ErrorKind::PermissionDenied) =>
            {
                Command::new("xdg-open").arg(path).spawn()?
            }
            result => result?,
        }
    } else {
        Command::new(program).args(args).spawn()?
    };
    // Reap child processes when the script continues running after launching an application.
    thread::spawn(move || {
        let mut child = child;
        let _ = child.wait();
    });
    Ok(())
}
