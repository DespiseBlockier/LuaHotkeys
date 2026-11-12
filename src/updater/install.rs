use std::{
    fs::{self, File, OpenOptions},
    io,
    os::windows::{ffi::OsStringExt, fs::MetadataExt, process::CommandExt},
    path::{Path, PathBuf},
    process::Command,
    ptr::{null, null_mut},
};

use windows_sys::Win32::{
    Storage::FileSystem::{ReplaceFileW, FILE_ATTRIBUTE_REPARSE_POINT},
    System::{
        ApplicationInstallationAndServicing::{
            MsiCloseHandle, MsiDatabaseOpenViewW, MsiOpenDatabaseW, MsiRecordGetStringW,
            MsiViewExecute, MsiViewFetch, MSIHANDLE,
        },
        SystemInformation::GetSystemDirectoryW,
        Threading::CREATE_NO_WINDOW,
    },
};
use winreg::{enums::HKEY_CURRENT_USER, RegKey};
use zip::ZipArchive;

use crate::{
    common::{same_path, wide},
    release::{Version, MAX_ASSET},
};

const UPGRADE_CODE: &str = "{54B8D0E7-0923-46B0-8694-7F3280300F48}";
const FILES: [&str; 4] = [
    "lua5.1.dll",
    "Lua-LICENSE.txt",
    "LuaHotkeysUpdater.exe",
    "LuaHotkeys.exe",
];
const MARKER: &str = "LuaHotkeys portable update";

pub fn installed(folder: &Path) -> io::Result<bool> {
    let user = RegKey::predef(HKEY_CURRENT_USER);
    let location: io::Result<String> = user
        .open_subkey(r"Software\LuaHotkeys\Installer")
        .and_then(|key| key.get_value("InstallFolder"));
    match location {
        Ok(location) => Ok(same_path(folder, Path::new(&location))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

pub fn msi(folder: &Path, package: &Path, version: Version) -> io::Result<bool> {
    if !msi_property(package, "UpgradeCode")?.eq_ignore_ascii_case(UPGRADE_CODE)
        || msi_property(package, "ProductVersion")? != version.to_string()
    {
        return Err(io::Error::other(
            "The installer does not match this LuaHotkeys release.",
        ));
    }
    let mut directory = vec![0_u16; 32768];
    let length =
        unsafe { GetSystemDirectoryW(directory.as_mut_ptr(), directory.len() as u32) } as usize;
    if length == 0 || length >= directory.len() {
        return Err(io::Error::last_os_error());
    }
    let directory = PathBuf::from(std::ffi::OsString::from_wide(&directory[..length]));
    let status = Command::new(directory.join("msiexec.exe"))
        .arg("/i")
        .arg(package)
        .args(["/quiet", "/norestart"])
        .creation_flags(CREATE_NO_WINDOW)
        .status()?;
    if status.code() == Some(3010) {
        return Ok(true);
    }
    if !status.success() {
        return Err(io::Error::other(format!(
            "Windows Installer returned {status}."
        )));
    }
    if Version::from_file(&folder.join("LuaHotkeys.exe"))? != version || !installed(folder)? {
        return Err(io::Error::other(
            "Windows Installer did not update LuaHotkeys in the expected folder.",
        ));
    }
    Ok(false)
}

pub fn msi_property(package: &Path, property: &str) -> io::Result<String> {
    let package = wide(package);
    let database =
        Msi::new(|handle| unsafe { MsiOpenDatabaseW(package.as_ptr(), null(), handle) })?;
    let query = wide(format!("SELECT \u{60}Value\u{60} FROM \u{60}Property\u{60} WHERE \u{60}Property\u{60}='{property}'"));
    let view =
        Msi::new(|handle| unsafe { MsiDatabaseOpenViewW(database.0, query.as_ptr(), handle) })?;
    checked(unsafe { MsiViewExecute(view.0, 0) })?;
    let record = Msi::new(|handle| unsafe { MsiViewFetch(view.0, handle) })?;
    let mut value = [0_u16; 128];
    let mut length = value.len() as u32;
    checked(unsafe { MsiRecordGetStringW(record.0, 1, value.as_mut_ptr(), &mut length) })?;
    Ok(String::from_utf16_lossy(&value[..length as usize]))
}

struct Msi(MSIHANDLE);
impl Msi {
    fn new(open: impl FnOnce(&mut MSIHANDLE) -> u32) -> io::Result<Self> {
        let mut handle = 0;
        checked(open(&mut handle))?;
        Ok(Self(handle))
    }
}
impl Drop for Msi {
    fn drop(&mut self) {
        unsafe { MsiCloseHandle(self.0) };
    }
}

fn checked(result: u32) -> io::Result<()> {
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "The update installer could not be validated (Windows error {result})."
        )))
    }
}

