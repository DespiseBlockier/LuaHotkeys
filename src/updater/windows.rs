#[path = "../common.rs"]
mod common;
mod install;
mod release;
mod window;

use std::{
    ffi::OsString,
    fs, io,
    mem::size_of,
    os::windows::{ffi::OsStringExt, process::CommandExt},
    path::{Path, PathBuf},
    process::Command,
    ptr::null,
    thread,
    time::{Duration, Instant},
};

use windows_sys::{
    core::GUID,
    Win32::{
        Foundation::{ERROR_INVALID_PARAMETER, INVALID_HANDLE_VALUE, WAIT_OBJECT_0, WAIT_TIMEOUT},
        System::{
            Com::CoCreateGuid,
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
                TH32CS_SNAPPROCESS,
            },
            Threading::{
                CreateMutexW, OpenEventW, OpenProcess, QueryFullProcessImageNameW, ReleaseMutex,
                SetEvent, WaitForSingleObject, CREATE_NO_WINDOW, EVENT_MODIFY_STATE,
                PROCESS_QUERY_LIMITED_INFORMATION, SYNCHRONIZATION_SYNCHRONIZE,
            },
        },
        UI::WindowsAndMessaging::MB_ICONERROR,
    },
};

use common::{app_folder, close_event, same_path, try_lock, update_mutex, Handle};
use release::{Release, Version};
use window::Status;

const APP: &str = "LuaHotkeys.exe";
const UPDATER: &str = "LuaHotkeysUpdater.exe";

fn main() {
    match entry() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            window::message(std::ptr::null_mut(), &error.to_string(), MB_ICONERROR);
            std::process::exit(1);
        }
    }
}

fn entry() -> io::Result<i32> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args.as_slice() {
        [] => {
            launch_copy()?;
            Ok(0)
        }
        [action, id, pid] if action == "--cleanup" => {
            let id = run_id(id)?;
            if wait_for_exit(process_id(pid)?)? {
                clean_temp(id);
            }
            Ok(0)
        }
        [action, id, pid, folder] if action == "--apply" => {
            let id = run_id(id)?;
            if !same_path(&app_folder()?, &temp_folder(id)) {
                return Err(io::Error::other(
                    "Open LuaHotkeysUpdater.exe to check for an update.",
                ));
            }
            let folder = std::path::absolute(folder)?;
            let temporary = temp_folder(id);
            let result = if wait_for_exit(process_id(pid)?)? {
                let selected = folder.clone();
                window::run(move |status| update(&selected, &temporary, status))
            } else {
                Err(io::Error::other("Close the other updater and try again."))
            };
            let _ = spawn(
                &folder.join(UPDATER),
                &[
                    "--cleanup".into(),
                    id.into(),
                    std::process::id().to_string().into(),
                ],
            );
            result
        }
        _ => Err(io::Error::other(
            "Open LuaHotkeysUpdater.exe to check for an update.",
        )),
    }
}

fn launch_copy() -> io::Result<()> {
    let folder = app_folder()?;
    if !folder.join(APP).is_file() {
        return Err(io::Error::other(
            "Keep LuaHotkeysUpdater.exe beside LuaHotkeys.exe.",
        ));
    }
    let id = new_id()?;
    let temporary = temp_folder(&id);
    fs::create_dir(&temporary)?;
    let helper = temporary.join(UPDATER);
    let result = fs::copy(std::env::current_exe()?, &helper).and_then(|_| {
        spawn(
            &helper,
            &[
                "--apply".into(),
                id.clone().into(),
                std::process::id().to_string().into(),
                folder.into_os_string(),
            ],
        )
    });
    if result.is_err() {
        clean_temp(&id);
    }
    result
}

fn update(folder: &Path, temporary: &Path, status: Status) -> io::Result<String> {
    let mutex = Handle(unsafe { CreateMutexW(null(), 0, update_mutex(folder).as_ptr()) });
    if mutex.0.is_null() {
        return Err(io::Error::last_os_error());
    }
    if !try_lock(mutex.0) {
        return Ok("LuaHotkeys is already being updated in this folder.".into());
    }
    let _lock = Unlock(mutex);
    let installed = install::installed(folder)?;
    if !installed && install::staging(folder).exists() {
        close_running(folder, status)?;
        install::recover(folder)?;
    }

    let current = Version::from_file(&folder.join(APP))?;
    status.set("Checking for updates…");
    let Some(release) = Release::check(current, installed)? else {
        return Ok(format!("LuaHotkeys {current} is up to date."));
    };

    let package = Download(temporary.join(if installed {
        "update.msi"
    } else {
        "update.zip"
    }));
    status.set(&format!("Downloading LuaHotkeys {}…", release.version));
    release.download(&package.0)?;
    close_running(folder, status)?;
    status.set("Installing the update…");
    if installed {
        if install::msi(folder, &package.0, release.version)? {
            return Ok("Restart Windows to finish updating LuaHotkeys.".into());
        }
    } else {
        install::portable(folder, &package.0, release.version)?;
    }
    Ok(format!("LuaHotkeys was updated to {}.", release.version))
}

