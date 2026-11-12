use std::{
    ffi::c_void,
    fmt,
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    mem::size_of,
    path::Path,
    ptr::{null, null_mut},
};

use serde::Deserialize;
use sha2::{Digest, Sha256};
use windows_sys::Win32::{
    Networking::WinHttp::*,
    Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW, VS_FIXEDFILEINFO,
    },
};

use crate::common::wide;

pub const MAX_ASSET: u64 = 32 * 1024 * 1024;
const REPOSITORY: &str = "DespiseBlockier/LuaHotkeys";

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Version([u16; 3]);

impl Version {
    pub fn from_file(path: &Path) -> io::Result<Self> {
        let name = wide(path);
        let size = unsafe { GetFileVersionInfoSizeW(name.as_ptr(), null_mut()) };
        if size == 0 {
            return if path.is_file() {
                Ok(Self([0, 0, 0]))
            } else {
                Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "LuaHotkeys.exe was not found.",
                ))
            };
        }
        let mut bytes = vec![0_u8; size as usize];
        checked(unsafe { GetFileVersionInfoW(name.as_ptr(), 0, size, bytes.as_mut_ptr().cast()) })?;
        let mut fixed: *mut c_void = null_mut();
        let mut length = 0;
        checked(unsafe {
            VerQueryValueW(
                bytes.as_ptr().cast(),
                wide("\\").as_ptr(),
                &mut fixed,
                &mut length,
            )
        })?;
        if fixed.is_null() || length < size_of::<VS_FIXEDFILEINFO>() as u32 {
            return Err(io::Error::other("The application version is unavailable."));
        }
        let fixed = unsafe { fixed.cast::<VS_FIXEDFILEINFO>().read_unaligned() };
        Ok(Self([
            (fixed.dwFileVersionMS >> 16) as u16,
            fixed.dwFileVersionMS as u16,
            (fixed.dwFileVersionLS >> 16) as u16,
        ]))
    }

    fn tag(value: &str) -> Option<Self> {
        let mut parts = value.strip_prefix('v')?.split('.');
        let version = Self([
            parts.next()?.parse().ok()?,
            parts.next()?.parse().ok()?,
            parts.next()?.parse().ok()?,
        ]);
        (value == format!("v{version}")).then_some(version)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [major, minor, patch] = self.0;
        write!(formatter, "{major}.{minor}.{patch}")
    }
}

#[derive(Deserialize)]
struct Metadata {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    size: u64,
    digest: Option<String>,
    state: String,
}

pub struct Release {
    pub version: Version,
    asset: Asset,
}

impl Release {
    pub fn check(current: Version, installed: bool) -> io::Result<Option<Self>> {
        let url = format!("https://api.github.com/repos/{REPOSITORY}/releases/latest");
        let response = Response::open(&url, true)?;
        let mut body = Vec::new();
        response.take(2 * 1024 * 1024 + 1).read_to_end(&mut body)?;
        if body.len() > 2 * 1024 * 1024 {
            return Err(io::Error::other(
                "GitHub returned too much release metadata.",
            ));
        }
        Self::parse(&body, current, installed)
    }

    fn parse(body: &[u8], current: Version, installed: bool) -> io::Result<Option<Self>> {
        let metadata: Metadata = serde_json::from_slice(body).map_err(io::Error::other)?;
        let version = Version::tag(&metadata.tag_name)
            .filter(|_| !metadata.draft && !metadata.prerelease)
            .ok_or_else(|| {
                io::Error::other("GitHub returned an invalid stable release version.")
            })?;
        if version <= current {
            return Ok(None);
        }
        let suffix = if installed { "x86.msi" } else { "win-x86.zip" };
        let name = format!("LuaHotkeys-{version}-{suffix}");
        let mut matches = metadata
            .assets
            .into_iter()
            .filter(|asset| asset.name == name);
        let asset = matches
            .next()
            .filter(|asset| {
                matches.next().is_none()
                    && asset.state == "uploaded"
                    && (1..=MAX_ASSET).contains(&asset.size)
                    && asset
                        .digest
                        .as_deref()
                        .and_then(|digest| digest.strip_prefix("sha256:"))
                        .is_some_and(|hash| {
                            hash.len() == 64
                                && hash.bytes().all(|byte| {
                                    byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
                                })
                        })
            })
            .ok_or_else(|| {
                io::Error::other(format!(
                    "The latest release does not have a verified {name} download."
                ))
            })?;
        Ok(Some(Self { version, asset }))
    }

