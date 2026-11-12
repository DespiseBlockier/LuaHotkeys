use std::{ffi::OsStr, io, path::Path, ptr::null};
use windows_sys::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};
use winreg::{enums::HKEY_CURRENT_USER, RegKey};

const PROG_ID: &str = "LuaHotkeys.Script";
const EXTENSIONS: [&str; 2] = [".lhk", ".luahotkeys"];

pub fn is_script(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("lhk") || extension.eq_ignore_ascii_case("luahotkeys")
        })
}

pub fn refresh() {
    unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED as i32, SHCNF_IDLIST, null(), null()) };
}

pub fn register(executable: &Path) -> io::Result<()> {
    let user = RegKey::predef(HKEY_CURRENT_USER);
    let (classes, _) = user.create_subkey(r"Software\Classes")?;
    let set = |key: &str, name: &str, value: &str| -> io::Result<()> {
        classes.create_subkey(key)?.0.set_value(name, &value)
    };
    let program = executable.to_string_lossy();
    let verb = |key: &str, label: &str, option: &str| -> io::Result<()> {
        set(key, "", label)?;
        set(
            &format!(r"{key}\command"),
            "",
            &format!("\"{program}\" {option} \"%1\""),
        )
    };

    set(PROG_ID, "", "LuaHotkeys Script")?;
    set(
        &format!(r"{PROG_ID}\DefaultIcon"),
        "",
        &format!("\"{program}\",0"),
    )?;
    set(&format!(r"{PROG_ID}\shell"), "", "open")?;
    verb(
        &format!(r"{PROG_ID}\shell\open"),
        "Run with LuaHotkeys",
        "--run",
    )?;
    verb(
        &format!(r"{PROG_ID}\shell\edit"),
        "Edit in LuaHotkeys",
        "--edit",
    )?;

    let app = r"Applications\LuaHotkeys.exe";
    set(app, "FriendlyAppName", "LuaHotkeys")?;
    set(
        &format!(r"{app}\shell\open\command"),
        "",
        &format!("\"{program}\" \"%1\""),
    )?;
    verb(
        &format!(r"{app}\shell\edit"),
        "Edit in LuaHotkeys",
        "--edit",
    )?;
    for extension in EXTENSIONS {
        set(extension, "", PROG_ID)?;
        set(&format!(r"{extension}\OpenWithProgids"), PROG_ID, "")?;
        set(&format!(r"{app}\SupportedTypes"), extension, "")?;
        verb(
            &format!(r"SystemFileAssociations\{extension}\shell\edit"),
            "Edit in LuaHotkeys",
            "--edit",
        )?;
    }
    set(r".lhk\ShellNew", "NullFile", "")?;
    refresh();
    Ok(())
}