fn close_running(folder: &Path, status: Status) -> io::Result<()> {
    let executable = folder.join(APP);
    let processes = app_processes(&executable)?;
    if processes.is_empty() {
        return Ok(());
    }
    status.set("Waiting for LuaHotkeys to close…");
    for pid in processes {
        let event = Handle(unsafe { OpenEventW(EVENT_MODIFY_STATE, 0, close_event(pid).as_ptr()) });
        if !event.0.is_null() {
            unsafe { SetEvent(event.0) };
        }
    }
    let mut began = Instant::now();
    while !app_processes(&executable)?.is_empty() {
        if began.elapsed() >= Duration::from_secs(5) {
            if !status.retry_close() {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "Update canceled.",
                ));
            }
            began = Instant::now();
        }
        thread::sleep(Duration::from_millis(200));
    }
    Ok(())
}

fn app_processes(executable: &Path) -> io::Result<Vec<u32>> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let snapshot = Handle(snapshot);
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut more = unsafe { Process32FirstW(snapshot.0, &mut entry) };
    let mut matches = Vec::new();
    let mut path = vec![0_u16; 32768];
    while more != 0 {
        let end = entry
            .szExeFile
            .iter()
            .position(|&character| character == 0)
            .unwrap_or(entry.szExeFile.len());
        if String::from_utf16_lossy(&entry.szExeFile[..end]).eq_ignore_ascii_case(APP) {
            let process = Handle(unsafe {
                OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, entry.th32ProcessID)
            });
            let mut length = path.len() as u32;
            if !process.0.is_null()
                && unsafe {
                    QueryFullProcessImageNameW(process.0, 0, path.as_mut_ptr(), &mut length)
                } != 0
            {
                let name = OsString::from_wide(&path[..length as usize]);
                if same_path(Path::new(&name), executable) {
                    matches.push(entry.th32ProcessID);
                }
            }
        }
        more = unsafe { Process32NextW(snapshot.0, &mut entry) };
    }
    Ok(matches)
}

fn spawn(program: &Path, arguments: &[OsString]) -> io::Result<()> {
    let folder = program
        .parent()
        .ok_or_else(|| io::Error::other("The updater folder is unavailable."))?;
    Command::new(program)
        .args(arguments)
        .current_dir(folder)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()?;
    Ok(())
}

fn wait_for_exit(pid: u32) -> io::Result<bool> {
    if pid == 0 || pid == std::process::id() {
        return Ok(false);
    }
    let process = Handle(unsafe { OpenProcess(SYNCHRONIZATION_SYNCHRONIZE, 0, pid) });
    if process.0.is_null() {
        let error = io::Error::last_os_error();
        return if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) {
            Ok(true)
        } else {
            Err(error)
        };
    }
    match unsafe { WaitForSingleObject(process.0, 15_000) } {
        WAIT_OBJECT_0 => Ok(true),
        WAIT_TIMEOUT => Ok(false),
        _ => Err(io::Error::last_os_error()),
    }
}

fn new_id() -> io::Result<String> {
    let mut id = GUID::default();
    if unsafe { CoCreateGuid(&mut id) } < 0 {
        return Err(io::Error::other("Could not create a temporary updater ID."));
    }
    let end = id
        .data4
        .into_iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(format!(
        "{:08x}{:04x}{:04x}{end}",
        id.data1, id.data2, id.data3
    ))
}

fn run_id(value: &OsString) -> io::Result<&str> {
    value
        .to_str()
        .filter(|id| id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| io::Error::other("Invalid temporary updater ID."))
}

fn process_id(value: &OsString) -> io::Result<u32> {
    value
        .to_str()
        .and_then(|id| id.parse().ok())
        .ok_or_else(|| io::Error::other("Invalid updater process ID."))
}

fn temp_folder(id: &str) -> PathBuf {
    std::env::temp_dir().join(format!("LuaHotkeys-Update-{id}"))
}

fn clean_temp(id: &str) {
    let temporary = temp_folder(id);
    let _ = fs::remove_file(temporary.join(UPDATER));
    let _ = fs::remove_dir(temporary);
}

struct Download(PathBuf);
impl Drop for Download {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

struct Unlock(Handle);
impl Drop for Unlock {
    fn drop(&mut self) {
        unsafe { ReleaseMutex(self.0 .0) };
    }
}
