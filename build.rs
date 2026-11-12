use std::env;
use winresource::{VersionInfo, WindowsResource};

fn main() {
    println!("cargo:rerun-if-changed=VERSION");
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=assets/LuaHotkeys.ico");
    println!("cargo:rerun-if-env-changed=LUAHOTKEYS_BUILD_VERSION");
    let version = env::var("LUAHOTKEYS_BUILD_VERSION")
        .unwrap_or_else(|_| include_str!("VERSION").trim().to_owned());
    let parts: Vec<u16> = version
        .split('.')
        .map(|part| part.parse().expect("invalid Windows file version"))
        .collect();
    assert_eq!(parts.len(), 3, "expected a three-part version");
    let packed =
        (u64::from(parts[0]) << 48) | (u64::from(parts[1]) << 32) | (u64::from(parts[2]) << 16);
    WindowsResource::new()
        .set("FileDescription", "LuaHotkeys")
        .set("ProductName", "LuaHotkeys")
        .set("FileVersion", &format!("{version}.0"))
        .set("ProductVersion", &version)
        .set_version_info(VersionInfo::FILEVERSION, packed)
        .set_version_info(VersionInfo::PRODUCTVERSION, packed)
        .set_manifest_file("app.manifest")
        .set_icon("assets/LuaHotkeys.ico")
        .compile()
        .expect("compile Windows version resources");
}