pub fn staging(folder: &Path) -> PathBuf {
    folder.join(".luahotkeys-update")
}

pub fn recover(folder: &Path) -> io::Result<()> {
    let stage = staging(folder);
    let metadata = match fs::symlink_metadata(&stage) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::other(format!(
            "Check the update staging path {}",
            stage.display()
        )));
    }
    let pending = stage.join("pending");
    if !pending.is_file() {
        if stage.read_dir()?.next().is_none() {
            return fs::remove_dir(&stage);
        }
        return Err(io::Error::other(format!(
            "The update staging folder already exists. Check {}",
            stage.display()
        )));
    }
    if fs::read_to_string(&pending)? != MARKER {
        return Err(io::Error::other(format!(
            "Unrecognized update staging folder. Check {}",
            stage.display()
        )));
    }
    if !stage.join("complete").is_file() {
        for name in FILES.into_iter().rev() {
            let destination = folder.join(name);
            let backup = stage.join(format!("{name}.old"));
            if backup.is_file() {
                if destination.exists() {
                    replace(&destination, &backup, None)?;
                } else {
                    fs::rename(&backup, &destination)?;
                }
            } else if stage.join(format!("{name}.absent")).is_file() {
                remove_file(&destination)?;
            }
        }
    }
    for name in FILES {
        remove_file(&stage.join(name))?;
        remove_file(&stage.join(format!("{name}.old")))?;
        remove_file(&stage.join(format!("{name}.absent")))?;
    }
    remove_file(&stage.join("complete"))?;
    remove_file(&pending)?;
    fs::remove_dir(&stage)
}

pub fn portable(folder: &Path, package: &Path, version: Version) -> io::Result<()> {
    let stage = staging(folder);
    fs::create_dir(&stage)?;
    fs::write(stage.join("pending"), MARKER)?;
    let result = (|| {
        extract(package, &stage)?;
        for name in ["LuaHotkeys.exe", "LuaHotkeysUpdater.exe"] {
            if Version::from_file(&stage.join(name))? != version {
                return Err(io::Error::other(
                    "The portable build does not match the release version.",
                ));
            }
        }
        for name in FILES {
            let destination = folder.join(name);
            let source = stage.join(name);
            if destination.exists() {
                replace(
                    &destination,
                    &source,
                    Some(&stage.join(format!("{name}.old"))),
                )?;
            } else {
                fs::write(stage.join(format!("{name}.absent")), MARKER)?;
                fs::rename(&source, &destination)?;
            }
        }
        fs::write(stage.join("complete"), MARKER)
    })();
    if let Err(error) = result {
        recover(folder).map_err(|recovery| {
            io::Error::other(format!(
                "Could not restore the previous build. Its files are backed up in {}. {recovery}",
                stage.display()
            ))
        })?;
        return Err(error);
    }
    recover(folder)
}

fn extract(package: &Path, stage: &Path) -> io::Result<()> {
    let mut archive = ZipArchive::new(File::open(package)?).map_err(io::Error::other)?;
    let mut found = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(io::Error::other)?;
        let Some(name) = FILES.iter().copied().find(|&name| name == entry.name()) else {
            continue;
        };
        if !entry.is_file()
            || entry.size() == 0
            || entry.size() > MAX_ASSET
            || found.contains(&name)
        {
            return Err(io::Error::other(format!(
                "The portable update has an invalid {name} file."
            )));
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(stage.join(name))?;
        io::copy(&mut entry, &mut file)?;
        found.push(name);
    }
    if found.len() != FILES.len() {
        return Err(io::Error::other(
            "The portable update is missing a program file.",
        ));
    }
    Ok(())
}

fn replace(destination: &Path, source: &Path, backup: Option<&Path>) -> io::Result<()> {
    let backup = backup.map(wide);
    if unsafe {
        ReplaceFileW(
            wide(destination).as_ptr(),
            wide(source).as_ptr(),
            backup.as_ref().map_or(null(), |path| path.as_ptr()),
            0,
            null_mut(),
            null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn remove_file(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}