    pub fn download(&self, destination: &Path) -> io::Result<()> {
        let url = format!(
            "https://github.com/{REPOSITORY}/releases/download/v{}/{}",
            self.version, self.asset.name
        );
        let response = Response::open(&url, false)?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)?;
        let size = io::copy(&mut response.take(self.asset.size + 1), &mut output)?;
        output.flush()?;
        drop(output);
        if size != self.asset.size {
            return Err(io::Error::other(
                "The update download has an unexpected size.",
            ));
        }
        let mut file = File::open(destination)?;
        let mut sha = Sha256::new();
        io::copy(&mut file, &mut sha)?;
        let expected = format!("sha256:{:x}", sha.finalize());
        if self.asset.digest.as_deref() != Some(expected.as_str()) {
            return Err(io::Error::other(
                "The update download failed SHA-256 verification.",
            ));
        }
        Ok(())
    }
}

struct Internet(*mut c_void);

impl Internet {
    fn new(handle: *mut c_void) -> io::Result<Self> {
        if handle.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(handle))
        }
    }
}

impl Drop for Internet {
    fn drop(&mut self) {
        unsafe { WinHttpCloseHandle(self.0) };
    }
}

struct Response {
    request: Internet,
    _connection: Internet,
    _session: Internet,
}

impl Response {
    fn open(address: &str, api: bool) -> io::Result<Self> {
        let mut address = address.to_owned();
        for _ in 0..6 {
            let (host, path) = address
                .strip_prefix("https://")
                .and_then(|rest| rest.split_once('/'))
                .ok_or_else(|| {
                    io::Error::other("GitHub returned an unexpected download location.")
                })?;
            let allowed = if api {
                host == "api.github.com"
            } else {
                matches!(
                    host,
                    "github.com"
                        | "release-assets.githubusercontent.com"
                        | "objects.githubusercontent.com"
                        | "github-releases.githubusercontent.com"
                )
            };
            if !allowed {
                return Err(io::Error::other(
                    "GitHub returned an unexpected download host.",
                ));
            }
            let session = Internet::new(unsafe {
                WinHttpOpen(
                    wide("LuaHotkeys-Updater").as_ptr(),
                    WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                    null(),
                    null(),
                    0,
                )
            })?;
            checked(unsafe { WinHttpSetTimeouts(session.0, 20_000, 20_000, 20_000, 20_000) })?;
            let connection =
                Internet::new(unsafe { WinHttpConnect(session.0, wide(host).as_ptr(), 443, 0) })?;
            let request = Internet::new(unsafe {
                WinHttpOpenRequest(
                    connection.0,
                    wide("GET").as_ptr(),
                    wide(format!("/{path}")).as_ptr(),
                    null(),
                    null(),
                    null(),
                    WINHTTP_FLAG_SECURE,
                )
            })?;
            let policy = WINHTTP_OPTION_REDIRECT_POLICY_NEVER;
            checked(unsafe {
                WinHttpSetOption(
                    request.0,
                    WINHTTP_OPTION_REDIRECT_POLICY,
                    (&policy as *const u32).cast(),
                    size_of::<u32>() as u32,
                )
            })?;
            let headers = if api {
                "Accept: application/vnd.github+json\r\nX-GitHub-Api-Version: 2022-11-28\r\n"
            } else {
                "Accept: application/octet-stream\r\n"
            };
            checked(unsafe {
                WinHttpSendRequest(request.0, wide(headers).as_ptr(), u32::MAX, null(), 0, 0, 0)
            })?;
            checked(unsafe { WinHttpReceiveResponse(request.0, null_mut()) })?;
            let mut status = 0_u32;
            let mut size = size_of::<u32>() as u32;
            checked(unsafe {
                WinHttpQueryHeaders(
                    request.0,
                    WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                    null(),
                    (&mut status as *mut u32).cast(),
                    &mut size,
                    null_mut(),
                )
            })?;
            if status == 200 {
                return Ok(Self {
                    request,
                    _connection: connection,
                    _session: session,
                });
            }
            if !(300..400).contains(&status) {
                return Err(io::Error::other(format!("GitHub returned HTTP {status}.")));
            }
            let mut location = vec![0_u16; 8192];
            let mut size = (location.len() * 2) as u32;
            checked(unsafe {
                WinHttpQueryHeaders(
                    request.0,
                    WINHTTP_QUERY_LOCATION,
                    null(),
                    location.as_mut_ptr().cast(),
                    &mut size,
                    null_mut(),
                )
            })?;
            let next =
                String::from_utf16_lossy(&location[..(size as usize / 2).min(location.len())]);
            let next = next.trim_end_matches('\0');
            address = if next.starts_with('/') && !next.starts_with("//") {
                format!("https://{host}{next}")
            } else {
                next.to_owned()
            };
        }
        Err(io::Error::other(
            "GitHub redirected the download too many times.",
        ))
    }
}

impl Read for Response {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let mut bytes = 0;
        checked(unsafe {
            WinHttpReadData(
                self.request.0,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut bytes,
            )
        })?;
        Ok(bytes as usize)
    }
}

fn checked(result: i32) -> io::Result<()> {
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
